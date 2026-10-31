//! The accessibility tree of a virtual display, read through a resident Python helper
//! (`py/atspi_dump.py`, GObject introspection of libatspi) and turned into a `SemTree`.
use crate::util::{err, Result};
use cw_fidelity::normalize::canonical_role;
use cw_fidelity::{NodePath, Rect, SemNode, SemTree, States};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

pub struct AtSpi {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

#[derive(Deserialize)]
struct RawApp {
    name: String,
    #[serde(default)]
    pid: String,
    #[serde(default)]
    toolkit: String,
    #[serde(default)]
    toolkit_version: String,
    #[serde(default)]
    windows: Vec<RawNode>,
}
#[derive(Deserialize)]
struct RawNode {
    role: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    states: Vec<String>,
    #[serde(default)]
    bounds: Option<[i32; 4]>,
    /// Bounds relative to the toplevel window, when the toolkit gave different ones.
    #[serde(default)]
    window_bounds: Option<[i32; 4]>,
    #[serde(default)]
    value: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    attrs: BTreeMap<String, String>,
    #[serde(default)]
    children: Vec<RawNode>,
}
#[derive(Deserialize)]
struct RawTree {
    #[serde(default)]
    apps: Vec<RawApp>,
    #[serde(default)]
    truncated: bool,
    #[serde(default)]
    ms: u64,
    #[serde(default)]
    error: Option<String>,
}

/// What a read of the tree also tells about the applications on the bus.
#[derive(Clone, Debug, Default)]
pub struct Apps {
    /// (name, pid, toolkit, toolkit version)
    pub list: Vec<(String, String, String, String)>,
    pub truncated: bool,
    pub ms: u64,
}

pub fn helper_path() -> PathBuf {
    if let Ok(p) = std::env::var("CWFID_ATSPI_HELPER") {
        return PathBuf::from(p);
    }
    // Beside the binary's source in the repository, or beside the binary.
    let here = Path::new(env!("CARGO_MANIFEST_DIR")).join("py/atspi_dump.py");
    if here.exists() {
        return here;
    }
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("atspi_dump.py")))
        .unwrap_or(here)
}

