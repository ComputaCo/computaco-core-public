//! Capturing a reference trajectory: the real application in a private virtual
//! display, driven through xdotool, observed through AT-SPI, xwd and the sandbox home.
use crate::atspi::AtSpi;
use crate::procedure::{plan, stage_sandbox, target_of, Plan};
use crate::resolve::resolve;
use crate::store::Store;
use crate::targetmap::{Mapped, TargetMap};
use crate::util::{err, now_iso, progress, run, walk, which, Result};
use crate::vdisplay::VirtualDisplay;
use crate::xinput::XInput;
use computerworld::procedure::Gesture;
use cw_fidelity::hash::sha256_hex;
use cw_fidelity::pixels::settled;
use cw_fidelity::*;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub struct RefOptions {
    pub procedure: PathBuf,
    pub world: Option<PathBuf>,
    pub seed: u64,
    pub overrides: BTreeMap<String, String>,
    pub store: PathBuf,
    pub width: u32,
    pub height: u32,
    pub app: String,
    pub launch: Vec<String>,
    pub home_template: Option<PathBuf>,
    pub sandbox: Option<PathBuf>,
    pub keep_sandbox: bool,
    pub settle_ms: u64,
    pub settle_frames: usize,
    pub settle_interval_ms: u64,
    pub startup_timeout_s: u64,
    pub max_nodes: usize,
    pub ignore_paths: Vec<String>,
    pub wm: bool,
    pub max_steps: Option<usize>,
    /// Gestures at the start that open the application on the simulated desktop; the
    /// tool launches it itself, so these are recorded as not applicable, undelivered.
    pub skip_steps: usize,
    /// Interaction-id maps (`fidelity/targets/<app>.json`).
    pub target_maps: Vec<PathBuf>,
    pub notes: BTreeMap<String, String>,
}
impl RefOptions {
    pub fn defaults(procedure: PathBuf, store: PathBuf, app: String) -> Self {
        Self {
            procedure,
            world: None,
            seed: 1,
            overrides: BTreeMap::new(),
            store,
            width: 1280,
            height: 800,
            launch: Vec::new(),
            app,
            home_template: None,
            sandbox: None,
            keep_sandbox: false,
            settle_ms: 4000,
            settle_frames: 3,
            settle_interval_ms: 120,
            startup_timeout_s: 90,
            max_nodes: 30000,
            ignore_paths: default_ignores(),
            wm: true,
            max_steps: None,
            skip_steps: 0,
            target_maps: Vec::new(),
            notes: BTreeMap::new(),
        }
    }
}
pub fn default_ignores() -> Vec<String> {
    [
        ".cache/*",
        ".local/share/recently-used.xbel*",
        "*.lck",
        "*.lock",
        "*fp-info-cache*",
        ".config/*/*.log",
        ".local/share/gvfs-metadata/*",
        ".dbus/*",
        ".Xauthority",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// How an application id is launched when no `--launch` is given.
pub fn default_launch(app: &str) -> Vec<String> {
    let v: &[&str] = match app {
        "kicad" => &["kicad"],
        "kicad-schematic" | "eeschema" => &["eeschema"],
        "kicad-pcb" | "pcbnew" => &["pcbnew"],
        "libreoffice-calc" | "calc" => &["libreoffice", "--calc", "--norestore", "--nologo"],
        "libreoffice-writer" | "writer" => &["libreoffice", "--writer", "--norestore", "--nologo"],
        "freecad" => &["freecad"],
        "gedit" | "editor" => &["gedit"],
        "nautilus" | "files" => &["nautilus", "--no-desktop"],
        "gnome-terminal" | "terminal" => &["gnome-terminal", "--wait"],
        other => return vec![other.to_owned()],
    };
    v.iter().map(|s| s.to_string()).collect()
}

struct Capture<'a> {
    opts: &'a RefOptions,
    vd: VirtualDisplay,
    atspi: AtSpi,
    store: &'a Store,
    sandbox: PathBuf,
    started: Instant,
    last_walk: BTreeMap<String, (String, u64)>,
}

impl Capture<'_> {
    /// Close a dialog the map says to dismiss, if one is on screen; its title when one was.
    fn dismiss(&mut self, map: &TargetMap, input: &XInput) -> Result<Option<String>> {
        if map.dismiss.is_empty() {
            return Ok(None);
        }
        let (tree, _) = self
            .atspi
            .tree(self.opts.width, self.opts.height, self.opts.max_nodes)?;
        for d in &map.dismiss {
            for w in &tree.root.children {
                if !is_window_role(&w.role)
                    || !w.states.visible
                    || !cw_fidelity::text::glob_matches(&d.window, &w.name)
                {
                    continue;
                }
                // Resolve inside that window only.
                let mut sub = SemTree::desktop(self.opts.width, self.opts.height);
                sub.root.children.push(w.clone());
                sub.active_window = Some(vec![0]);
                if let Some(r) = resolve(&sub, &d.click) {
                    input.deliver(&Stimulus::click(r.x, r.y))?;
                    self.settle()?;
                    return Ok(Some(w.name.clone()));
                }
            }
        }
        Ok(None)
    }
    /// Sample frames until the screen stops changing; the last frame and how it settled.
    fn settle(&self) -> Result<(Image, Settle)> {
        let start = Instant::now();
        let mut hashes = Vec::new();
        loop {
            let img = self.vd.grab()?;
            hashes.push(img.content_hash());
            if settled(&hashes, self.opts.settle_frames) {
                return Ok((
                    img,
                    Settle {
                        frames: hashes.len() as u32,
                        waited: start.elapsed().as_millis() as u64,
                        stable: true,
                    },
                ));
            }
            if start.elapsed() > Duration::from_millis(self.opts.settle_ms) {
                return Ok((
                    img,
                    Settle {
                        frames: hashes.len() as u32,
                        waited: start.elapsed().as_millis() as u64,
                        stable: false,
                    },
                ));
            }
            std::thread::sleep(Duration::from_millis(self.opts.settle_interval_ms));
        }
    }
    fn clipboard(&self) -> Option<String> {
        if which("xclip").is_some() {
            run(
                "xclip",
                &["-o", "-selection", "clipboard"],
                &self.vd.env,
                Duration::from_secs(2),
            )
            .ok()
        } else if which("xsel").is_some() {
            run(
                "xsel",
                &["--clipboard", "--output"],
                &self.vd.env,
                Duration::from_secs(2),
            )
            .ok()
        } else {
            None
        }
    }
    fn effects(&mut self, tree: &SemTree) -> Effects {
        let now = walk(&self.sandbox, &self.opts.ignore_paths);
        let mut files = Vec::new();
        for (path, (sha, size)) in &now {
            match self.last_walk.get(path) {
                None => files.push(FileChange {
                    path: path.clone(),
                    kind: FileChangeKind::Created,
                    sha256: Some(sha.clone()),
                    size: *size,
                }),
                Some((old, _)) if old != sha => files.push(FileChange {
                    path: path.clone(),
                    kind: FileChangeKind::Modified,
                    sha256: Some(sha.clone()),
                    size: *size,
                }),
                _ => {}
            }
        }
        for path in self.last_walk.keys() {
            if !now.contains_key(path) {
                files.push(FileChange {
                    path: path.clone(),
                    kind: FileChangeKind::Deleted,
                    sha256: None,
                    size: 0,
                });
            }
        }
        files.sort();
        self.last_walk = now;
        let window_titles = tree
            .root
            .children
            .iter()
            .filter(|w| is_window_role(&w.role) && w.states.visible)
            .map(|w| w.name.clone())
            .collect();
        Effects {
            files,
            clipboard: self.clipboard(),
            window_titles,
        }
    }
    fn observe(&mut self) -> Result<(Observation, SemTree)> {
        let (frame, settle) = self.settle()?;
        let (tree, apps) =
            self.atspi
                .tree(self.opts.width, self.opts.height, self.opts.max_nodes)?;
        if apps.truncated {
            progress("atspi.truncated", json!({"ms": apps.ms}));
        }
        let frame_ref = self.store.put_frame(&frame)?;
        let effects = self.effects(&tree);
        let normalized = normalize(&tree, &NormalizeOptions::default());
        let hash = cw_fidelity::normalize::tree_hash_hex(
            &normalized,
            cw_fidelity::normalize::HashFields::Content,
        );
        Ok((
            Observation {
                semantic: Some(tree.clone()),
                semantic_hash: Some(hash),
                frame: Some(frame_ref),
                effects,
                settle,
                time: self.started.elapsed().as_millis() as u64,
            },
            tree,
        ))
    }
}

