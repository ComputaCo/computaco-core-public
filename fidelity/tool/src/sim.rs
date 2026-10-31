//! The simulated side: ComputerWorld performing the same plan, observed through the
//! scene's accessibility list, the renderer and the virtual file system; and the
//! lockstep replay of a reference trajectory with a comparison at every step.
use crate::procedure::{plan, Plan};
use crate::store::Store;
use crate::util::{err, now_iso, progress, Result};
use computerworld::procedure::{Gesture, Mode, ProjectOptions, Target as ProcTarget, Typing};
use computerworld::{ActionEnvelope, EnvironmentConfig, World};
use cw_fidelity::cascade::{compare_step, describe, CompareConfig, TrajectoryComparison};
use cw_fidelity::hash::sha256_hex;
use cw_fidelity::*;
use cw_scene::Scene;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const GRANTS: &[&str] = &[
    "application.v1",
    "keyboard.v1",
    "pointer.v1",
    "browser.v1",
    "filesystem.v1",
    "terminal.v1",
];

pub struct SimOptions {
    pub procedure: PathBuf,
    pub world: Option<PathBuf>,
    pub seed: u64,
    pub world_seed: u64,
    pub overrides: BTreeMap<String, String>,
    pub store: PathBuf,
    pub width: u32,
    pub height: u32,
    pub app: Option<String>,
    pub max_steps: Option<usize>,
    pub notes: BTreeMap<String, String>,
}

pub struct Sim {
    pub world: World,
    pub session: String,
    pub machine: String,
    pub home: String,
    pub width: u32,
    pub height: u32,
    last_walk: BTreeMap<String, (String, u64)>,
}

fn rect(r: cw_scene::Rect) -> Rect {
    Rect::new(r.x, r.y, r.width as i32, r.height as i32)
}

/// The scene as a semantic tree: windows in stacking order, each holding its
/// interactive controls and painted text.
pub fn scene_tree(scene: &Scene) -> SemTree {
    let mut tree = SemTree::desktop(scene.width, scene.height);
    let mut windows: Vec<&cw_scene::SceneWindow> =
        scene.windows.iter().filter(|w| !w.minimized).collect();
    windows.sort_by_key(|w| w.z);
    let mut by_window: BTreeMap<Option<u64>, Vec<SemNode>> = BTreeMap::new();
    let mut focus_interaction = scene.focus.as_ref().and_then(|f| f.interaction.clone());
    for ax in scene.accessibility() {
        let node = SemNode {
            role: cw_fidelity::normalize::canonical_role(&ax.role).to_owned(),
            name: ax.name.clone(),
            value: ax.value.clone(),
            states: States {
                enabled: ax.enabled,
                focusable: ax.focusable,
                focused: ax.focused,
                visible: true,
                checked: ax.checked,
                selected: ax.selected,
                expanded: ax.expanded,
            },
            bounds: rect(ax.bounds),
            source_id: Some(ax.id.clone()),
            attrs: BTreeMap::new(),
            children: Vec::new(),
        };
        if ax.focused {
            focus_interaction = Some(ax.id.clone());
        }
        by_window.entry(ax.window).or_default().push(node);
    }
    // Painted text that is not itself a control.
    for n in scene.ordered_nodes() {
        if n.interaction.is_some() {
            continue;
        }
        if let Some(text) = n.painted_text() {
            if text.trim().is_empty() {
                continue;
            }
            let node = SemNode {
                role: "text".into(),
                name: text.to_owned(),
                value: None,
                states: States::visible_enabled(),
                bounds: rect(n.painted_bounds()),
                source_id: Some(format!("node:{}", n.id)),
                attrs: BTreeMap::new(),
                children: Vec::new(),
            };
            by_window.entry(n.window).or_default().push(node);
        }
    }
    let mut focus_path = None;
    for w in windows {
        let mut node = SemNode::new("window", w.title.clone(), rect(w.bounds));
        node.states.focused = w.focused;
        node.attrs.insert("app".into(), w.app.clone());
        if !w.document.is_empty() {
            node.attrs.insert("document".into(), w.document.clone());
        }
        node.source_id = Some(format!("window:{}", w.id));
        node.children = by_window.remove(&Some(w.id)).unwrap_or_default();
        let index = tree.root.children.len() as u32;
        if let Some(f) = &focus_interaction {
            if let Some(ci) = node
                .children
                .iter()
                .position(|c| c.source_id.as_deref() == Some(f.as_str()))
            {
                focus_path = Some(vec![index, ci as u32]);
            }
        }
        if w.focused {
            tree.active_window = Some(vec![index]);
        }
        tree.root.children.push(node);
    }
    // Shell controls and loose text (taskbar, dock, desktop icons) go first, as the
    // bottom-most "window".
    let mut loose: Vec<SemNode> = by_window.into_values().flatten().collect();
    if !loose.is_empty() {
        let mut shell = SemNode::new("window", "", tree.root.bounds);
        shell.attrs.insert("app".into(), "shell".into());
        loose.sort_by_key(|n| (n.bounds.y, n.bounds.x));
        shell.children = loose;
        tree.root.children.insert(0, shell);
        if let Some(a) = &mut tree.active_window {
            a[0] += 1;
        }
        if let Some(f) = &mut focus_path {
            f[0] += 1;
        }
    }
    tree.focus = focus_path;
    tree
}

