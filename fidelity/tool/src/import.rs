//! Gold from elsewhere: a computer-use trace, a video with an event sidecar, or an rrweb
//! recording, read into the same trajectory shape the Xvfb capture produces.
use crate::pngio::decode_png;
use crate::store::Store;
use crate::util::{err, now_iso, progress, read_json, Result};
use cw_fidelity::normalize::canonical_role;
use cw_fidelity::*;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

/// Fingerprint fields a caller may set from the command line.
#[derive(Clone, Debug, Default)]
pub struct FingerprintOverrides {
    pub app: Option<String>,
    pub os_family: Option<String>,
    pub os_version: Option<String>,
    pub app_version: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub toolkit: Option<String>,
    pub theme: Option<String>,
    pub locale: Option<String>,
}
impl FingerprintOverrides {
    fn apply(&self, fp: &mut Fingerprint) {
        if let Some(v) = &self.app {
            fp.app = v.clone();
        }
        if let Some(v) = &self.os_family {
            fp.os_family = v.clone();
        }
        if let Some(v) = &self.os_version {
            fp.os_version = v.clone();
        }
        if let Some(v) = &self.app_version {
            fp.app_version = v.clone();
        }
        if let Some(v) = self.width {
            fp.display.width = v;
        }
        if let Some(v) = self.height {
            fp.display.height = v;
        }
        if let Some(v) = &self.toolkit {
            fp.toolkit = Some(v.clone());
        }
        if let Some(v) = &self.theme {
            fp.theme = Some(v.clone());
        }
        if let Some(v) = &self.locale {
            fp.locale = Some(v.clone());
        }
    }
}

fn load_fingerprint(dir: &Path, over: &FingerprintOverrides) -> Result<Fingerprint> {
    let mut fp: Fingerprint = if dir.join("fingerprint.json").exists() {
        read_json(&dir.join("fingerprint.json"))?
    } else {
        Fingerprint::default()
    };
    over.apply(&mut fp);
    if fp.app.is_empty() {
        return err("the fingerprint names no application; pass --app");
    }
    Ok(fp)
}

/// One line of `trace.jsonl`.
#[derive(Deserialize)]
struct TraceLine {
    /// Milliseconds since the trace started.
    #[serde(default)]
    t: u64,
    /// Absent on the initial line.
    #[serde(default)]
    stimulus: Option<Stimulus>,
    #[serde(default)]
    target: Option<Target>,
    /// PNG beside `trace.jsonl`, the screen after the stimulus.
    #[serde(default)]
    frame: Option<String>,
    /// JSON `SemTree` beside `trace.jsonl` (a macOS AX dump, an iOS hierarchy).
    #[serde(default)]
    tree: Option<String>,
    #[serde(default)]
    phase: Option<String>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default = "yes")]
    ok: bool,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    files: Vec<FileChange>,
    #[serde(default)]
    clipboard: Option<String>,
}
fn yes() -> bool {
    true
}

fn observe_files(
    store: &Store,
    dir: &Path,
    frame: Option<&str>,
    tree: Option<&str>,
    t: u64,
) -> Result<Observation> {
    let mut o = Observation {
        time: t,
        settle: Settle {
            frames: 1,
            waited: 0,
            stable: true,
        },
        ..Default::default()
    };
    if let Some(f) = frame {
        let bytes = std::fs::read(dir.join(f)).map_err(|e| format!("{f}: {e}"))?;
        let img = decode_png(&bytes)?;
        o.frame = Some(store.put_frame(&img)?);
    }
    if let Some(tp) = tree {
        let tree: SemTree = read_json(&dir.join(tp))?;
        let n = normalize(&tree, &NormalizeOptions::default());
        o.semantic_hash = Some(cw_fidelity::normalize::tree_hash_hex(
            &n,
            cw_fidelity::normalize::HashFields::Content,
        ));
        o.semantic = Some(tree);
    }
    Ok(o)
}