impl AtSpi {
    pub fn start(env: &BTreeMap<String, String>) -> Result<Self> {
        let helper = helper_path();
        if !helper.exists() {
            return err(format!("AT-SPI helper not found at {}", helper.display()));
        }
        let mut c = Command::new("python3");
        c.arg("-I")
            .arg(&helper)
            .env_clear()
            .envs(env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .process_group(0);
        let mut child = c
            .spawn()
            .map_err(|e| format!("python3 atspi helper: {e}"))?;
        let stdin = child.stdin.take().ok_or("helper stdin")?;
        let stdout = BufReader::new(child.stdout.take().ok_or("helper stdout")?);
        let mut me = Self {
            child,
            stdin,
            stdout,
        };
        let pong = me.request(r#"{"cmd":"ping"}"#)?;
        if !pong.contains("true") {
            return err(format!("AT-SPI helper did not answer: {pong}"));
        }
        Ok(me)
    }
    fn request(&mut self, line: &str) -> Result<String> {
        self.stdin
            .write_all(line.as_bytes())
            .and_then(|_| self.stdin.write_all(b"\n"))
            .and_then(|_| self.stdin.flush())
            .map_err(|e| format!("helper write: {e}"))?;
        let mut out = String::new();
        let n = self
            .stdout
            .read_line(&mut out)
            .map_err(|e| format!("helper read: {e}"))?;
        if n == 0 {
            return err("AT-SPI helper exited");
        }
        Ok(out)
    }
    /// The whole screen as a semantic tree in the canonical vocabulary, un-normalized.
    pub fn tree(&mut self, width: u32, height: u32, max_nodes: usize) -> Result<(SemTree, Apps)> {
        let reply = self.request(&format!(
            r#"{{"cmd":"tree","max_nodes":{max_nodes},"max_depth":48}}"#
        ))?;
        let raw: RawTree = serde_json::from_str(&reply).map_err(|e| {
            format!(
                "helper reply: {e}: {}",
                reply.chars().take(200).collect::<String>()
            )
        })?;
        if let Some(e) = raw.error {
            return err(format!("AT-SPI: {e}"));
        }
        let mut tree = SemTree::desktop(width, height);
        let mut apps = Apps {
            truncated: raw.truncated,
            ms: raw.ms,
            list: Vec::new(),
        };
        let mut focus: Option<NodePath> = None;
        let mut active: Option<NodePath> = None;
        for app in raw.apps {
            apps.list.push((
                app.name.clone(),
                app.pid.clone(),
                app.toolkit.clone(),
                app.toolkit_version.clone(),
            ));
            for w in app.windows {
                let index = tree.root.children.len() as u32;
                let origin = w.bounds.map(|b| (b[0], b[1])).unwrap_or((0, 0));
                let mut node = convert(
                    &w,
                    &app.name,
                    &mut vec![index],
                    &mut focus,
                    &mut active,
                    origin,
                );
                node.attrs.insert("app".into(), app_kind(&app.name));
                if !app.toolkit.is_empty() {
                    node.attrs.insert("toolkit".into(), app.toolkit.clone());
                }
                tree.root.children.push(node);
            }
        }
        // Active window last, as the simulator stacks them.
        if let Some(a) = &active {
            if a.len() == 1 {
                let i = a[0] as usize;
                if i < tree.root.children.len() {
                    let w = tree.root.children.remove(i);
                    tree.root.children.push(w);
                    let last = tree.root.children.len() as u32 - 1;
                    active = Some(vec![last]);
                    if let Some(f) = &mut focus {
                        if f.first() == Some(&(i as u32)) {
                            f[0] = last;
                        } else if f.first().is_some_and(|&x| x > i as u32) {
                            f[0] -= 1;
                        }
                    }
                }
            }
        }
        tree.focus = focus;
        tree.active_window = active;
        Ok((tree, apps))
    }
}
impl Drop for AtSpi {
    fn drop(&mut self) {
        let _ = self.stdin.write_all(b"{\"cmd\":\"quit\"}\n");
        let _ = self.stdin.flush();
        std::thread::sleep(std::time::Duration::from_millis(50));
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// `kicad`, `libreoffice-calc`, … from the application's bus name.
pub fn app_kind(name: &str) -> String {
    let n = name.to_lowercase();
    let n = n.trim().trim_end_matches(".desktop");
    let n = n.rsplit('/').next().unwrap_or(n);
    let n = n
        .strip_prefix("org.")
        .map(|s| s.rsplit('.').next().unwrap_or(s))
        .unwrap_or(n);
    n.replace(' ', "-")
}

fn convert(
    raw: &RawNode,
    app: &str,
    path: &mut NodePath,
    focus: &mut Option<NodePath>,
    active: &mut Option<NodePath>,
    origin: (i32, i32),
) -> SemNode {
    let has = |s: &str| raw.states.iter().any(|x| x == s);
    let mut role = canonical_role(&raw.role).to_owned();
    // AT-SPI's `text` role is a text widget: editable means a text box, else static.
    if raw.role == "text" {
        role = if has("editable") {
            "textbox".into()
        } else {
            "text".into()
        };
    }
    if role == "button" && has("checkable") {
        role = "checkbox".into();
    }
    // A descendant answered at the screen's origin while its window coordinates say
    // otherwise has not been placed by the toolkit: place it from the window.
    let bounds = match (raw.bounds, raw.window_bounds) {
        (Some(b), Some(w))
            if path.len() > 1 && b[0] == 0 && b[1] == 0 && (w[0] != 0 || w[1] != 0) =>
        {
            Rect::new(origin.0 + w[0], origin.1 + w[1], w[2], w[3])
        }
        (Some(b), _) => Rect::new(b[0], b[1], b[2], b[3]),
        (None, Some(w)) => Rect::new(origin.0 + w[0], origin.1 + w[1], w[2], w[3]),
        (None, None) => Rect::default(),
    };
    let visible = has("visible") && has("showing") || (raw.states.is_empty() && !bounds.is_empty());
    let states = States {
        enabled: has("enabled") || has("sensitive") || raw.states.is_empty(),
        focusable: has("focusable"),
        focused: has("focused"),
        visible: visible || (role == "window" && !has("iconified")),
        checked: if has("checkable") || matches!(role.as_str(), "checkbox" | "radio" | "switch") {
            Some(has("checked"))
        } else {
            None
        },
        selected: if has("selectable") {
            Some(has("selected"))
        } else {
            None
        },
        expanded: if has("expandable") {
            Some(has("expanded"))
        } else {
            None
        },
    };
    if states.focused && focus.is_none() {
        *focus = Some(path.clone());
    }
    if cw_fidelity::is_window_role(&role) && has("active") && path.len() == 1 {
        *active = Some(path.clone());
    }
    let mut node = SemNode {
        role,
        name: raw.name.clone(),
        value: raw.value.clone(),
        states,
        bounds,
        source_id: None,
        attrs: BTreeMap::new(),
        children: Vec::new(),
    };
    if let Some(d) = &raw.description {
        node.attrs.insert("description".into(), d.clone());
    }
    for (k, v) in &raw.attrs {
        node.attrs.insert(k.clone(), v.clone());
    }
    if path.len() == 1 {
        node.attrs.insert("app".into(), app_kind(app));
    }
    for (i, c) in raw.children.iter().enumerate() {
        path.push(i as u32);
        node.children
            .push(convert(c, app, path, focus, active, origin));
        path.pop();
    }
    node
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn app_kinds() {
        assert_eq!(app_kind("kicad"), "kicad");
        assert_eq!(app_kind("org.gnome.gedit"), "gedit");
        assert_eq!(app_kind("soffice"), "soffice");
    }
}