fn os_version() -> String {
    std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|t| {
            t.lines().find_map(|l| {
                l.strip_prefix("PRETTY_NAME=")
                    .map(|v| v.trim_matches('"').to_owned())
            })
        })
        .unwrap_or_default()
}
/// The application's version string. `kicad --version` opens the GUI, so KiCad is asked
/// through `kicad-cli`; anything else answers `--version` on the private display, where a
/// window it opens instead of answering harms nothing and is killed by the timeout.
/// `kicad-cli` beside the `kicad` that was launched (a different one may be first on
/// PATH); the bare name when the launch command had no directory.
fn sibling(cmd: &str, name: &str) -> String {
    let resolved = if cmd.contains('/') {
        Some(PathBuf::from(cmd))
    } else {
        which(cmd)
    };
    match resolved.and_then(|p| p.parent().map(|d| d.join(name))) {
        Some(p) if p.exists() => p.to_string_lossy().into_owned(),
        _ => name.to_owned(),
    }
}

fn app_version(launch: &[String], env: &BTreeMap<String, String>) -> String {
    let Some(cmd) = launch.first() else {
        return String::new();
    };
    let base = Path::new(cmd)
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (program, args): (String, Vec<&str>) = match base.as_str() {
        "kicad" | "eeschema" | "pcbnew" => (sibling(cmd, "kicad-cli"), vec!["version"]),
        "libreoffice" | "soffice" => ("libreoffice".to_owned(), vec!["--version"]),
        _ => (cmd.clone(), vec!["--version"]),
    };
    if !program.contains('/') && which(&program).is_none() {
        return String::new();
    }
    run(&program, &args, env, Duration::from_secs(10))
        .ok()
        .and_then(|o| {
            o.lines()
                .find(|l| !l.trim().is_empty())
                .map(|l| l.trim().to_owned())
        })
        .unwrap_or_default()
}
fn fonts_hash(env: &BTreeMap<String, String>) -> Option<String> {
    let out = run("fc-list", &[":", "family"], env, Duration::from_secs(20)).ok()?;
    let mut fams: Vec<&str> = out
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    fams.sort_unstable();
    fams.dedup();
    Some(sha256_hex(fams.join("\n").as_bytes()))
}
fn config_hash(sandbox: &Path) -> Option<String> {
    let cfg = walk(&sandbox.join(".config"), &[]);
    if cfg.is_empty() {
        return None;
    }
    let text: String = cfg.iter().map(|(p, (h, _))| format!("{p} {h}\n")).collect();
    Some(sha256_hex(text.as_bytes()))
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for e in std::fs::read_dir(from)
        .map_err(|e| format!("{}: {e}", from.display()))?
        .flatten()
    {
        let p = e.path();
        let dest = to.join(e.file_name());
        if p.is_dir() {
            copy_dir(&p, &dest)?;
        } else {
            std::fs::copy(&p, &dest).map_err(|e| format!("{}: {e}", p.display()))?;
        }
    }
    Ok(())
}