/// `cwfid import DIR`: `trace.jsonl` plus PNGs (and optional tree JSONs) in one folder.
pub fn import_trace(
    store: &Store,
    dir: &Path,
    over: &FingerprintOverrides,
    source: &str,
    provenance: &str,
) -> Result<String> {
    let fp = load_fingerprint(dir, over)?;
    let text = std::fs::read_to_string(dir.join("trace.jsonl"))
        .map_err(|e| format!("trace.jsonl: {e}"))?;
    let mut t = Trajectory::new(Side::Reference, fp);
    t.source = Source {
        kind: source.to_owned(),
        provenance: if provenance.is_empty() {
            dir.to_string_lossy().into_owned()
        } else {
            provenance.to_owned()
        },
        tool: format!("cwfid import {}", env!("CARGO_PKG_VERSION")),
    };
    let mut index = 0u32;
    let mut first = true;
    for (n, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let l: TraceLine =
            serde_json::from_str(line).map_err(|e| format!("trace.jsonl line {}: {e}", n + 1))?;
        let o = observe_files(store, dir, l.frame.as_deref(), l.tree.as_deref(), l.t)?;
        match l.stimulus {
            None if first => t.initial = o,
            None => {
                return err(format!(
                "trace.jsonl line {}: no stimulus (only the first line may be the initial screen)",
                n + 1
            ))
            }
            Some(stimulus) => {
                let mut o = o;
                o.effects.files = l.files;
                o.effects.clipboard = l.clipboard;
                t.steps.push(Step {
                    index,
                    phase: l.phase,
                    label: l.label,
                    stimulus,
                    target: l.target,
                    resolved: None,
                    ok: l.ok,
                    error: l.error,
                    observation: o,
                });
                index += 1;
            }
        }
        first = false;
    }
    if t.initial.frame.is_none() && t.initial.semantic.is_none() {
        progress(
            "import.no_initial",
            json!({"note": "the trace has no initial screen line; comparisons start at step 0"}),
        );
    }
    t.created = now_iso();
    let id = store.save_trajectory(&mut t)?;
    progress("saved", json!({"id": id, "steps": t.steps.len()}));
    Ok(id)
}

/// `cwfid import-video VIDEO EVENTS.jsonl`: frames sampled with ffmpeg `settle_ms` after
/// each event's time. Events are `{"t": ms, "stimulus": {...}, "target"?: {...}}`.
pub fn import_video(
    store: &Store,
    video: &Path,
    events: &Path,
    over: &FingerprintOverrides,
    settle_ms: u64,
    start_ms: u64,
) -> Result<String> {
    let dir = events.parent().unwrap_or(Path::new("."));
    let fp = load_fingerprint(dir, over)?;
    let text = std::fs::read_to_string(events).map_err(|e| format!("{}: {e}", events.display()))?;
    let grab = |ms: u64| -> Result<Image> {
        let ts = format!("{}.{:03}", ms / 1000, ms % 1000);
        let out = run_bytes(
            "ffmpeg",
            &[
                "-loglevel",
                "error",
                "-ss",
                &ts,
                "-i",
                &video.to_string_lossy(),
                "-frames:v",
                "1",
                "-f",
                "image2",
                "-vcodec",
                "png",
                "pipe:1",
            ],
            Duration::from_secs(120),
        )?;
        decode_png(&out)
    };
    let mut t = Trajectory::new(Side::Reference, fp);
    t.source = Source {
        kind: "video".into(),
        provenance: video.to_string_lossy().into_owned(),
        tool: format!("cwfid import-video {}", env!("CARGO_PKG_VERSION")),
    };
    let initial = grab(start_ms)?;
    t.fingerprint.display.width = initial.width;
    t.fingerprint.display.height = initial.height;
    t.initial = Observation {
        frame: Some(store.put_frame(&initial)?),
        time: start_ms,
        settle: Settle {
            frames: 1,
            waited: 0,
            stable: true,
        },
        ..Default::default()
    };
    let mut index = 0;
    for (n, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let l: TraceLine =
            serde_json::from_str(line).map_err(|e| format!("events line {}: {e}", n + 1))?;
        let Some(stimulus) = l.stimulus else { continue };
        let at = l.t + settle_ms;
        let img = grab(at)?;
        let o = Observation {
            frame: Some(store.put_frame(&img)?),
            time: at,
            settle: Settle {
                frames: 1,
                waited: settle_ms,
                stable: true,
            },
            ..Default::default()
        };
        progress("frame", json!({"index": index, "t": at}));
        t.steps.push(Step {
            index,
            phase: l.phase,
            label: l.label,
            stimulus,
            target: l.target,
            resolved: None,
            ok: l.ok,
            error: l.error,
            observation: o,
        });
        index += 1;
    }
    t.created = now_iso();
    let id = store.save_trajectory(&mut t)?;
    progress("saved", json!({"id": id, "steps": t.steps.len()}));
    Ok(id)
}

