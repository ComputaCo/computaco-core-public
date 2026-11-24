//! What an application declares so that rules can be learned for it with no code written for it anywhere
//! else (the engine's design notes § 4 and § Stage 10).
//!
//! An application says three things, as data, beside its own code:
//!
//! - **its commands' prefix** (`cal` for `cal:new`), which tells its controls from the shell's;
//! - **its command table** ([`Command`]): for a command or a family of commands (`contacts:person:*`), the
//!   role of its control, the piece of state the control stands for (`*` is that object's id), and the
//!   region it is drawn in. [`annotate`] writes these onto the nodes the application painted, so the
//!   scene carries the same structure as one drawn with `Painter::begin_region` and `Painter::tag`;
//! - **the structure of its state** ([`Field`]): which parts of its state decide what its commands do
//!   (a tab, whether a record is open, whether a field has text) as against the data they act on.
//!   [`abstract_state`] reads them from the state as it is serialized.
//!
//! Mail, Messages and Docs say the same things in code (regions and tags while they draw, an
//! `abstract_state` of their own); what is here is the same declaration for an application that has not
//! been touched.
use cw_scene::{Landmark, Node, ObjRef};
use serde_json::Value;

/// How a part of the state is read as structure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum As {
    /// Its value as it is: a tab, a mode, a flag.
    Value,
    /// Whether it is there (not `null`): something is open or chosen.
    Present,
    /// Whether it holds anything: a text that is not blank, a list or a map with an entry, a number
    /// that is not zero.
    Filled,
    /// Which variant of an enum it is, without what the variant carries: a unit variant is serialized
    /// as its name, one with data as an object of one key (`{"offline": "…"}` is `offline`).
    Variant,
}
/// One part of an application's state that its commands branch on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Field {
    /// Its name in the abstract state.
    pub name: &'static str,
    /// Where it is in the serialized state (a JSON pointer).
    pub pointer: &'static str,
    pub read: As,
}
/// A part of the state at `pointer`, named `name`.
pub const fn field(name: &'static str, pointer: &'static str, read: As) -> Field {
    Field {
        name,
        pointer,
        read,
    }
}
/// One command of an application, or every command that starts alike.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Command {
    /// The command as its control is drawn (`contacts:reload`), or its start and `*` where the id of the
    /// object it acts on stands (`contacts:person:*`).
    pub pattern: &'static str,
    /// The control's role, when it is not a button: `listitem`, `textbox`, `tab`.
    pub role: Option<&'static str>,
    /// The kind of thing the control stands for: a record (`contact`), whose id the `*` gives, or a
    /// field of one (`event.title`).
    pub object: Option<&'static str>,
    /// The region it is drawn in, outermost first, each `(role, name)`.
    pub region: &'static [(&'static str, &'static str)],
}
/// A command drawn as a button in no declared region.
pub const fn command(pattern: &'static str) -> Command {
    Command {
        pattern,
        role: None,
        object: None,
        region: &[],
    }
}
impl Command {
    pub const fn role(mut self, role: &'static str) -> Self {
        self.role = Some(role);
        self
    }
    pub const fn object(mut self, kind: &'static str) -> Self {
        self.object = Some(kind);
        self
    }
    pub const fn region(mut self, path: &'static [(&'static str, &'static str)]) -> Self {
        self.region = path;
        self
    }
    /// The id a command of this pattern carries, when it is one: `Some("")` for an exact match.
    fn id_of<'a>(&self, command: &'a str) -> Option<&'a str> {
        match self.pattern.strip_suffix('*') {
            Some(start) => command.strip_prefix(start).filter(|id| !id.is_empty()),
            None => (self.pattern == command).then_some(""),
        }
    }
}
/// Everything one application declares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Declaration {
    /// What its commands start with, without the colon.
    pub prefix: &'static str,
    pub structure: &'static [Field],
    pub commands: &'static [Command],
}

/// The abstract state `d` declares, read from the application's state as serialized.
pub fn abstract_state(d: &Declaration, state: &Value) -> Value {
    let mut out = serde_json::Map::new();
    for f in d.structure {
        let v = state.pointer(f.pointer).unwrap_or(&Value::Null);
        let read = match f.read {
            As::Value => v.clone(),
            As::Present => Value::Bool(!v.is_null()),
            As::Variant => match v {
                Value::Object(o) if o.len() == 1 => {
                    Value::String(o.keys().next().cloned().unwrap_or_default())
                }
                other => other.clone(),
            },
            As::Filled => Value::Bool(match v {
                Value::Null => false,
                Value::Bool(b) => *b,
                Value::Number(n) => n.as_f64() != Some(0.0),
                Value::String(s) => !s.trim().is_empty(),
                Value::Array(a) => !a.is_empty(),
                Value::Object(o) => !o.is_empty(),
            }),
        };
        out.insert(f.name.to_owned(), read);
    }
    Value::Object(out)
}