/// Resolve a target, going through the interaction map when the target is a simulator
/// interaction id. Returns the resolution and the target actually used.
fn resolve_mapped(
    tree: &SemTree,
    t: &Target,
    map: &TargetMap,
) -> std::result::Result<(Resolution, Target), String> {
    if let Some(id) = &t.interaction {
        match map.lookup(id) {
            Some(Mapped::Targets(list)) => {
                for mut alt in list {
                    if t.at.is_some() {
                        alt.at = t.at;
                    }
                    if let Some(r) = resolve(tree, &alt) {
                        return Ok((r, alt));
                    }
                }
                return Err(format!("no mapped target for {id} is on screen"));
            }
            Some(Mapped::Skip) => return Err(format!("{id}: mapped as not applicable")),
            Some(Mapped::Key(_)) | Some(Mapped::Type(_)) | Some(Mapped::Sequence(_)) => {
                return Err(format!("{id}: mapped to input, not a target"))
            }
            None => {
                return Err(format!(
                    "{id}: simulator interaction id with no entry in the target map"
                ))
            }
        }
    }
    resolve(tree, t)
        .map(|r| (r, t.clone()))
        .ok_or_else(|| "target not found".to_owned())
}

type Planned = (
    Option<Target>,
    Option<Resolution>,
    std::result::Result<Stimulus, String>,
);

