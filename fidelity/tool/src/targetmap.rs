//! Reference-side equivalents of the simulator's interaction ids. Procedures are written
//! against ComputerWorld's `window:<id>:content:kicad:…` ids; a real application has no
//! such ids, so a per-application map says what each means there: a key chord, typed
//! text, an ordered list of targets to try, a sequence of those, or that the gesture
//! does not apply. The map is conversion data a person (or Claude) maintains beside the
//! application. `_dismiss` names dialogs the tool closes whenever they appear.
use crate::util::{read_json, Result};
use cw_fidelity::text::glob_matches;
use cw_fidelity::Target;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default, rename = "type")]
    pub type_text: Option<String>,
    #[serde(default)]
    pub target: Option<Target>,
    #[serde(default)]
    pub targets: Vec<Target>,
    #[serde(default)]
    pub skip: bool,
    /// Steps delivered one after another, the screen re-read between them.
    #[serde(default)]
    pub sequence: Vec<Entry>,
}

/// A dialog the tool closes whenever it is on screen.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dismiss {
    /// Window title, `*` matching any run of characters.
    pub window: String,
    pub click: Target,
}

#[derive(Clone, Debug)]
pub enum Mapped {
    Key(String),
    Type(String),
    /// Targets to try in order; `{n}` in a label is the n-th `*` of the pattern's match.
    Targets(Vec<Target>),
    Sequence(Vec<Mapped>),
    Skip,
}

#[derive(Clone, Debug, Default)]
pub struct TargetMap {
    entries: Vec<(String, Entry)>,
    pub dismiss: Vec<Dismiss>,
}
impl TargetMap {
    pub fn load(paths: &[impl AsRef<Path>]) -> Result<Self> {
        let mut entries = Vec::new();
        let mut dismiss = Vec::new();
        for p in paths {
            let raw: BTreeMap<String, serde_json::Value> = read_json(p.as_ref())?;
            for (pattern, v) in raw {
                if pattern == "_dismiss" {
                    let list: Vec<Dismiss> = serde_json::from_value(v)
                        .map_err(|e| format!("{}: _dismiss: {e}", p.as_ref().display()))?;
                    dismiss.extend(list);
                    continue;
                }
                if pattern.starts_with('_') {
                    continue;
                }
                let e: Entry = serde_json::from_value(v)
                    .map_err(|e| format!("{}: {pattern}: {e}", p.as_ref().display()))?;
                entries.push((pattern, e));
            }
        }
        Ok(Self { entries, dismiss })
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.dismiss.is_empty()
    }
    /// The first entry whose pattern matches the interaction id.
    pub fn lookup(&self, interaction: &str) -> Option<Mapped> {
        let (pattern, e) = self
            .entries
            .iter()
            .find(|(p, _)| glob_matches(p, interaction))?;
        Some(resolve_entry(e, pattern, interaction))
    }
}

fn resolve_entry(e: &Entry, pattern: &str, interaction: &str) -> Mapped {
    if e.skip {
        return Mapped::Skip;
    }
    if !e.sequence.is_empty() {
        return Mapped::Sequence(
            e.sequence
                .iter()
                .map(|x| resolve_entry(x, pattern, interaction))
                .collect(),
        );
    }
    if let Some(k) = &e.key {
        return Mapped::Key(k.clone());
    }
    let stars = star_captures(pattern, interaction);
    if let Some(t) = &e.type_text {
        let mut out = t.clone();
        for (i, cap) in stars.iter().enumerate() {
            out = out.replace(&format!("{{{i}}}"), cap);
        }
        return Mapped::Type(out);
    }
    let fill = |s: &str| {
        let mut out = s.to_owned();
        for (i, cap) in stars.iter().enumerate() {
            out = out.replace(&format!("{{{i}}}"), cap);
        }
        out
    };
    let mut targets: Vec<Target> = e
        .target
        .iter()
        .cloned()
        .chain(e.targets.iter().cloned())
        .collect();
    for t in &mut targets {
        if let Some(l) = &t.label {
            t.label = Some(fill(l));
        }
        if let Some(x) = &t.text {
            t.text = Some(fill(x));
        }
    }
    Mapped::Targets(targets)
}

/// What each `*` of `pattern` matched in `text`, leftmost-shortest, for `{n}` fills.
pub fn star_captures(pattern: &str, text: &str) -> Vec<String> {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() < 2 {
        return Vec::new();
    }
    let mut caps = Vec::new();
    let mut rest = text;
    if !rest.starts_with(parts[0]) {
        return Vec::new();
    }
    rest = &rest[parts[0].len()..];
    for (i, lit) in parts[1..].iter().enumerate() {
        let last = i + 2 == parts.len();
        if lit.is_empty() {
            if last {
                caps.push(rest.to_owned());
                rest = "";
            } else {
                caps.push(String::new());
            }
            continue;
        }
        let pos = if last {
            rest.rfind(lit)
        } else {
            rest.find(lit)
        };
        let Some(pos) = pos else { return Vec::new() };
        caps.push(rest[..pos].to_owned());
        rest = &rest[pos + lit.len()..];
    }
    caps
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn captures() {
        assert_eq!(
            star_captures(
                "*kicad:dlg:pick:*:*",
                "window:3:content:kicad:dlg:pick:Device:R"
            ),
            vec!["window:3:content:", "Device", "R"]
        );
        assert_eq!(
            star_captures("*kicad:pm:new", "window:3:content:kicad:pm:new"),
            vec!["window:3:content:"]
        );
    }
}