/// Write `d`'s command table onto the nodes an application just painted: each control's role, the
/// object it stands for and its region, and for a text drawn on a control that stands for a record,
/// that record's item region (so a row's words are read as the row's). A node the application gave a
/// region or an object itself keeps it.
pub fn annotate(d: &Declaration, nodes: &mut [Node]) {
    // The controls the table names: (node, region, object).
    let mut bound: Vec<(usize, Vec<Landmark>, Option<ObjRef>)> = vec![];
    for (i, n) in nodes.iter_mut().enumerate() {
        let Some(command) = n.interaction.as_deref() else {
            continue;
        };
        let Some((c, id)) = d
            .commands
            .iter()
            .find_map(|c| c.id_of(command).map(|id| (c, id.to_owned())))
        else {
            continue;
        };
        if let (Some(role), Some(s)) = (c.role, n.semantic.as_mut()) {
            s.role = role.into();
        }
        let object = c.object.map(|kind| ObjRef::new(kind, id.as_str()));
        let mut region: Vec<Landmark> = c
            .region
            .iter()
            .map(|(role, name)| Landmark {
                role: (*role).into(),
                name: (*name).into(),
                object: None,
            })
            .collect();
        // A control that is one record of many is that record's item, as a row drawn by hand is.
        if let Some(o) = object.as_ref().filter(|o| !o.id.is_empty()) {
            region.push(Landmark {
                role: "item".into(),
                name: o.kind.clone(),
                object: Some(o.clone()),
            });
        }
        if n.object.is_none() {
            n.object.clone_from(&object);
        }
        if n.region.is_none() && !region.is_empty() {
            n.region = Some(region.clone().into());
        }
        bound.push((i, region, object));
    }
    // Words drawn on a record's control are the record's.
    for i in 0..nodes.len() {
        let n = &nodes[i];
        if n.interaction.is_some() || n.region.is_some() || n.painted_text().is_none() {
            continue;
        }
        let mine = n.painted_bounds();
        let inside = bound
            .iter()
            .filter(|(_, region, object)| {
                !region.is_empty() && object.as_ref().is_some_and(|o| !o.id.is_empty())
            })
            .filter(|(j, _, _)| {
                let b = nodes[*j].painted_bounds();
                b.x <= mine.x
                    && b.y <= mine.y
                    && mine.x + mine.width as i32 <= b.x + b.width as i32
                    && mine.y + mine.height as i32 <= b.y + b.height as i32
            })
            .min_by_key(|(j, _, _)| {
                let b = nodes[*j].painted_bounds();
                (u64::from(b.width) * u64::from(b.height), *j)
            })
            .map(|(_, region, _)| region.clone());
        if let Some(region) = inside {
            nodes[i].region = Some(region.into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const D: Declaration = Declaration {
        prefix: "people",
        structure: &[
            field("tab", "/tab", As::Value),
            field("open", "/selected", As::Present),
            field("rows", "/people", As::Filled),
            field("query", "/query", As::Filled),
            field("status", "/status", As::Variant),
        ],
        commands: &[
            command("people:open:*")
                .role("listitem")
                .object("person")
                .region(&[("list", "people")]),
            command("people:query")
                .role("textbox")
                .object("search.query"),
        ],
    };

    #[test]
    fn structure_is_read_without_the_data() {
        let a = json!({"tab": "all", "selected": null, "people": {"bob": {}}, "query": "  ", "status": {"offline": "down"}});
        let b = json!({"tab": "all", "selected": null, "people": {"carol": {}, "dan": {}}, "query": "", "status": {"offline": "gone"}});
        assert_eq!(abstract_state(&D, &a), abstract_state(&D, &b));
        assert_eq!(
            abstract_state(&D, &a),
            json!({"tab": "all", "open": false, "rows": true, "query": false, "status": "offline"})
        );
    }

    #[test]
    fn a_pattern_gives_the_object_its_id() {
        let c = &D.commands[0];
        assert_eq!(c.id_of("people:open:bob"), Some("bob"));
        assert_eq!(c.id_of("people:open:"), None);
        assert_eq!(c.id_of("people:query"), None);
        assert_eq!(D.commands[1].id_of("people:query"), Some(""));
    }
}
