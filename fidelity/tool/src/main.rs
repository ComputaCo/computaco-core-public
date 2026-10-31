//! `cwfid`: capture, compare, inspect and tune fidelity between a real desktop
//! application and ComputerWorld. Every command prints JSON on stdout unless `--text`
//! is given; progress goes to stderr as JSON lines.
mod atspi;
mod commands;
mod import;
mod pngio;
mod procedure;
mod reference;
mod resolve;
mod sim;
mod store;
mod targetmap;
mod util;
mod vdisplay;
mod xinput;

use cw_fidelity::cascade::CompareConfig;
use cw_fidelity::normalize::NormalizeOptions;
use cw_fidelity::{Rect, Verdict};
use std::collections::BTreeMap;
use std::path::PathBuf;
use store::Store;
use util::Result;

struct Args {
    positional: Vec<String>,
    flags: BTreeMap<String, Vec<String>>,
}
impl Args {
    fn parse(argv: &[String]) -> Self {
        let mut positional = Vec::new();
        let mut flags: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut i = 0;
        while i < argv.len() {
            let a = &argv[i];
            if let Some(name) = a.strip_prefix("--") {
                if let Some((k, v)) = name.split_once('=') {
                    flags.entry(k.to_owned()).or_default().push(v.to_owned());
                } else if i + 1 < argv.len()
                    && !argv[i + 1].starts_with("--")
                    && !is_boolean_flag(name)
                {
                    flags
                        .entry(name.to_owned())
                        .or_default()
                        .push(argv[i + 1].clone());
                    i += 1;
                } else {
                    flags
                        .entry(name.to_owned())
                        .or_default()
                        .push("true".into());
                }
            } else {
                positional.push(a.clone());
            }
            i += 1;
        }
        Self { positional, flags }
    }
    fn get(&self, k: &str) -> Option<&str> {
        self.flags.get(k).and_then(|v| v.last()).map(String::as_str)
    }
    fn all(&self, k: &str) -> Vec<String> {
        self.flags.get(k).cloned().unwrap_or_default()
    }
    fn has(&self, k: &str) -> bool {
        self.flags.contains_key(k)
    }
    fn num<T: std::str::FromStr>(&self, k: &str, default: T) -> Result<T> {
        match self.get(k) {
            None => Ok(default),
            Some(v) => v.parse().map_err(|_| format!("--{k}: not a number: {v}")),
        }
    }
    fn pos(&self, i: usize, what: &str) -> Result<&str> {
        self.positional
            .get(i)
            .map(String::as_str)
            .ok_or_else(|| format!("missing {what}"))
    }
}
fn is_boolean_flag(name: &str) -> bool {
    matches!(
        name,
        "text"
            | "json"
            | "save"
            | "no-save"
            | "keep-sandbox"
            | "no-wm"
            | "by-target"
            | "by-coordinates"
            | "stop-at-first"
            | "continue"
            | "apply"
            | "raw"
            | "no-pixels"
            | "strict-files"
            | "all-widgets"
            | "help"
    )
}

const USAGE: &str = r#"cwfid — fidelity between a real desktop application and ComputerWorld

Store: --store <dir> (default $CWFID_STORE or ./fidelity-store)

Capture
  capture-ref  <procedure.json> --app <id> [--seed N] [--world world.json] [--launch "cmd args"]
               [--size 1280x800] [--home-template DIR] [--sandbox DIR] [--keep-sandbox] [--no-wm]
               [--settle-ms 4000] [--max-steps N] [--skip-steps N] [--target-map kicad.json]... [--param name=value]... [--note k=v]...
               Real application in a private Xvfb; AT-SPI, xwd and the sandbox home observed.
  capture-sim  <procedure.json> [--seed N] [--world-seed N] [--world world.json] [--size WxH]
               [--app id] [--max-steps N] [--param name=value]...
               ComputerWorld performing the same procedure.
  replay       <reference-id> [--by-target|--by-coordinates] [--stop-at-first] [--procedure p.json]
               [--world world.json] [--size WxH] [compare flags]
               Lockstep: the reference's stimuli into the simulator, compared after each.