fn run_bytes(cmd: &str, args: &[&str], timeout: Duration) -> Result<Vec<u8>> {
    use std::process::{Command, Stdio};
    let mut child = Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{cmd}: {e}"))?;
    let start = std::time::Instant::now();
    loop {
        if let Ok(Some(_)) = child.try_wait() {
            break;
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            return err(format!("{cmd} timed out"));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return err(format!(
            "{cmd}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(out.stdout)
}

// ---------------------------------------------------------------------------------------
// rrweb

#[derive(Default, Clone)]
struct DomNode {
    kind: u8, // 0 document, 1 doctype, 2 element, 3 text, 4 cdata, 5 comment
    tag: String,
    attrs: BTreeMap<String, String>,
    text: String,
    children: Vec<u64>,
    parent: Option<u64>,
}

#[derive(Default)]
struct Dom {
    nodes: BTreeMap<u64, DomNode>,
    root: Option<u64>,
    width: u32,
    height: u32,
    href: String,
}
impl Dom {
    fn add_snapshot(&mut self, v: &Value, parent: Option<u64>) -> Option<u64> {
        let id = v.get("id")?.as_u64()?;
        let kind = v.get("type")?.as_u64()? as u8;
        let mut node = DomNode {
            kind,
            parent,
            ..Default::default()
        };
        if kind == 2 {
            node.tag = v
                .get("tagName")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_lowercase();
            if let Some(a) = v.get("attributes").and_then(Value::as_object) {
                for (k, x) in a {
                    node.attrs.insert(
                        k.clone(),
                        match x {
                            Value::String(s) => s.clone(),
                            other => other.to_string(),
                        },
                    );
                }
            }
        } else if kind == 3 {
            node.text = v
                .get("textContent")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
        }
        self.nodes.insert(id, node);
        if let Some(children) = v.get("childNodes").and_then(Value::as_array) {
            for c in children {
                if let Some(cid) = self.add_snapshot(c, Some(id)) {
                    self.nodes.get_mut(&id).unwrap().children.push(cid);
                }
            }
        }
        if parent.is_none() {
            self.root = Some(id);
        }
        Some(id)
    }
    fn remove(&mut self, id: u64) {
        if let Some(n) = self.nodes.remove(&id) {
            if let Some(p) = n.parent.and_then(|p| self.nodes.get_mut(&p)) {
                p.children.retain(|c| *c != id);
            }
            for c in n.children {
                self.remove(c);
            }
        }
    }
    fn mutate(&mut self, d: &Value) {
        for r in d
            .get("removes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(id) = r.get("id").and_then(Value::as_u64) {
                self.remove(id);
            }
        }
        for a in d
            .get("adds")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let parent = a.get("parentId").and_then(Value::as_u64);
            let next = a.get("nextId").and_then(Value::as_u64);
            if let Some(node) = a.get("node") {
                if let Some(id) = self.add_snapshot(node, parent) {
                    if let Some(p) = parent.and_then(|p| self.nodes.get_mut(&p)) {
                        p.children.retain(|c| *c != id);
                        let pos = next
                            .and_then(|n| p.children.iter().position(|c| *c == n))
                            .unwrap_or(p.children.len());
                        p.children.insert(pos, id);
                    }
                }
            }
        }
        for t in d
            .get("texts")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let (Some(id), Some(v)) = (t.get("id").and_then(Value::as_u64), t.get("value")) {
                if let Some(n) = self.nodes.get_mut(&id) {
                    n.text = v.as_str().unwrap_or("").to_owned();
                }
            }
        }
        for a in d
            .get("attributes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let (Some(id), Some(attrs)) = (
                a.get("id").and_then(Value::as_u64),
                a.get("attributes").and_then(Value::as_object),
            ) {
                if let Some(n) = self.nodes.get_mut(&id) {
                    for (k, v) in attrs {
                        match v {
                            Value::Null => {
                                n.attrs.remove(k);
                            }
                            Value::String(s) => {
                                n.attrs.insert(k.clone(), s.clone());
                            }
                            other => {
                                n.attrs.insert(k.clone(), other.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    fn text_of(&self, id: u64, depth: usize) -> String {
        let Some(n) = self.nodes.get(&id) else {
            return String::new();
        };
        if n.kind == 3 {
            return n.text.clone();
        }
        if depth > 6 || matches!(n.tag.as_str(), "script" | "style" | "noscript") {
            return String::new();
        }
        n.children
            .iter()
            .map(|c| self.text_of(*c, depth + 1))
            .collect::<Vec<_>>()
            .join(" ")
    }
    fn role_of(&self, n: &DomNode) -> Option<&'static str> {
        if let Some(r) = n.attrs.get("role") {
            let r = canonical_role(r);
            if r != "unknown" {
                return Some(r);
            }
        }
        let t = n
            .attrs
            .get("type")
            .map(|s| s.to_lowercase())
            .unwrap_or_default();
        Some(match n.tag.as_str() {
            "a" if n.attrs.contains_key("href") => "link",
            "button" | "summary" => "button",
            "input" => match t.as_str() {
                "checkbox" => "checkbox",
                "radio" => "radio",
                "range" => "slider",
                "number" => "spinbutton",
                "search" => "searchbox",
                "submit" | "button" | "reset" | "image" => "button",
                "hidden" => return None,
                _ => "textbox",
            },
            "textarea" => "textbox",
            "select" => "combobox",
            "option" => "option",
            "img" | "svg" | "picture" => "img",
            "canvas" | "video" => "canvas",
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => "heading",
            "nav" | "menu" => "menubar",
            "li" => "option",
            "ul" | "ol" => "list",
            "table" => "table",
            "tr" => "row",
            "td" => "cell",
            "th" => "columnheader",
            "label" => "label",
            "dialog" => "dialog",
            "progress" => "progressbar",
            "hr" => "separator",
            "p" | "span" | "div" | "em" | "strong" | "b" | "i" | "small" | "code" | "pre"
            | "blockquote" | "figcaption" | "dt" | "dd" | "legend" | "cite" | "q" | "time"
            | "address" | "sup" | "sub" | "mark" | "abbr" => "text",
            "body" | "main" | "section" | "article" | "header" | "footer" | "aside" | "form"
            | "fieldset" | "figure" | "details" | "tbody" | "thead" | "tfoot" | "dl" | "html"
            | "iframe" | "center" | "font" | "u" | "s" | "tt" | "ins" | "del" => "panel",
            "script" | "style" | "noscript" | "head" | "meta" | "link" | "title" | "template"
            | "br" | "wbr" | "base" => return None,
            _ => "panel",
        })
    }
    fn name_of(&self, id: u64, n: &DomNode, role: &str) -> String {
        for k in [
            "aria-label",
            "alt",
            "title",
            "placeholder",
            "aria-placeholder",
        ] {
            if let Some(v) = n.attrs.get(k) {
                if !v.trim().is_empty() {
                    return v.trim().to_owned();
                }
            }
        }
        if n.tag == "input" {
            if let Some(v) = n.attrs.get("value") {
                if matches!(role, "button") {
                    return v.clone();
                }
            }
        }
        if matches!(
            role,
            "text"
                | "label"
                | "heading"
                | "link"
                | "button"
                | "option"
                | "cell"
                | "columnheader"
                | "tab"
                | "menuitem"
                | "treeitem"
        ) {
            let t = self.text_of(id, 0);
            let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
            return t.chars().take(200).collect();
        }
        String::new()
    }
    fn hidden(&self, n: &DomNode) -> bool {
        n.attrs.contains_key("hidden")
            || n.attrs.get("aria-hidden").is_some_and(|v| v == "true")
            || n.attrs.get("style").is_some_and(|s| {
                let s = s.replace(' ', "").to_lowercase();
                s.contains("display:none") || s.contains("visibility:hidden")
            })
    }
    fn sem(&self, id: u64, desktop: Rect) -> Vec<SemNode> {
        let Some(n) = self.nodes.get(&id) else {
            return vec![];
        };
        if n.kind == 3 {
            let t = n.text.split_whitespace().collect::<Vec<_>>().join(" ");
            if t.is_empty() {
                return vec![];
            }
            let mut node = SemNode::new("text", t, desktop);
            node.attrs.insert("geometry".into(), "none".into());
            return vec![node];
        }
        if n.kind != 2 {
            return n
                .children
                .iter()
                .flat_map(|c| self.sem(*c, desktop))
                .collect();
        }
        let Some(role) = self.role_of(n) else {
            return vec![];
        };
        if self.hidden(n) {
            return vec![];
        }
        let name = self.name_of(id, n, role);
        let mut node = SemNode::new(role, name.clone(), desktop);
        node.states.enabled = !n.attrs.contains_key("disabled")
            && n.attrs.get("aria-disabled").is_none_or(|v| v != "true");
        node.states.focusable =
            cw_fidelity::normalize::is_interactive(role) || n.attrs.contains_key("tabindex");
        if role == "checkbox" || role == "radio" || role == "switch" {
            node.states.checked = Some(
                n.attrs.contains_key("checked")
                    || n.attrs.get("aria-checked").is_some_and(|v| v == "true"),
            );
        }
        if let Some(v) = n.attrs.get("aria-expanded") {
            node.states.expanded = Some(v == "true");
        }
        if let Some(v) = n.attrs.get("aria-selected") {
            node.states.selected = Some(v == "true");
        }
        if matches!(
            role,
            "textbox" | "searchbox" | "spinbutton" | "slider" | "combobox"
        ) {
            node.value = n
                .attrs
                .get("value")
                .cloned()
                .or_else(|| (n.tag == "textarea").then(|| self.text_of(id, 0)));
        }
        if let Some(i) = n.attrs.get("id") {
            node.source_id = Some(format!("#{i}"));
        } else {
            node.source_id = Some(format!("rr:{id}"));
        }
        node.attrs.insert("geometry".into(), "none".into());
        node.attrs.insert("tag".into(), n.tag.clone());
        // A named leaf-like control swallows its text; everything else keeps children.
        let swallow = matches!(
            role,
            "button"
                | "link"
                | "heading"
                | "option"
                | "cell"
                | "columnheader"
                | "label"
                | "tab"
                | "menuitem"
        ) && !name.is_empty();
        if !swallow {
            node.children = n
                .children
                .iter()
                .flat_map(|c| self.sem(*c, desktop))
                .collect();
            if role == "text"
                && node.children.len() == 1
                && node.children[0].role == "text"
                && node.name.is_empty()
            {
                return node.children;
            }
            if role == "text" && node.children.is_empty() && node.name.is_empty() {
                return vec![];
            }
        }
        vec![node]
    }
    fn tree(&self) -> SemTree {
        let desktop = Rect::new(0, 0, self.width.max(1) as i32, self.height.max(1) as i32);
        let mut t = SemTree::desktop(self.width.max(1), self.height.max(1));
        let mut page = SemNode::new("window", self.title(), desktop);
        page.attrs.insert("app".into(), "browser".into());
        page.attrs.insert("document".into(), self.href.clone());
        page.attrs.insert("geometry".into(), "none".into());
        if let Some(root) = self.root {
            page.children = self.sem(root, desktop);
        }
        t.root.children.push(page);
        t.active_window = Some(vec![0]);
        t
    }
    fn title(&self) -> String {
        for (id, n) in &self.nodes {
            if n.kind == 2 && n.tag == "title" {
                return self.text_of(*id, 0).trim().to_owned();
            }
        }
        self.href.clone()
    }
}

/// `cwfid import-rrweb events.json`: an rrweb recording (an array of events, or one per
/// line) into a reference trajectory of DOM-derived semantic trees. No geometry is in a
/// recording, so bounds are the viewport and layout is not compared.
pub fn import_rrweb(store: &Store, path: &Path, over: &FingerprintOverrides) -> Result<String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let events: Vec<Value> = match serde_json::from_str::<Value>(&text) {
        Ok(Value::Array(a)) => a,
        Ok(Value::Object(o)) => o
            .get("events")
            .and_then(Value::as_array)
            .cloned()
            .ok_or("rrweb: no events array")?,
        _ => text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>>>()?,
    };
    let mut fp = Fingerprint {
        os_family: "web".into(),
        app: "browser".into(),
        toolkit: Some("dom".into()),
        ..Default::default()
    };
    over.apply(&mut fp);
    let mut t = Trajectory::new(Side::Reference, fp);
    t.source = Source {
        kind: "rrweb".into(),
        provenance: path.to_string_lossy().into_owned(),
        tool: format!("cwfid import-rrweb {}", env!("CARGO_PKG_VERSION")),
    };
    let mut dom = Dom::default();
    let mut t0: Option<u64> = None;
    let mut pending: Option<(u64, Stimulus, Option<Target>)> = None;
    let mut have_initial = false;
    let mut index = 0u32;
    let mut scroll: BTreeMap<u64, i64> = BTreeMap::new();
    let mut last_input: BTreeMap<u64, String> = BTreeMap::new();
    let observe = |dom: &Dom, time: u64| -> Observation {
        let tree = dom.tree();
        let n = normalize(&tree, &NormalizeOptions::default());
        Observation {
            semantic_hash: Some(cw_fidelity::normalize::tree_hash_hex(
                &n,
                cw_fidelity::normalize::HashFields::Content,
            )),
            semantic: Some(tree),
            time,
            settle: Settle {
                frames: 1,
                waited: 0,
                stable: true,
            },
            ..Default::default()
        }
    };
    let flush = |t: &mut Trajectory,
                 dom: &Dom,
                 pending: &mut Option<(u64, Stimulus, Option<Target>)>,
                 index: &mut u32,
                 time: u64| {
        if let Some((_, stimulus, target)) = pending.take() {
            t.steps.push(Step {
                index: *index,
                phase: None,
                label: None,
                stimulus,
                target,
                resolved: None,
                ok: true,
                error: None,
                observation: observe(dom, time),
            });
            *index += 1;
        }
    };
    for e in &events {
        let ty = e.get("type").and_then(Value::as_u64).unwrap_or(99);
        let ts = e.get("timestamp").and_then(Value::as_u64).unwrap_or(0);
        let t0v = *t0.get_or_insert(ts);
        let rel = ts.saturating_sub(t0v);
        let data = e.get("data").cloned().unwrap_or(Value::Null);
        match ty {
            4 => {
                dom.width = data.get("width").and_then(Value::as_u64).unwrap_or(1280) as u32;
                dom.height = data.get("height").and_then(Value::as_u64).unwrap_or(800) as u32;
                dom.href = data
                    .get("href")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                t.fingerprint.display.width = dom.width;
                t.fingerprint.display.height = dom.height;
            }
            2 => {
                dom.nodes.clear();
                dom.root = None;
                if let Some(node) = data.get("node") {
                    dom.add_snapshot(node, None);
                }
                if !have_initial {
                    t.initial = observe(&dom, rel);
                    have_initial = true;
                }
            }
            3 => {
                let source = data.get("source").and_then(Value::as_u64).unwrap_or(99);
                match source {
                    0 => dom.mutate(&data),
                    2 => {
                        // Mouse interaction: 0 MouseUp 1 MouseDown 2 Click 3 ContextMenu 4 DblClick 5 Focus 6 Blur 7 TouchStart 9 TouchEnd
                        let kind = data.get("type").and_then(Value::as_u64).unwrap_or(99);
                        let x = data.get("x").and_then(Value::as_f64).unwrap_or(0.0).round() as i32;
                        let y = data.get("y").and_then(Value::as_f64).unwrap_or(0.0).round() as i32;
                        let id = data.get("id").and_then(Value::as_u64);
                        let op = match kind {
                            2 => Some(PointerOp::Click),
                            4 => Some(PointerOp::DoubleClick),
                            3 => Some(PointerOp::Click),
                            _ => None,
                        };
                        if let Some(op) = op {
                            flush(&mut t, &dom, &mut pending, &mut index, rel);
                            let target = id.and_then(|i| dom.nodes.get(&i)).and_then(|n| {
                                let role = dom.role_of(n)?;
                                let name = dom.name_of(id.unwrap(), n, role);
                                Some(Target {
                                    role: Some(role.to_owned()),
                                    label: (!name.is_empty()).then_some(name),
                                    ..Default::default()
                                })
                            });
                            pending = Some((
                                rel,
                                Stimulus::Pointer {
                                    op,
                                    x,
                                    y,
                                    button: if kind == 3 { 3 } else { 1 },
                                    modifiers: vec![],
                                    delta_x: 0,
                                    delta_y: 0,
                                },
                                target,
                            ));
                        }
                    }
                    5 => {
                        // Input: the field's whole value after the keystroke.
                        let id = data.get("id").and_then(Value::as_u64).unwrap_or(0);
                        let text = data
                            .get("text")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_owned();
                        let prev = last_input.get(&id).cloned().unwrap_or_default();
                        let typed = text.strip_prefix(prev.as_str()).map(str::to_owned);
                        last_input.insert(id, text.clone());
                        if let Some(n) = dom.nodes.get_mut(&id) {
                            n.attrs.insert("value".into(), text.clone());
                            if let Some(c) = data.get("isChecked").and_then(Value::as_bool) {
                                if c {
                                    n.attrs.insert("checked".into(), String::new());
                                } else {
                                    n.attrs.remove("checked");
                                }
                            }
                        }
                        // Consecutive keystrokes into one field merge into one typed step.
                        match (&mut pending, typed) {
                            (Some((_, Stimulus::Type { text: acc }, Some(tg))), Some(more))
                                if tg.interaction.as_deref() == Some(&format!("rr:{id}")) =>
                            {
                                acc.push_str(&more)
                            }
                            (_, typed) => {
                                flush(&mut t, &dom, &mut pending, &mut index, rel);
                                let target = dom.nodes.get(&id).and_then(|n| {
                                    let role = dom.role_of(n)?;
                                    Some(Target {
                                        role: Some(role.to_owned()),
                                        label: Some(dom.name_of(id, n, role))
                                            .filter(|s| !s.is_empty()),
                                        interaction: Some(format!("rr:{id}")),
                                        ..Default::default()
                                    })
                                });
                                pending = Some((
                                    rel,
                                    Stimulus::Type {
                                        text: typed.unwrap_or(text),
                                    },
                                    target,
                                ));
                            }
                        }
                    }
                    3 => {
                        let id = data.get("id").and_then(Value::as_u64).unwrap_or(0);
                        let y = data.get("y").and_then(Value::as_f64).unwrap_or(0.0) as i64;
                        let prev = scroll.insert(id, y).unwrap_or(0);
                        let delta = y - prev;
                        if delta != 0 {
                            flush(&mut t, &dom, &mut pending, &mut index, rel);
                            pending = Some((
                                rel,
                                Stimulus::Pointer {
                                    op: PointerOp::Wheel,
                                    x: (dom.width / 2) as i32,
                                    y: (dom.height / 2) as i32,
                                    button: 0,
                                    modifiers: vec![],
                                    delta_x: 0,
                                    delta_y: delta,
                                },
                                None,
                            ));
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    let last = events
        .last()
        .and_then(|e| e.get("timestamp").and_then(Value::as_u64))
        .unwrap_or(0)
        .saturating_sub(t0.unwrap_or(0));
    flush(&mut t, &dom, &mut pending, &mut index, last);
    if !have_initial {
        return err("rrweb: no full snapshot (type 2) event");
    }
    t.created = now_iso();
    let id = store.save_trajectory(&mut t)?;
    progress(
        "saved",
        json!({"id": id, "steps": t.steps.len(), "href": dom.href}),
    );
    Ok(id)
}

pub fn fingerprint_overrides(get: &dyn Fn(&str) -> Option<String>) -> FingerprintOverrides {
    FingerprintOverrides {
        app: get("app"),
        os_family: get("os"),
        os_version: get("os-version"),
        app_version: get("app-version"),
        width: get("width").and_then(|v| v.parse().ok()),
        height: get("height").and_then(|v| v.parse().ok()),
        toolkit: get("toolkit"),
        theme: get("theme"),
        locale: get("locale"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rrweb_dom_to_tree() {
        let mut dom = Dom {
            width: 800,
            height: 600,
            href: "https://x.test/".into(),
            ..Default::default()
        };
        let snap = json!({"type": 0, "id": 1, "childNodes": [{"type": 2, "id": 2, "tagName": "html", "attributes": {}, "childNodes": [
            {"type": 2, "id": 3, "tagName": "head", "attributes": {}, "childNodes": [{"type": 2, "id": 4, "tagName": "title", "attributes": {}, "childNodes": [{"type": 3, "id": 5, "textContent": "Shop"}]}]},
            {"type": 2, "id": 6, "tagName": "body", "attributes": {}, "childNodes": [
                {"type": 2, "id": 7, "tagName": "button", "attributes": {"class": "x"}, "childNodes": [{"type": 3, "id": 8, "textContent": " Add to cart "}]},
                {"type": 2, "id": 9, "tagName": "input", "attributes": {"type": "text", "placeholder": "Search"}, "childNodes": []},
                {"type": 2, "id": 10, "tagName": "div", "attributes": {"style": "display:none"}, "childNodes": [{"type": 3, "id": 11, "textContent": "hidden"}]},
                {"type": 2, "id": 12, "tagName": "p", "attributes": {}, "childNodes": [{"type": 3, "id": 13, "textContent": "Welcome"}]}
            ]}
        ]}]});
        dom.add_snapshot(&snap, None);
        let tree = dom.tree();
        assert_eq!(tree.root.children[0].name, "Shop");
        let n = normalize(&tree, &NormalizeOptions::default());
        let w = &n.root.children[0];
        let roles: Vec<(&str, &str)> = w
            .children
            .iter()
            .map(|c| (c.role.as_str(), c.name.as_str()))
            .collect();
        assert!(roles.contains(&("button", "Add to cart")), "{roles:?}");
        assert!(roles.contains(&("textbox", "Search")), "{roles:?}");
        assert!(roles.contains(&("text", "Welcome")), "{roles:?}");
        assert!(!roles.iter().any(|(_, n)| *n == "hidden"));
        dom.mutate(&json!({"removes": [{"id": 7, "parentId": 6}], "adds": [], "texts": [{"id": 13, "value": "Bye"}], "attributes": []}));
        let n2 = normalize(&dom.tree(), &NormalizeOptions::default());
        let roles: Vec<(&str, &str)> = n2.root.children[0]
            .children
            .iter()
            .map(|c| (c.role.as_str(), c.name.as_str()))
            .collect();
        assert!(!roles.iter().any(|(r, _)| *r == "button"));
        assert!(roles.contains(&("text", "Bye")));
    }
}