impl Sim {
    pub fn boot(plan: &Plan, world_seed: u64, width: u32, height: u32) -> Result<Self> {
        let Some(mut definition) = plan.definition.clone() else {
            return err("the procedure's world.json was not found; pass --world");
        };
        plan.instance.procedure.prepare(&mut definition);
        let mut world = World::new(definition, world_seed).map_err(|e| e.to_string())?;
        world.stage(&plan.instance).map_err(|e| e.to_string())?;
        let config = EnvironmentConfig {
            actor: plan.user.clone(),
            machines: vec![plan.machine.clone()],
            actions: GRANTS.iter().map(|s| s.to_string()).collect(),
            observations: vec!["semantic.v1".into()],
            action_budget: 1024,
        };
        let session = world.environment(config).map_err(|e| e.to_string())?;
        Ok(Self {
            world,
            session,
            machine: plan.machine.clone(),
            home: plan.home.clone(),
            width,
            height,
            last_walk: BTreeMap::new(),
        })
    }
    pub fn fingerprint(&self, app: &str) -> Fingerprint {
        let def = self.world.definition();
        let computer = def.computers.iter().find(|c| c.id == self.machine);
        let profile = computer.and_then(|c| def.profiles.iter().find(|p| p.id == c.profile));
        Fingerprint {
            os_family: profile
                .map(|p| p.family.clone())
                .unwrap_or_else(|| "computerworld".into()),
            os_version: profile.map(|p| p.name.clone()).unwrap_or_default(),
            app: app.to_owned(),
            app_version: format!("computerworld {}", env!("CARGO_PKG_VERSION")),
            display: Display {
                width: self.width,
                height: self.height,
                dpi: 96,
                scale: 1.0,
            },
            toolkit: Some("cw-native".into()),
            theme: def
                .metadata
                .get("desktop_themes")
                .and_then(|t| t.get(&self.machine))
                .and_then(|v| v.as_str())
                .map(str::to_owned),
            locale: Some("en_US.UTF-8".into()),
            fonts: None,
            config_hash: None,
            extra: BTreeMap::from([("world".to_owned(), def.id.clone())]),
        }
    }
    fn walk_home(&self) -> BTreeMap<String, (String, u64)> {
        let mut out = BTreeMap::new();
        let Ok(computer) = self.world.runtime().computer(&self.machine) else {
            return out;
        };
        let mut stack = vec![self.home.clone()];
        while let Some(dir) = stack.pop() {
            let Ok(names) = computer.vfs.list(&dir) else {
                continue;
            };
            for name in names {
                if name == "." || name == ".." {
                    continue;
                }
                let path = if dir.ends_with('/') {
                    format!("{dir}{name}")
                } else {
                    format!("{dir}/{name}")
                };
                let Ok(meta) = computer.vfs.stat(&path) else {
                    continue;
                };
                if meta.is_dir {
                    stack.push(path);
                } else {
                    let rel = path
                        .strip_prefix(&self.home)
                        .unwrap_or(&path)
                        .trim_start_matches('/')
                        .to_owned();
                    let sha = self
                        .world
                        .runtime()
                        .read_file(&self.machine, &path)
                        .ok()
                        .map(|b| sha256_hex(&b))
                        .unwrap_or_default();
                    out.insert(rel, (sha, meta.size as u64));
                }
            }
        }
        out
    }
    fn effects(&mut self, tree: &SemTree) -> Effects {
        let now = self.walk_home();
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
        let clipboard = self
            .world
            .interfaces()
            .session(&self.session)
            .ok()
            .and_then(|s| s.machines.get(&self.machine))
            .and_then(|m| m.desktop.clipboard_text.clone());
        let window_titles = tree
            .root
            .children
            .iter()
            .filter(|w| is_window_role(&w.role))
            .map(|w| w.name.clone())
            .collect();
        Effects {
            files,
            clipboard,
            window_titles,
        }
    }
    pub fn observe(&mut self, store: &Store, with_frame: bool) -> Result<(Observation, SemTree)> {
        let scene = self
            .world
            .scene(&self.session, self.width, self.height)
            .map_err(|e| e.to_string())?;
        let tree = scene_tree(&scene);
        let frame = if with_frame {
            let f = self
                .world
                .render(&self.session, self.width, self.height)
                .map_err(|e| e.to_string())?;
            Some(store.put_frame(&Image::new(f.width, f.height, f.rgba))?)
        } else {
            None
        };
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
                frame,
                effects,
                settle: Settle {
                    frames: 1,
                    waited: 0,
                    stable: true,
                },
                time: self.world.tick(),
            },
            tree,
        ))
    }
    /// Perform one gesture through the engine's own target resolution.
    pub fn perform(
        &mut self,
        gesture: &Gesture,
    ) -> (
        Option<Target>,
        Option<Resolution>,
        Stimulus,
        bool,
        Option<String>,
    ) {
        let options = ProjectOptions {
            width: self.width,
            height: self.height,
            mode: Mode::Gui,
            typing: Typing::Text,
        };
        let target = match gesture {
            Gesture::Click { target, at }
            | Gesture::DoubleClick { target, at }
            | Gesture::Scroll { target, at, .. } => Some(crate::procedure::target_of(target, *at)),
            Gesture::Drag { from, from_at, .. } => {
                Some(crate::procedure::target_of(from, *from_at))
            }
            _ => None,
        };
        let result = self.world.perform(&self.session, gesture, options);
        let (steps, error) = match result {
            Ok(steps) => (steps, None),
            Err(e) => (Vec::new(), Some(format!("{}: {}", e.code, e.message))),
        };
        let ok = error.is_none() && !steps.is_empty() && steps.iter().all(|s| s.success);
        let error = error.or_else(|| {
            steps.iter().find_map(|s| {
                s.error
                    .as_ref()
                    .map(|e| format!("{}: {}", e.code, e.message))
            })
        });
        let resolved = steps
            .iter()
            .find_map(|s| s.target.as_ref())
            .map(|r| Resolution {
                path: Vec::new(),
                role: r.role.clone(),
                name: r.label.clone(),
                bounds: Rect::new(r.x, r.y, 0, 0),
                x: r.x,
                y: r.y,
            });
        let (x, y) = resolved.as_ref().map(|r| (r.x, r.y)).unwrap_or((-1, -1));
        let stimulus = match gesture {
            Gesture::Click { .. } => Stimulus::Pointer {
                op: PointerOp::Click,
                x,
                y,
                button: 1,
                modifiers: vec![],
                delta_x: 0,
                delta_y: 0,
            },
            Gesture::DoubleClick { .. } => Stimulus::Pointer {
                op: PointerOp::DoubleClick,
                x,
                y,
                button: 1,
                modifiers: vec![],
                delta_x: 0,
                delta_y: 0,
            },
            Gesture::Scroll { delta_y, .. } => Stimulus::Pointer {
                op: PointerOp::Wheel,
                x,
                y,
                button: 0,
                modifiers: vec![],
                delta_x: 0,
                delta_y: *delta_y,
            },
            Gesture::Drag { .. } => {
                let last = steps
                    .iter()
                    .filter_map(|s| s.target.as_ref())
                    .next_back()
                    .map(|r| (r.x, r.y))
                    .unwrap_or((x, y));
                Stimulus::Drag {
                    x,
                    y,
                    to_x: last.0,
                    to_y: last.1,
                    button: 1,
                }
            }
            Gesture::ClickModel { .. } => Stimulus::Pointer {
                op: PointerOp::Click,
                x,
                y,
                button: 1,
                modifiers: vec![],
                delta_x: 0,
                delta_y: 0,
            },
            Gesture::Type { text, .. } => Stimulus::Type { text: text.clone() },
            Gesture::Key { key } => Stimulus::Key { key: key.clone() },
            Gesture::Wait { .. } => Stimulus::Wait {
                ticks: steps.iter().map(|s| s.advance).sum(),
            },
        };
        (target, resolved, stimulus, ok, error)
    }
    /// Deliver a recorded stimulus by coordinates, through the raw action families.
    pub fn deliver(&mut self, s: &Stimulus) -> (bool, Option<String>) {
        let m = self.machine.clone();
        let (w, h) = (self.width, self.height);
        let actions: Vec<ActionEnvelope> = match s {
            Stimulus::Pointer {
                op,
                x,
                y,
                button,
                modifiers,
                delta_x,
                delta_y,
            } => {
                let mut payload =
                    json!({"x": x, "y": y, "width": w, "height": h, "button": button});
                if !modifiers.is_empty() {
                    payload["modifiers"] = json!(modifiers
                        .iter()
                        .map(|m| m.to_lowercase())
                        .collect::<Vec<_>>());
                }
                let op_name = match op {
                    PointerOp::Move => "move",
                    PointerOp::Down => "down",
                    PointerOp::Up => "up",
                    PointerOp::Click => "click",
                    PointerOp::DoubleClick => "double_click",
                    PointerOp::Wheel => {
                        payload["delta_y"] = json!(delta_y);
                        payload["delta_x"] = json!(delta_x);
                        "wheel"
                    }
                };
                vec![ActionEnvelope::new("pointer.v1", op_name, m, payload)]
            }
            Stimulus::Drag {
                x,
                y,
                to_x,
                to_y,
                button,
            } => vec![
                ActionEnvelope::new(
                    "pointer.v1",
                    "down",
                    m.clone(),
                    json!({"x": x, "y": y, "width": w, "height": h, "button": button}),
                ),
                ActionEnvelope::new(
                    "pointer.v1",
                    "move",
                    m.clone(),
                    json!({"x": (x + to_x) / 2, "y": (y + to_y) / 2, "width": w, "height": h}),
                ),
                ActionEnvelope::new(
                    "pointer.v1",
                    "move",
                    m.clone(),
                    json!({"x": to_x, "y": to_y, "width": w, "height": h}),
                ),
                ActionEnvelope::new(
                    "pointer.v1",
                    "up",
                    m,
                    json!({"x": to_x, "y": to_y, "width": w, "height": h, "button": button}),
                ),
            ],
            Stimulus::Window { op } => {
                return (
                    false,
                    Some(format!(
                        "window {op:?}: no coordinate equivalent; replay by target"
                    )),
                );
            }
            Stimulus::Key { key } => vec![ActionEnvelope::new(
                "keyboard.v1",
                "key",
                m,
                json!({"key": key}),
            )],
            Stimulus::Type { text } => {
                let mut v = Vec::new();
                let mut first = true;
                for line in text.split('\n') {
                    if !first {
                        v.push(ActionEnvelope::new(
                            "keyboard.v1",
                            "key",
                            m.clone(),
                            json!({"key": "Enter"}),
                        ));
                    }
                    first = false;
                    if !line.is_empty() {
                        v.push(ActionEnvelope::new(
                            "keyboard.v1",
                            "type",
                            m.clone(),
                            json!({"text": line}),
                        ));
                    }
                }
                v
            }
            Stimulus::Wait { ticks } => {
                return match self.world.advance(*ticks) {
                    Ok(()) => (true, None),
                    Err(e) => (false, Some(e.to_string())),
                };
            }
        };
        let mut ok = true;
        let mut error = None;
        for a in actions {
            match self.world.step(&self.session, vec![a]) {
                Ok(r) => {
                    for o in &r.outcomes {
                        if !o.success {
                            ok = false;
                            if error.is_none() {
                                error = o
                                    .error
                                    .as_ref()
                                    .map(|e| format!("{}: {}", e.code, e.message));
                            }
                        }
                    }
                }
                Err(e) => {
                    ok = false;
                    error.get_or_insert_with(|| e.to_string());
                }
            }
        }
        (ok, error)
    }
}