Import gold captured elsewhere
  import       <DIR> --app <id> [--source computer-use] [--provenance text] [--os linux|macos|ios]
               DIR/trace.jsonl: one JSON per line {"t": ms, "stimulus": {...}, "target"?: {...},
               "frame": "shot.png", "tree"?: "tree.json"}; the first line may omit "stimulus"
               for the initial screen. DIR/fingerprint.json may give the Fingerprint.
  import-video <video> <events.jsonl> --app <id> [--settle-ms 400] [--start-ms 0]
               Frames sampled with ffmpeg after each event's time.
  import-rrweb <events.json> [--app browser] [--os web]
               An rrweb recording's DOM snapshots and interactions as trees and stimuli.

Compare and inspect
  compare      <reference-id> <simulated-id> [--no-save] [--stop-at-first] [--no-pixels]
               [--strict-files] [--all-widgets] [--tolerances file.json] [--flat|--deep]
  report       [--reference id] [--simulated id] [--text]
  inspect      <reference-id> <simulated-id> [--step N] --out DIR [--finding ID]
               Writes trees, frames, marked frames, side-by-side, heat map and per-finding crops.
  tree         <trajectory-id> [--step N] [--raw] [--text]
  frame        <trajectory-id> [--step N] --out file.png
  steps        <trajectory-id> [--text]
  show         <trajectory-id> [--step N]            The step record as JSON.

Feedback and sampling
  annotate     <reference-id> <simulated-id> --step N --verdict matters|cosmetic|ignore|ref_noise
               [--finding ID] [--node 0.1.2] [--region x,y,w,h] [--note text] [--author who]
  tune         [--apply] [--min-support 5]           Thresholds from annotations (Youden's J).
  coverage     [--text] [--limit 40]                 Cells and sampling priorities.
  ls           [--text]                              Rebuild the index and list the store.
  index

Flags: --text for a human rendering where available; everything else is JSON on stdout.
"#;

fn size(args: &Args, default: (u32, u32)) -> Result<(u32, u32)> {
    match args.get("size") {
        None => Ok(default),
        Some(s) => {
            let (w, h) = s.split_once('x').ok_or("--size wants WxH")?;
            Ok((
                w.parse().map_err(|_| "--size width")?,
                h.parse().map_err(|_| "--size height")?,
            ))
        }
    }
}
fn kv(list: Vec<String>) -> BTreeMap<String, String> {
    list.iter()
        .filter_map(|s| s.split_once('='))
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
}
fn compare_config(args: &Args, store: &Store) -> Result<CompareConfig> {
    let mut cfg = CompareConfig {
        tolerances: store.tolerances()?,
        ..Default::default()
    };
    if let Some(p) = args.get("tolerances") {
        cfg.tolerances = util::read_json(&PathBuf::from(p))?;
    }
    if args.has("no-pixels") {
        cfg.pixel_metrics = false;
    }
    if args.has("strict-files") {
        cfg.strict_file_content = true;
    }
    if args.has("all-widgets") {
        cfg.keep_all_widgets = true;
    }
    if args.has("stop-at-first") {
        cfg.stop_at_first = true;
    }
    if args.has("deep") {
        cfg.normalize = NormalizeOptions {
            flatten: false,
            ..NormalizeOptions::default()
        };
    }
    if let Some(n) = args.get("ignore-role") {
        cfg.tolerances.ignore_roles.push(n.to_owned());
    }
    for n in args.all("ignore-name") {
        cfg.tolerances.ignore_names.push(n);
    }
    Ok(cfg)
}
fn out_json<T: serde::Serialize>(v: &T) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(v).map_err(|e| e.to_string())?
    );
    Ok(())
}
fn step_arg(args: &Args) -> Result<Option<u32>> {
    match args.get("step") {
        None | Some("initial") => Ok(None),
        Some(s) => s
            .parse::<u32>()
            .map(Some)
            .map_err(|_| "--step wants a number or `initial`".to_owned()),
    }
}