/// A mapped sequence: the stimuli before the last one, which the step records.
fn sequence_for(g: &Gesture, map: &TargetMap) -> Option<Vec<Mapped>> {
    if let Gesture::Click { target, .. } | Gesture::DoubleClick { target, .. } = g {
        if let Some(Mapped::Sequence(list)) =
            target.interaction.as_deref().and_then(|id| map.lookup(id))
        {
            return Some(list);
        }
    }
    None
}

/// One element of a mapped sequence against the current screen.
fn mapped_stimulus(
    m: &Mapped,
    tree: &SemTree,
    at: Option<[u32; 2]>,
) -> std::result::Result<(Stimulus, Option<Resolution>), String> {
    match m {
        Mapped::Key(k) => Ok((Stimulus::Key { key: k.clone() }, None)),
        Mapped::Type(x) => Ok((Stimulus::Type { text: x.clone() }, None)),
        Mapped::Skip => Err("mapped as not applicable".into()),
        Mapped::Sequence(_) => Err("nested sequence".into()),
        Mapped::Targets(list) => {
            for alt in list {
                let mut alt = alt.clone();
                if at.is_some() {
                    alt.at = at;
                }
                if let Some(r) = resolve(tree, &alt) {
                    let s = Stimulus::Pointer {
                        op: PointerOp::Click,
                        x: r.x,
                        y: r.y,
                        button: 1,
                        modifiers: vec![],
                        delta_x: 0,
                        delta_y: 0,
                    };
                    return Ok((s, Some(r)));
                }
            }
            Err("no mapped target is on screen".into())
        }
    }
}

/// Turn a gesture into the stimulus the reference receives, resolving its target.
fn stimulus_for(g: &Gesture, tree: &SemTree, map: &TargetMap) -> Planned {
    // An interaction id mapped to a key or typed text replaces the click outright.
    if let Gesture::Click { target, .. } | Gesture::DoubleClick { target, .. } = g {
        if let Some(id) = &target.interaction {
            match map.lookup(id) {
                Some(Mapped::Key(k)) => {
                    return (
                        Some(target_of(target, None)),
                        None,
                        Ok(Stimulus::Key { key: k }),
                    )
                }
                Some(Mapped::Type(x)) => {
                    return (
                        Some(target_of(target, None)),
                        None,
                        Ok(Stimulus::Type { text: x }),
                    )
                }
                _ => {}
            }
        }
    }
    let pointer = |op: PointerOp, r: &Resolution| Stimulus::Pointer {
        op,
        x: r.x,
        y: r.y,
        button: 1,
        modifiers: vec![],
        delta_x: 0,
        delta_y: 0,
    };
    // A click on the simulator's title-bar buttons is a window-manager operation here.
    if let Gesture::Click { target, .. } = g {
        if let Some(label) = &target.label {
            let l = label.to_lowercase();
            let op = if l.starts_with("maximize") {
                Some(WindowOp::Maximize)
            } else if l.starts_with("minimize") {
                Some(WindowOp::Minimize)
            } else if l.starts_with("close window") {
                Some(WindowOp::Close)
            } else if l.starts_with("restore") {
                Some(WindowOp::Restore)
            } else {
                None
            };
            if let Some(op) = op {
                return (
                    Some(target_of(target, None)),
                    None,
                    Ok(Stimulus::Window { op }),
                );
            }
        }
    }
    match g {
        Gesture::Click { target, at } | Gesture::DoubleClick { target, at } => {
            let t = target_of(target, *at);
            if target
                .interaction
                .as_deref()
                .is_some_and(|i| i.starts_with("shell:"))
            {
                return (
                    Some(t),
                    None,
                    Err("shell gesture: not applicable on a reference desktop".into()),
                );
            }
            match resolve_mapped(tree, &t, map) {
                Ok((r, used)) => {
                    let op = if matches!(g, Gesture::Click { .. }) {
                        PointerOp::Click
                    } else {
                        PointerOp::DoubleClick
                    };
                    let s = pointer(op, &r);
                    (Some(used), Some(r), Ok(s))
                }
                Err(e) => (Some(t), None, Err(e)),
            }
        }
        Gesture::Drag {
            from,
            from_at,
            to,
            to_at,
        } => {
            let tf = target_of(from, *from_at);
            let tt = target_of(to, *to_at);
            match (
                resolve_mapped(tree, &tf, map),
                resolve_mapped(tree, &tt, map),
            ) {
                (Ok((a, used)), Ok((b, _))) => {
                    let s = Stimulus::Drag {
                        x: a.x,
                        y: a.y,
                        to_x: b.x,
                        to_y: b.y,
                        button: 1,
                    };
                    (Some(used), Some(a), Ok(s))
                }
                (Err(e), _) => (Some(tf), None, Err(format!("drag source: {e}"))),
                (_, Err(e)) => (Some(tf), None, Err(format!("drag destination: {e}"))),
            }
        }
        Gesture::Scroll {
            target,
            delta_y,
            at,
        } => {
            let t = target_of(target, *at);
            match resolve_mapped(tree, &t, map) {
                Ok((r, used)) => {
                    let s = Stimulus::Pointer {
                        op: PointerOp::Wheel,
                        x: r.x,
                        y: r.y,
                        button: 0,
                        modifiers: vec![],
                        delta_x: 0,
                        delta_y: *delta_y,
                    };
                    (Some(used), Some(r), Ok(s))
                }
                Err(e) => (Some(t), None, Err(e)),
            }
        }
        Gesture::ClickModel { .. } => (
            None,
            None,
            Err("click_model needs the simulator's camera".into()),
        ),
        Gesture::Type { text, .. } => (None, None, Ok(Stimulus::Type { text: text.clone() })),
        Gesture::Key { key } => (None, None, Ok(Stimulus::Key { key: key.clone() })),
        Gesture::Wait {
            days,
            hours,
            minutes,
            seconds,
            ticks,
        } => {
            // World time cannot pass faster on a real desktop; wait a bounded real
            // second per logical unit so the application gets a chance to idle.
            let logical =
                ((*days * 24 + *hours) * 60 + *minutes) * 60 + *seconds + ticks / 1_000_000;
            (
                None,
                None,
                Ok(Stimulus::Wait {
                    ticks: (logical.min(5) * 1000).max(200),
                }),
            )
        }
    }
}