pub fn capture(opts: &SimOptions) -> Result<String> {
    let store = Store::open(&opts.store)?;
    let plan = plan(
        &opts.procedure,
        opts.world.as_deref(),
        opts.seed,
        &opts.overrides,
    )?;
    let app = opts.app.clone().unwrap_or_else(|| app_of_plan(&plan));
    let mut sim = Sim::boot(&plan, opts.world_seed, opts.width, opts.height)?;
    let mut trajectory = Trajectory::new(Side::Simulated, sim.fingerprint(&app));
    trajectory.procedure = Some(plan.reference.clone());
    trajectory.notes = opts.notes.clone();
    trajectory
        .notes
        .insert("machine".into(), plan.machine.clone());
    trajectory.notes.insert("home".into(), plan.home.clone());
    sim.last_walk = sim.walk_home();
    let (initial, _) = sim.observe(&store, true)?;
    trajectory.initial = initial;
    let total = plan.gestures.len();
    for (i, (phase, gesture)) in plan.gestures.iter().enumerate() {
        if opts.max_steps.is_some_and(|m| i >= m) {
            break;
        }
        let (target, resolved, stimulus, ok, error) = sim.perform(gesture);
        let (observation, tree) = sim.observe(&store, true)?;
        progress(
            "step",
            json!({"index": i, "of": total, "phase": phase, "stimulus": describe(&stimulus), "ok": ok, "error": error, "windows": tree.root.children.len(), "nodes": tree.size(), "tick": observation.time}),
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
    }
    trajectory.created = now_iso();
    let id = store.save_trajectory(&mut trajectory)?;
    progress(
        "saved",
        json!({"id": id, "steps": trajectory.steps.len(), "state_hash": sim.world.state_hash().ok()}),
    );
    Ok(id)
}

/// The application a procedure is about: the `app` most of its targets name, else the
/// procedure's domain.
pub fn app_of_plan(plan: &Plan) -> String {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for (_, g) in &plan.gestures {
        let t: Option<&ProcTarget> = match g {
            Gesture::Click { target, .. }
            | Gesture::DoubleClick { target, .. }
            | Gesture::Scroll { target, .. } => Some(target),
            Gesture::Drag { from, .. } => Some(from),
            _ => None,
        };
        if let Some(app) = t.and_then(|t| t.app.clone()) {
            *counts.entry(app).or_default() += 1;
        }
        if let Some(launch) = t
            .and_then(|t| t.interaction.as_deref())
            .and_then(|i| i.strip_prefix("shell:launch:"))
        {
            *counts.entry(launch.to_owned()).or_default() += 3;
        }
    }
    counts
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .map(|(a, _)| a)
        .unwrap_or_else(|| plan.instance.procedure.domain.clone())
}

pub struct ReplayOptions {
    pub reference: String,
    pub store: PathBuf,
    pub procedure: Option<PathBuf>,
    pub world: Option<PathBuf>,
    pub world_seed: u64,
    /// Resolve each step's recorded target in the simulator; else deliver the
    /// recorded coordinates.
    pub by_target: bool,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub config: CompareConfig,
    pub save_comparison: bool,
}

/// Replay a reference trajectory's stimuli in the simulator, comparing after each.
pub fn replay(opts: &ReplayOptions) -> Result<(String, Option<TrajectoryComparison>)> {
    let store = Store::open(&opts.store)?;
    let reference = store.load_trajectory(&opts.reference)?;
    let proc_ref = reference
        .procedure
        .clone()
        .ok_or("the reference has no procedure; pass --procedure")?;
    let procedure_path = match (&opts.procedure, &proc_ref.path) {
        (Some(p), _) => p.clone(),
        (None, Some(p)) if Path::new(p).exists() => PathBuf::from(p),
        _ => {
            let world = proc_ref
                .world
                .clone()
                .ok_or("the reference names no world folder; pass --procedure")?;
            PathBuf::from("worlds")
                .join(world)
                .join("procedures")
                .join(format!("{}.json", proc_ref.id))
        }
    };
    let world_path = opts.world.clone().or_else(|| {
        proc_ref
            .world
            .as_ref()
            .map(|w| PathBuf::from("worlds").join(w).join("world.json"))
            .filter(|p| p.exists())
    });
    let plan = plan(
        &procedure_path,
        world_path.as_deref(),
        proc_ref.seed,
        &BTreeMap::new(),
    )?;
    let width = opts.width.unwrap_or(reference.fingerprint.display.width);
    let height = opts.height.unwrap_or(reference.fingerprint.display.height);
    let mut sim = Sim::boot(&plan, opts.world_seed, width, height)?;
    let mut trajectory = Trajectory::new(Side::Simulated, sim.fingerprint(&reference.app));
    trajectory.procedure = Some(plan.reference.clone());
    trajectory
        .notes
        .insert("replay_of".into(), reference.id.clone());
    trajectory.notes.insert(
        "by".into(),
        if opts.by_target {
            "target".into()
        } else {
            "coordinates".into()
        },
    );
    sim.last_walk = sim.walk_home();
    let (initial, _) = sim.observe(&store, true)?;
    trajectory.initial = initial;
    let mut comparisons = Vec::new();
    let load = |f: &FrameRef| store.load_frame(f);
    let c0 = compare_step(
        None,
        &reference.initial,
        &trajectory.initial,
        load_opt(&reference.initial, &load).as_ref(),
        load_opt(&trajectory.initial, &load).as_ref(),
        None,
        None,
        &opts.config,
    );
    progress(
        "compare",
        json!({"step": null, "divergent": c0.divergent, "findings": c0.findings.len(), "overall": c0.fidelity.overall}),
    );
    let mut stop = opts.config.stop_at_first && c0.divergent;
    comparisons.push(c0);
    let gestures: Vec<Option<&Gesture>> = (0..reference.steps.len())
        .map(|i| plan.gestures.get(i).map(|(_, g)| g))
        .collect();
    for (i, rs) in reference.steps.iter().enumerate() {
        if stop {
            break;
        }
        let (target, resolved, stimulus, ok, error) = if opts.by_target {
            match (rs.target.as_ref(), gestures[i]) {
                (Some(_), Some(g)) if !matches!(g, Gesture::Wait { .. }) => sim.perform(g),
                _ => {
                    let (ok, error) = sim.deliver(&rs.stimulus);
                    (rs.target.clone(), None, rs.stimulus.clone(), ok, error)
                }
            }
        } else {
            let (ok, error) = sim.deliver(&rs.stimulus);
            (rs.target.clone(), None, rs.stimulus.clone(), ok, error)
        };
        let (observation, _) = sim.observe(&store, true)?;
        let step = Step {
            index: i as u32,
            phase: rs.phase.clone(),
            label: rs.label.clone(),
            stimulus,
            target,
            resolved,
            ok,
            error,
            observation,
        };
        let c = compare_step(
            Some(i as u32),
            &rs.observation,
            &step.observation,
            load_opt(&rs.observation, &load).as_ref(),
            load_opt(&step.observation, &load).as_ref(),
            Some(rs),
            Some(&step),
            &opts.config,
        );
        progress(
            "compare",
            json!({"step": i, "stimulus": describe(&step.stimulus), "ok": step.ok, "divergent": c.divergent, "findings": c.findings.len(), "over": c.findings.iter().filter(|f| f.over).count(), "overall": c.fidelity.overall, "worst": c.findings.iter().filter(|f| f.over).map(|f| f.detail.clone()).next()}),
        );
        stop = opts.config.stop_at_first && c.divergent;
        comparisons.push(c);
        trajectory.steps.push(step);
    }
    trajectory.created = now_iso();
    let id = store.save_trajectory(&mut trajectory)?;
    // Assemble the trajectory comparison from the per-step ones so a stopped replay
    // still has a record.
    let mut full = cw_fidelity::cascade::compare_trajectories(
        &reference,
        &trajectory,
        &load,
        &CompareConfig {
            pixel_metrics: false,
            ..opts.config.clone()
        },
    );
    full.steps = comparisons;
    full.first_divergence = full
        .steps
        .iter()
        .find(|s| s.divergent)
        .map(|s| cw_fidelity::cascade::At::of(s.step));
    full.fidelity = cw_fidelity::cascade::FidelityVector::mean(
        &full.steps.iter().map(|s| s.fidelity).collect::<Vec<_>>(),
    );
    full.summary = summarize(&full);
    full.simulated = id.clone();
    let comparison = if opts.save_comparison {
        store.save_comparison(&mut full)?;
        Some(full)
    } else {
        Some(full)
    };
    progress(
        "saved",
        json!({"id": id, "steps": trajectory.steps.len(), "first_divergence": comparison.as_ref().and_then(|c| c.first_divergence)}),
    );
    Ok((id, comparison))
}

fn load_opt(o: &Observation, load: &dyn Fn(&FrameRef) -> Option<Image>) -> Option<Image> {
    o.frame.as_ref().and_then(load)
}

pub fn summarize(c: &TrajectoryComparison) -> cw_fidelity::cascade::ComparisonSummary {
    use cw_fidelity::cascade::{severity, ComparisonSummary};
    let mut summary = ComparisonSummary {
        steps_compared: c.steps.len() as u32,
        ..Default::default()
    };
    let mut worst: Vec<(f32, Option<u32>, String, String)> = Vec::new();
    for s in &c.steps {
        if s.divergent {
            summary.divergent_steps += 1;
        }
        for f in &s.findings {
            summary.findings += 1;
            if f.over {
                summary.findings_over += 1;
                *summary.by_kind.entry(format!("{:?}", f.kind)).or_default() += 1;
                worst.push((
                    severity(f),
                    s.step,
                    format!("{:?}", f.kind),
                    f.detail.clone(),
                ));
            }
        }
    }
    worst.sort_by(|x, y| y.0.partial_cmp(&x.0).unwrap());
    summary.worst = worst
        .into_iter()
        .take(12)
        .map(|(_, s, k, d)| (s, k, d))
        .collect();
    summary
}