fn run(args: &Args) -> Result<()> {
    let cmd = args.pos(0, "command")?;
    if args.has("help") || cmd == "help" {
        print!("{USAGE}");
        return Ok(());
    }
    let store_dir = args
        .get("store")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("CWFID_STORE").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("fidelity-store"));
    let store = Store::open(&store_dir)?;
    match cmd {
        "capture-ref" => {
            let procedure = PathBuf::from(args.pos(1, "procedure path")?);
            let app = args
                .get("app")
                .ok_or("--app <id> is required (kicad, libreoffice-calc, …)")?
                .to_owned();
            let mut o = reference::RefOptions::defaults(procedure, store_dir.clone(), app);
            o.world = args.get("world").map(PathBuf::from);
            o.seed = args.num("seed", 1)?;
            o.overrides = kv(args.all("param"));
            (o.width, o.height) = size(args, (1280, 800))?;
            o.launch = args
                .get("launch")
                .map(|l| l.split_whitespace().map(String::from).collect())
                .unwrap_or_default();
            o.home_template = args.get("home-template").map(PathBuf::from);
            o.sandbox = args.get("sandbox").map(PathBuf::from);
            o.keep_sandbox = args.has("keep-sandbox");
            o.settle_ms = args.num("settle-ms", 4000)?;
            o.startup_timeout_s = args.num("startup-timeout", 90)?;
            o.max_steps = args
                .get("max-steps")
                .map(|s| s.parse())
                .transpose()
                .map_err(|_| "--max-steps")?;
            o.skip_steps = args.num("skip-steps", 0)?;
            o.target_maps = args
                .all("target-map")
                .into_iter()
                .map(PathBuf::from)
                .collect();
            o.wm = !args.has("no-wm");
            o.notes = kv(args.all("note"));
            for g in args.all("ignore-path") {
                o.ignore_paths.push(g);
            }
            let id = reference::capture(&o)?;
            out_json(&serde_json::json!({"id": id, "store": store_dir}))
        }
        "capture-sim" => {
            let procedure = PathBuf::from(args.pos(1, "procedure path")?);
            let (width, height) = size(args, (1280, 800))?;
            let o = sim::SimOptions {
                procedure,
                world: args.get("world").map(PathBuf::from),
                seed: args.num("seed", 1)?,
                world_seed: args.num("world-seed", 7)?,
                overrides: kv(args.all("param")),
                store: store_dir.clone(),
                width,
                height,
                app: args.get("app").map(str::to_owned),
                max_steps: args
                    .get("max-steps")
                    .map(|s| s.parse())
                    .transpose()
                    .map_err(|_| "--max-steps")?,
                notes: kv(args.all("note")),
            };
            let id = sim::capture(&o)?;
            out_json(&serde_json::json!({"id": id, "store": store_dir}))
        }
        "replay" => {
            let reference = args.pos(1, "reference id")?.to_owned();
            let config = compare_config(args, &store)?;
            let (w, h) = match args.get("size") {
                Some(_) => {
                    let (w, h) = size(args, (0, 0))?;
                    (Some(w), Some(h))
                }
                None => (None, None),
            };
            let o = sim::ReplayOptions {
                reference,
                store: store_dir.clone(),
                procedure: args.get("procedure").map(PathBuf::from),
                world: args.get("world").map(PathBuf::from),
                world_seed: args.num("world-seed", 7)?,
                by_target: !args.has("by-coordinates"),
                width: w,
                height: h,
                config,
                save_comparison: !args.has("no-save"),
            };
            let (id, comparison) = sim::replay(&o)?;
            if args.has("text") {
                if let Some(c) = &comparison {
                    print!("{}", cw_fidelity::cascade::render_text(c));
                }
                println!("simulated trajectory {id}");
                Ok(())
            } else {
                out_json(
                    &serde_json::json!({"id": id, "comparison": comparison.map(|c| serde_json::json!({"first_divergence": c.first_divergence, "fidelity": c.fidelity, "summary": c.summary, "path": cw_fidelity::store::comparison_path(&c.reference, &c.simulated)}))}),
                )
            }
        }
        "import" => {
            let dir = PathBuf::from(args.pos(1, "trace directory")?);
            let over = import::fingerprint_overrides(&|k| args.get(k).map(str::to_owned));
            let id = import::import_trace(
                &store,
                &dir,
                &over,
                args.get("source").unwrap_or("computer-use"),
                args.get("provenance").unwrap_or(""),
            )?;
            out_json(&serde_json::json!({"id": id, "store": store_dir}))
        }
        "import-video" => {
            let video = PathBuf::from(args.pos(1, "video path")?);
            let events = PathBuf::from(args.pos(2, "events.jsonl path")?);
            let over = import::fingerprint_overrides(&|k| args.get(k).map(str::to_owned));
            let id = import::import_video(
                &store,
                &video,
                &events,
                &over,
                args.num("settle-ms", 400)?,
                args.num("start-ms", 0)?,
            )?;
            out_json(&serde_json::json!({"id": id, "store": store_dir}))
        }
        "import-rrweb" => {
            let path = PathBuf::from(args.pos(1, "rrweb events path")?);
            let over = import::fingerprint_overrides(&|k| args.get(k).map(str::to_owned));
            let id = import::import_rrweb(&store, &path, &over)?;
            out_json(&serde_json::json!({"id": id, "store": store_dir}))
        }
        "compare" => {
            let cfg = compare_config(args, &store)?;
            let c = commands::compare(
                &store,
                args.pos(1, "reference id")?,
                args.pos(2, "simulated id")?,
                &cfg,
                !args.has("no-save"),
            )?;
            if args.has("text") {
                print!("{}", cw_fidelity::cascade::render_text(&c));
                Ok(())
            } else if args.has("full") {
                out_json(&c)
            } else {
                out_json(
                    &serde_json::json!({"reference": c.reference, "simulated": c.simulated, "same_class": c.same_class, "first_divergence": c.first_divergence, "fidelity": c.fidelity, "summary": c.summary, "steps": c.steps.iter().map(|s| serde_json::json!({"step": s.step, "divergent": s.divergent, "overall": s.fidelity.overall, "findings_over": s.findings.iter().filter(|f| f.over).count()})).collect::<Vec<_>>()}),
                )
            }
        }
        "report" => {
            let cs = commands::report(&store, args.get("reference"), args.get("simulated"))?;
            if args.has("text") {
                print!("{}", commands::report_text(&cs));
                Ok(())
            } else {
                out_json(&cs.iter().map(|c| serde_json::json!({"reference": c.reference, "simulated": c.simulated, "app": c.app, "same_class": c.same_class, "first_divergence": c.first_divergence, "fidelity": c.fidelity, "summary": c.summary, "created": c.created})).collect::<Vec<_>>())
            }
        }
        "inspect" => {
            let cfg = compare_config(args, &store)?;
            let out = PathBuf::from(args.get("out").ok_or("--out DIR is required")?);
            let finding = args
                .get("finding")
                .map(|s| s.parse())
                .transpose()
                .map_err(|_| "--finding")?;
            let r = commands::inspect(
                &store,
                args.pos(1, "reference id")?,
                args.pos(2, "simulated id")?,
                step_arg(args)?,
                &out,
                &cfg,
                finding,
            )?;
            out_json(&r)
        }
        "tree" => {
            let cfg = compare_config(args, &store)?;
            let t = commands::show_tree(
                &store,
                args.pos(1, "trajectory id")?,
                step_arg(args)?,
                args.has("raw"),
                &cfg.normalize,
            )?;
            if args.has("text") {
                print!("{}", commands::tree_text(&t));
                Ok(())
            } else {
                out_json(&t)
            }
        }
        "frame" => {
            let out = PathBuf::from(args.get("out").ok_or("--out file.png is required")?);
            commands::frame(&store, args.pos(1, "trajectory id")?, step_arg(args)?, &out)?;
            out_json(&serde_json::json!({"written": out}))
        }
        "steps" => {
            let t = store.load_trajectory(args.pos(1, "trajectory id")?)?;
            if args.has("text") {
                print!("{}", commands::steps_text(&t));
                Ok(())
            } else {
                out_json(&t.steps.iter().map(|s| serde_json::json!({"index": s.index, "phase": s.phase, "stimulus": s.stimulus, "target": s.target, "resolved": s.resolved, "ok": s.ok, "error": s.error, "semantic_hash": s.observation.semantic_hash, "frame": s.observation.frame.as_ref().map(|f| &f.sha256), "files": s.observation.effects.files.len(), "settle": s.observation.settle})).collect::<Vec<_>>())
            }
        }
        "show" => {
            let t = store.load_trajectory(args.pos(1, "trajectory id")?)?;
            match step_arg(args)? {
                None => out_json(
                    &serde_json::json!({"id": t.id, "side": t.side, "app": t.app, "fingerprint": t.fingerprint, "class": t.fingerprint.class(), "source": t.source, "procedure": t.procedure, "created": t.created, "steps": t.steps.len(), "notes": t.notes, "initial": t.initial}),
                ),
                Some(i) => out_json(t.steps.get(i as usize).ok_or("no such step")?),
            }
        }
        "annotate" => {
            let step: u32 = args
                .get("step")
                .ok_or("--step N is required")?
                .parse()
                .map_err(|_| "--step")?;
            let verdict = match args.get("verdict").ok_or("--verdict is required")? {
                "matters" => Verdict::Matters,
                "cosmetic" => Verdict::Cosmetic,
                "ignore" => Verdict::Ignore,
                "ref_noise" | "ref-noise" | "noise" => Verdict::RefNoise,
                other => return Err(format!("unknown verdict {other}")),
            };
            let finding = args
                .get("finding")
                .map(|s| s.parse())
                .transpose()
                .map_err(|_| "--finding")?;
            let node = args
                .get("node")
                .map(|s| {
                    s.split('.')
                        .map(|p| {
                            p.parse::<u32>()
                                .map_err(|_| "--node wants 0.1.2".to_owned())
                        })
                        .collect::<Result<Vec<u32>>>()
                })
                .transpose()?;
            let region = args
                .get("region")
                .map(|s| {
                    let v: Vec<i32> = s.split(',').filter_map(|p| p.parse().ok()).collect();
                    if v.len() == 4 {
                        Ok(Rect::new(v[0], v[1], v[2], v[3]))
                    } else {
                        Err("--region wants x,y,w,h".to_owned())
                    }
                })
                .transpose()?;
            let author = args
                .get("author")
                .map(str::to_owned)
                .unwrap_or_else(|| std::env::var("USER").unwrap_or_else(|_| "unknown".into()));
            let a = commands::annotate(
                &store,
                args.pos(1, "reference id")?,
                args.pos(2, "simulated id")?,
                step,
                verdict,
                finding,
                node,
                region,
                args.get("note").unwrap_or(""),
                &author,
            )?;
            out_json(&a)
        }
        "tune" => {
            let t = commands::tune_store(&store, args.has("apply"), args.num("min-support", 5)?)?;
            out_json(&t)
        }
        "coverage" => {
            let cov = commands::coverage(&store)?;
            if args.has("text") {
                print!("{}", commands::coverage_text(&cov, args.num("limit", 40)?));
                Ok(())
            } else {
                out_json(
                    &serde_json::json!({"totals": cov.totals(), "priorities": cov.priorities().into_iter().take(args.num("limit", 200)?).collect::<Vec<_>>()}),
                )
            }
        }
        "ls" => {
            if args.has("text") {
                print!("{}", commands::ls_text(&store)?);
                Ok(())
            } else {
                out_json(&commands::ls(&store)?)
            }
        }
        "index" => out_json(&store.rebuild_index()?),
        other => Err(format!("unknown command {other}\n{USAGE}")),
    }
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.is_empty() {
        print!("{USAGE}");
        std::process::exit(2);
    }
    let args = Args::parse(&argv);
    if let Err(e) = run(&args) {
        eprintln!("cwfid: {e}");
        std::process::exit(1);
    }
}