pub fn capture(opts: &RefOptions) -> Result<String> {
    let store = Store::open(&opts.store)?;
    let plan: Plan = plan(
        &opts.procedure,
        opts.world.as_deref(),
        opts.seed,
        &opts.overrides,
    )?;
    let map = TargetMap::load(&opts.target_maps)?;
    if map.is_empty() {
        progress(
            "target_map",
            json!({"note": "no --target-map: simulator interaction ids will not resolve on the reference"}),
        );
    }
    let sandbox = opts.sandbox.clone().unwrap_or_else(|| {
        std::env::temp_dir().join(format!(
            "cwfid-home-{}-{}",
            std::process::id(),
            plan.reference.id
        ))
    });
    if sandbox.exists() && opts.sandbox.is_none() {
        std::fs::remove_dir_all(&sandbox).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir_all(&sandbox).map_err(|e| e.to_string())?;
    if let Some(t) = &opts.home_template {
        copy_dir(t, &sandbox)?;
    }
    let staged = stage_sandbox(&plan, &sandbox)?;
    progress(
        "sandbox",
        json!({"path": sandbox, "staged": staged, "procedure": plan.procedure_path, "world": plan.world_path}),
    );

    let mut vd = VirtualDisplay::start(opts.width, opts.height, opts.wm)?;
    progress("display", json!({"display": vd.display, "wm": vd.wm}));
    let mut home_env = BTreeMap::new();
    home_env.insert("HOME".to_owned(), sandbox.to_string_lossy().into_owned());
    home_env.insert(
        "XDG_CONFIG_HOME".to_owned(),
        sandbox.join(".config").to_string_lossy().into_owned(),
    );
    home_env.insert(
        "XDG_DATA_HOME".to_owned(),
        sandbox.join(".local/share").to_string_lossy().into_owned(),
    );
    home_env.insert(
        "XDG_CACHE_HOME".to_owned(),
        sandbox.join(".cache").to_string_lossy().into_owned(),
    );
    home_env.insert(
        "LANG".to_owned(),
        std::env::var("LANG").unwrap_or_else(|_| "en_US.UTF-8".into()),
    );
    let mut env_all = vd.env.clone();
    env_all.extend(home_env.clone());

    let launch = if opts.launch.is_empty() {
        default_launch(&opts.app)
    } else {
        opts.launch.clone()
    };
    let Some(cmd) = launch.first() else {
        return err("empty launch command");
    };
    if which(cmd).is_none() {
        return err(format!("{cmd} is not installed on this machine"));
    }
    let mut atspi = AtSpi::start(&env_all)?;
    let version = app_version(&launch, &env_all);
    let args: Vec<&str> = launch[1..].iter().map(String::as_str).collect();
    let pid = vd.spawn("app", cmd, &args, &home_env, Some(&sandbox))?;
    progress(
        "launched",
        json!({"command": launch, "pid": pid, "version": version}),
    );

    // Wait for a window with a size.
    let start = Instant::now();
    let apps_seen;
    loop {
        if !vd.alive("app") {
            return err(format!("{cmd} exited before showing a window"));
        }
        let (tree, apps) = atspi.tree(opts.width, opts.height, 2000)?;
        if tree
            .root
            .children
            .iter()
            .any(|w| w.states.visible && w.bounds.w > 50 && w.bounds.h > 50)
        {
            apps_seen = apps.list.clone();
            break;
        }
        if start.elapsed() > Duration::from_secs(opts.startup_timeout_s) {
            return err(format!(
                "no window appeared within {} s; AT-SPI apps: {:?}",
                opts.startup_timeout_s, apps.list
            ));
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    // Toolkits take a moment to publish their full tree after the first window.
    std::thread::sleep(Duration::from_millis(800));

    let toolkit = apps_seen
        .iter()
        .find(|(_, p, _, _)| p == &pid.to_string())
        .map(|(_, _, t, v)| format!("{t} {v}").trim().to_owned())
        .or_else(|| {
            apps_seen
                .iter()
                .find(|(_, _, t, _)| !t.is_empty())
                .map(|(_, _, t, v)| format!("{t} {v}").trim().to_owned())
        });
    let fingerprint = Fingerprint {
        os_family: "linux".into(),
        os_version: os_version(),
        app: opts.app.clone(),
        app_version: version.clone(),
        display: Display {
            width: opts.width,
            height: opts.height,
            dpi: 96,
            scale: 1.0,
        },
        toolkit,
        theme: Some("Adwaita".into()),
        locale: std::env::var("LANG").ok(),
        fonts: fonts_hash(&env_all),
        config_hash: config_hash(&sandbox),
        extra: BTreeMap::from([
            ("wm".to_owned(), vd.wm.clone().unwrap_or_default()),
            ("launch".to_owned(), launch.join(" ")),
            (
                "hostname".to_owned(),
                std::fs::read_to_string("/etc/hostname")
                    .unwrap_or_default()
                    .trim()
                    .to_owned(),
            ),
        ]),
    };

    let mut cap = Capture {
        opts,
        vd,
        atspi,
        store: &store,
        sandbox: sandbox.clone(),
        started: Instant::now(),
        last_walk: BTreeMap::new(),
    };
    cap.last_walk = walk(&cap.sandbox, &opts.ignore_paths);
    let mut trajectory = Trajectory::new(Side::Reference, fingerprint);
    trajectory.procedure = Some(plan.reference.clone());
    trajectory.notes = opts.notes.clone();
    trajectory
        .notes
        .insert("machine".into(), plan.machine.clone());
    trajectory.notes.insert("home".into(), plan.home.clone());
    let input_env = cap.vd.env.clone();
    let input = XInput { env: &input_env };
    for _ in 0..3 {
        match cap.dismiss(&map, &input)? {
            Some(w) => progress("dismissed", json!({"window": w, "before_step": "initial"})),
            None => break,
        }
    }
    let (initial, mut tree) = cap.observe()?;
    progress(
        "initial",
        json!({"windows": tree.root.children.len(), "nodes": tree.size(), "settle": initial.settle}),
    );
    trajectory.initial = initial;

    let total = plan.gestures.len();
    for (i, (phase, gesture)) in plan.gestures.iter().enumerate() {
        if opts.max_steps.is_some_and(|m| i >= m) {
            break;
        }
        if i >= opts.skip_steps {
            if let Some(dismissed) = cap.dismiss(&map, &input)? {
                progress("dismissed", json!({"window": dismissed, "before_step": i}));
                tree = cap.atspi.tree(opts.width, opts.height, opts.max_nodes)?.0;
            }
        }
        // A mapped sequence: deliver all but its last element here, re-reading the
        // screen after each, so the gesture's own step records the last.
        let at = match gesture {
            Gesture::Click { at, .. } | Gesture::DoubleClick { at, .. } => *at,
            _ => None,
        };
        let mut sequence_error = None;
        let sequence = if i >= opts.skip_steps {
            sequence_for(gesture, &map)
        } else {
            None
        };
        if let Some(list) = &sequence {
            for m in list.iter().take(list.len().saturating_sub(1)) {
                match mapped_stimulus(m, &tree, at) {
                    Ok((s, _)) => {
                        if let Err(e) = input.deliver(&s) {
                            sequence_error = Some(e);
                            break;
                        }
                        cap.settle()?;
                        tree = cap.atspi.tree(opts.width, opts.height, opts.max_nodes)?.0;
                    }
                    Err(e) => {
                        sequence_error = Some(format!("sequence: {e}"));
                        break;
                    }
                }
            }
        }
        let (target, resolved, stimulus) = if i < opts.skip_steps {
            let (t, _, _) = stimulus_for(gesture, &tree, &map);
            (
                t,
                None,
                Err("skipped: the tool launched the application".to_owned()),
            )
        } else if let Some(e) = sequence_error {
            let (t, _, _) = stimulus_for(gesture, &tree, &map);
            (t, None, Err(e))
        } else if let Some(list) = &sequence {
            let (t, _, _) = stimulus_for(gesture, &tree, &map);
            match list.last().map(|m| mapped_stimulus(m, &tree, at)) {
                Some(Ok((s, r))) => (t, r, Ok(s)),
                Some(Err(e)) => (t, None, Err(format!("sequence: {e}"))),
                None => (t, None, Err("empty sequence".into())),
            }
        } else {
            stimulus_for(gesture, &tree, &map)
        };
        let (stimulus, ok, error) = match stimulus {
            Ok(s) => match input.deliver(&s) {
                Ok(()) => (s, true, None),
                Err(e) => (s, false, Some(e)),
            },
            Err(e) => {
                // Nothing was delivered; record the intent with a null pointer stimulus.
                let placeholder = match gesture {
                    Gesture::Type { text, .. } => Stimulus::Type { text: text.clone() },
                    Gesture::Key { key } => Stimulus::Key { key: key.clone() },
                    _ => Stimulus::Pointer {
                        op: PointerOp::Click,
                        x: -1,
                        y: -1,
                        button: 1,
                        modifiers: vec![],
                        delta_x: 0,
                        delta_y: 0,
                    },
                };
                (placeholder, false, Some(e))
            }
        };
        let (observation, new_tree) = match cap.observe() {
            Ok(x) => x,
            Err(e) => {
                progress("aborted", json!({"step": i, "error": e}));
                trajectory.notes.insert(
                    "aborted".into(),
                    format!("observation after step {i} failed: {e}"),
                );
                break;
            }
        };
        tree = new_tree;
        progress(
            "step",
            json!({"index": i, "of": total, "phase": phase, "stimulus": cw_fidelity::cascade::describe(&stimulus), "ok": ok, "error": error, "windows": tree.root.children.len(), "nodes": tree.size(), "settle_ms": observation.settle.waited, "files": observation.effects.files.len()}),
        );
        trajectory.steps.push(Step {
            index: i as u32,
            phase: Some(phase.clone()),
            label: None,
            stimulus,
            target,
            resolved,
            ok,
            error,
            observation,
        });
        if !cap.vd.alive("app") {
            trajectory.notes.insert(
                "aborted".into(),
                format!("application exited after step {i}"),
            );
            break;
        }
    }
    trajectory.created = now_iso();
    let id = store.save_trajectory(&mut trajectory)?;
    progress("saved", json!({"id": id, "steps": trajectory.steps.len()}));
    drop(cap);
    if !opts.keep_sandbox && opts.sandbox.is_none() {
        let _ = std::fs::remove_dir_all(&sandbox);
    }
    Ok(id)
}
