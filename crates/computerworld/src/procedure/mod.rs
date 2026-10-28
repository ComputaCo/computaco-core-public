//! Expert procedures, projected onto the graphical interface.
//!
//! A [`Procedure`] is how a practitioner does one piece of work — open the matter,
//! fill in the letter, save it — written as phases of gestures aimed at what the screen
//! shows (a button's label, a painted line of text) rather than at pixels. Each phase
//! may also carry the tool calls that reach the same result without a screen.
//!
//! [`World::project`] performs a procedure the way a person at the machine would:
//! every gesture is resolved against the live scene to a point a click really reaches,
//! sent as `pointer.v1` or `keyboard.v1`, and recorded. The [`Demonstration`] it
//! returns is a list of ordinary actions, so it replays to the same state hash on every
//! platform, and its outcome is judged by the procedure's own [`Check`]s against owner
//! state the actor never sees. Parameters sampled from a seed make one procedure into
//! as many distinct, verifiable episodes as there are seeds (docs/expert-procedures.md).
use crate::{ActionEnvelope, Result, SimError, World};
use cw_evaluation::Predicate;
use cw_scene::{Node, Scene};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

mod expr;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Procedure {
    pub id: String,
    /// The profession the procedure comes from: `legal`, `accounting`, …
    #[serde(default)]
    pub domain: String,
    /// What a person would be asked to do. Given to the agent; the phases are not.
    pub instruction: String,
    /// The machine a one-person procedure runs on, and the default for setup files and
    /// checks that do not name one.
    #[serde(default)]
    pub machine: String,
    /// The people in a team procedure, by role: each acts on a machine of their own,
    /// through a session of their own.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub roles: BTreeMap<String, Role>,
    /// Sampled in order, so a later parameter may be computed from earlier ones.
    #[serde(default)]
    pub params: Vec<Param>,
    /// Files the owner writes before the episode: the case file, the ledger.
    #[serde(default)]
    pub setup: Vec<SeedFile>,
    pub phases: Vec<Phase>,
    pub verify: Vec<Check>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Role {
    pub machine: String,
    /// The account the role works as on that machine.
    pub user: String,
    /// What the person does on the team: "Staff engineer", "Reviewer".
    #[serde(default)]
    pub title: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
    #[serde(flatten)]
    pub spec: ParamSpec,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ParamSpec {
    Choice {
        choices: Vec<String>,
    },
    /// Inclusive on both ends.
    Integer {
        min: i64,
        max: i64,
    },
    /// The sum of earlier integer parameters.
    Sum {
        of: Vec<String>,
    },
    /// Earlier parameters substituted into `{{name}}` placeholders.
    Format {
        template: String,
    },
    /// Arithmetic over earlier parameters, exact to twelve decimal places
    /// (`round(principal * r / (1 - pow(1 + r, -n)), 2)`), written with `decimals`
    /// places (all significant ones when absent) and, with `grouping`, thousands commas.
    Expr {
        expr: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        decimals: Option<u32>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        grouping: bool,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeedFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine: Option<String>,
    pub path: String,
    pub content: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Phase {
    /// The step as the expert would name it: "Record the retainer in the letter".
    pub label: String,
    /// Who does it, in a team procedure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    pub gui: Vec<Gesture>,
    /// The same step as tool calls. A phase that only moves around the screen (opening
    /// a folder) has none.
    #[serde(default)]
    pub tool: Vec<ToolCall>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCall {
    pub family: String,
    pub op: String,
    #[serde(default)]
    pub payload: Value,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "do", rename_all = "snake_case", deny_unknown_fields)]
pub enum Gesture {
    Click {
        target: Target,
        /// Where inside the target, in thousandths of its width and height from its
        /// top-left corner: a point on a canvas, a cell in a grid. The centre (or the
        /// first point a click reaches) when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<[u32; 2]>,
    },
    DoubleClick {
        target: Target,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<[u32; 2]>,
    },
    /// Press on one point, move to another, release: a slider, a selection, a stroke.
    Drag {
        from: Target,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from_at: Option<[u32; 2]>,
        to: Target,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to_at: Option<[u32; 2]>,
    },
    /// Turn the wheel over a target; positive `delta_y` moves the content up.
    Scroll {
        target: Target,
        delta_y: i64,
    },
    /// Click where a point of the part in FreeCAD's 3D view appears on screen, through
    /// the camera the view was drawn with: a sketch vertex, a point on an edge.
    ClickModel {
        point: [f64; 3],
    },
    /// A newline is pressed as `Enter`, so the text means the same in every app.
    /// `verbatim` sends the whole text, newlines included, as one input, the way a paste
    /// arrives: code editors auto-indent after an `Enter` but not inside a paste.
    Type {
        text: String,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        verbatim: bool,
    },
    Key {
        key: String,
    },
}
/// What a gesture is aimed at, as a person would find it on screen. Every field given
/// must match; the topmost node that matches and that a click can reach wins.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    /// Accessible role: `button`, `textbox`, …
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Accessible label, exactly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Painted text containing this; the node need not be interactive itself, only
    /// lie over something that is (a line of a document over its text area).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Interaction id, with `*` matching any run of characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interaction: Option<String>,
    /// Application kind of the owning window: `files`, `editor`, `spreadsheet`, …
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Check {
    FileEquals {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        machine: Option<String>,
        path: String,
        content: String,
    },
    FileContains {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        machine: Option<String>,
        path: String,
        text: String,
    },
    /// Some line of the file matches `line`, whichever terminator it ends with:
    /// spreadsheets write CSV with `\r\n` (RFC 4180), editors with `\n`. `*` matches
    /// any run of characters, for a column the application is free to reformat.
    FileHasLine {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        machine: Option<String>,
        path: String,
        line: String,
    },
    FileLacks {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        machine: Option<String>,
        path: String,
        text: String,
    },
    /// The file parses as JSON and the value at `pointer` equals `value`.
    FileJson {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        machine: Option<String>,
        path: String,
        pointer: String,
        value: Value,
    },
    /// The file is a FreeCAD document whose feature tree has the reference's object
    /// types in the reference's order, and whose every body recomputes without error to
    /// the reference body's volume within `tolerance` (relative).
    CadMatches {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        machine: Option<String>,
        path: String,
        reference: Value,
        tolerance: f64,
    },
    /// A file as a git service holds it at a ref: `ref` resolved to its commit, and the
    /// commit's copy of `path` equal to `content` (absent when `content` is null).
    GitFile {
        service: String,
        repo: String,
        #[serde(rename = "ref")]
        git_ref: String,
        path: String,
        content: Option<String>,
    },
    /// A predicate over `World::inspect()`.
    State(Predicate),
}

/// A procedure with its parameters drawn and substituted: one concrete episode.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Instance {
    pub seed: u64,
    pub values: BTreeMap<String, String>,
    pub procedure: Procedure,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Pointer and keyboard on the rendered desktop, every target found on screen.
    #[default]
    Gui,
    /// The phases' tool calls, with no screen at all.
    Tool,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Typing {
    /// One `keyboard.v1 type` per run of text, as computer-use agents send it.
    #[default]
    Text,
    /// One action per character, as a person presses keys.
    Keys,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ProjectOptions {
    pub width: u32,
    pub height: u32,
    pub mode: Mode,
    pub typing: Typing,
}
impl Default for ProjectOptions {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 800,
            mode: Mode::Gui,
            typing: Typing::Text,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Demonstration {
    pub procedure: String,
    pub instruction: String,
    pub seed: u64,
    pub options: ProjectOptions,
    pub steps: Vec<Step>,
    /// Every gesture found its target and every action succeeded.
    pub completed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<String>,
    pub verification: Verification,
    pub state_hash: String,
    pub counts: Counts,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub phase: usize,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// The session the action was sent through: whose hands it was.
    pub session: String,
    pub action: ActionEnvelope,
    /// What a pointer action was aimed at, and where it landed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<Resolved>,
    /// Digest of the scene the action was chosen from; 0 in tool mode.
    pub scene: u64,
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<SimError>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resolved {
    pub node: u64,
    /// Application kind of the window the click landed in; absent on the desktop shell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    pub role: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interaction: Option<String>,
    pub x: i32,
    pub y: i32,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counts {
    pub phases: usize,
    pub actions: usize,
    pub pointer_actions: usize,
    pub keyboard_actions: usize,
    pub tool_actions: usize,
    pub typed_characters: usize,
    /// Scenes the projector looked at to choose an action: one per GUI action.
    pub observations: usize,
    /// Serialized JSON of every action sent, summed.
    pub action_bytes: usize,
    /// Actions per role, in a team procedure.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub roles: BTreeMap<String, usize>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Verification {
    pub passed: bool,
    pub checks: Vec<CheckResult>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CheckResult {
    pub check: Check,
    pub passed: bool,
}

fn invalid(message: impl Into<String>) -> SimError {
    SimError::new("procedure", message)
}

/// SplitMix64: enough to spread a seed over a procedure's choices, and the same in
/// every binding because it is this code.
struct Draw(u64);
impl Draw {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}
fn fnv(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Replace every `{{name}}` in `text`. A placeholder with no value is an error, so a
/// misspelt parameter cannot quietly put braces into a legal letter.
fn substitute(text: &str, values: &BTreeMap<String, String>) -> Result<String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let end = after
            .find("}}")
            .ok_or_else(|| invalid(format!("unclosed placeholder in {text:?}")))?;
        let name = after[..end].trim();
        let value = values
            .get(name)
            .ok_or_else(|| invalid(format!("no parameter named {name:?}")))?;
        out.push_str(value);
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    Ok(out)
}
fn substitute_all(value: &mut Value, values: &BTreeMap<String, String>) -> Result<()> {
    match value {
        Value::String(s) => *s = substitute(s, values)?,
        Value::Array(items) => {
            for item in items {
                substitute_all(item, values)?;
            }
        }
        Value::Object(map) => {
            for item in map.values_mut() {
                substitute_all(item, values)?;
            }
        }
        _ => {}
    }
    Ok(())
}

impl Procedure {
    pub fn from_json(text: &str) -> Result<Self> {
        serde_json::from_str(text).map_err(|e| invalid(e.to_string()))
    }
    /// Draw this procedure's parameters from `seed`, let `overrides` replace any of
    /// them, and substitute the values everywhere else.
    pub fn instantiate(&self, seed: u64, overrides: &BTreeMap<String, String>) -> Result<Instance> {
        let mut draw = Draw(seed ^ fnv(&self.id));
        let mut values = BTreeMap::new();
        for param in &self.params {
            // Always draw, so overriding one parameter leaves the others unchanged.
            let roll = draw.next();
            let value = match &param.spec {
                ParamSpec::Choice { choices } if choices.is_empty() => {
                    return Err(invalid(format!("{} has no choices", param.name)));
                }
                ParamSpec::Choice { choices } => {
                    choices[(roll % choices.len() as u64) as usize].clone()
                }
                ParamSpec::Integer { min, max } if max < min => {
                    return Err(invalid(format!("{} has max below min", param.name)));
                }
                ParamSpec::Integer { min, max } => {
                    let span = (max - min) as u64 + 1;
                    (min + (roll % span) as i64).to_string()
                }
                ParamSpec::Sum { of } => {
                    let mut total: i64 = 0;
                    for name in of {
                        let part = values
                            .get(name)
                            .and_then(|v: &String| v.parse::<i64>().ok());
                        total += part.ok_or_else(|| {
                            invalid(format!(
                                "{} sums {name}, which is not an earlier integer",
                                param.name
                            ))
                        })?;
                    }
                    total.to_string()
                }
                ParamSpec::Format { template } => substitute(template, &values)?,
                ParamSpec::Expr {
                    expr,
                    decimals,
                    grouping,
                } => expr::evaluate(expr, &values)
                    .map_err(|e| invalid(format!("{}: {e}", param.name)))?
                    .format(*decimals, *grouping),
            };
            let value = overrides.get(&param.name).cloned().unwrap_or(value);
            values.insert(param.name.clone(), value);
        }
        if let Some(name) = overrides.keys().find(|k| !values.contains_key(*k)) {
            return Err(invalid(format!("no parameter named {name:?} to override")));
        }
        let mut body = serde_json::to_value(self).map_err(|e| invalid(e.to_string()))?;
        if let Value::Object(map) = &mut body {
            map.insert("params".into(), json!([]));
        }
        substitute_all(&mut body, &values)?;
        let procedure = serde_json::from_value(body).map_err(|e| invalid(e.to_string()))?;
        Ok(Instance {
            seed,
            values,
            procedure,
        })
    }
}

fn glob(pattern: &str, text: &str) -> bool {
    let mut parts = pattern.split('*');
    let first = parts.next().unwrap_or("");
    let Some(mut rest) = text.strip_prefix(first) else {
        return false;
    };
    let parts: Vec<&str> = parts.collect();
    let Some((last, middle)) = parts.split_last() else {
        return rest.is_empty();
    };
    for part in middle {
        match rest.find(part) {
            Some(i) => rest = &rest[i + part.len()..],
            None => return false,
        }
    }
    rest.len() >= last.len() && rest.ends_with(last)
}
impl Target {
    fn matches(&self, scene: &Scene, node: &Node) -> bool {
        let semantic = node.semantic.as_ref();
        let role = semantic.map(|s| s.role.as_str());
        let label = semantic.map(|s| s.label.as_str());
        self.role.as_deref().is_none_or(|r| role == Some(r))
            && self.label.as_deref().is_none_or(|l| label == Some(l))
            && self
                .text
                .as_deref()
                .is_none_or(|t| node.painted_text().is_some_and(|p| p.contains(t)))
            && self
                .interaction
                .as_deref()
                .is_none_or(|p| node.interaction.as_deref().is_some_and(|i| glob(p, i)))
            && self.app.as_deref().is_none_or(|app| {
                node.window
                    .and_then(|w| scene.window(w))
                    .is_some_and(|w| w.app == app)
            })
    }
    /// The topmost matching node a click reaches, and the point it reaches it at.
    /// With `at`, the point is that fraction of the node's painted bounds, and it must
    /// land on the node (or, for painted text, on its window).
    fn resolve_at(&self, scene: &Scene, at: Option<[u32; 2]>) -> Option<Resolved> {
        let Some([fx, fy]) = at else {
            return self.resolve(scene);
        };
        let mut nodes: Vec<(usize, &Node)> = scene
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| self.matches(scene, n))
            .collect();
        nodes.sort_by_key(|(i, n)| std::cmp::Reverse((n.z, *i)));
        nodes.into_iter().find_map(|(_, node)| {
            let b = node.painted_bounds();
            if b.area() == 0 {
                return None;
            }
            let x = b.x + (i64::from(b.width) * i64::from(fx.min(999)) / 1000) as i32;
            let y = b.y + (i64::from(b.height) * i64::from(fy.min(999)) / 1000) as i32;
            let hit = scene.hit_test(x, y)?;
            let lands = if node.accepts_input() {
                hit.id == node.id
                    || hit.interaction.is_some() && hit.interaction == node.interaction
            } else {
                node.window.is_none() || hit.window == node.window
            };
            lands.then(|| resolved(scene, node, hit, x, y))
        })
    }
    /// The topmost matching node a click reaches, and the point it reaches it at.
    fn resolve(&self, scene: &Scene) -> Option<Resolved> {
        let mut nodes: Vec<(usize, &Node)> = scene
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| self.matches(scene, n))
            .collect();
        nodes.sort_by_key(|(i, n)| std::cmp::Reverse((n.z, *i)));
        nodes.into_iter().find_map(|(_, node)| {
            let (x, y) = if node.accepts_input() {
                scene.reachable_point(node.id)?
            } else {
                // Painted text: any point over it that lands on its own window.
                covered_point(scene, node)?
            };
            let hit = scene.hit_test(x, y)?;
            Some(resolved(scene, node, hit, x, y))
        })
    }
}
fn resolved(scene: &Scene, node: &Node, hit: &Node, x: i32, y: i32) -> Resolved {
    let semantic = node.semantic.clone().unwrap_or_default();
    Resolved {
        node: node.id,
        app: hit
            .window
            .and_then(|w| scene.window(w))
            .map(|w| w.app.clone()),
        role: semantic.role,
        label: if semantic.label.is_empty() {
            node.painted_text().unwrap_or_default().to_owned()
        } else {
            semantic.label
        },
        interaction: hit.interaction.clone(),
        x,
        y,
    }
}

#[cfg(feature = "freecad")]
fn cad_matches(saved: &str, reference: &Value, tolerance: f64) -> bool {
    use cw_cad::{document, io};
    let Ok(mut saved) = io::load_native(saved) else {
        return false;
    };
    let Ok(mut reference) = io::load_native(&reference.to_string()) else {
        return false;
    };
    let kinds = |d: &document::Document| -> Vec<String> {
        d.objects
            .iter()
            .map(|o| {
                serde_json::to_value(o)
                    .ok()
                    .and_then(|v| v.get("TypeId").and_then(Value::as_str).map(str::to_owned))
                    .unwrap_or_default()
            })
            .collect()
    };
    if kinds(&saved) != kinds(&reference) {
        return false;
    }
    let (got, want) = (
        document::recompute(&mut saved),
        document::recompute(&mut reference),
    );
    let bodies: Vec<String> = reference.bodies().iter().map(|b| (*b).to_owned()).collect();
    !bodies.is_empty()
        && bodies
            .iter()
            .zip(saved.bodies())
            .all(|(want_body, got_body)| {
                let (Some(w), Some(g)) =
                    (want.body_shape.get(want_body), got.body_shape.get(got_body))
                else {
                    return false;
                };
                let (w, g) = (w.volume(), g.volume());
                w > 0.0 && ((g - w) / w).abs() <= tolerance
            })
}
/// A build without the CAD engine cannot judge a part, so it never passes one.
#[cfg(not(feature = "freecad"))]
fn cad_matches(_saved: &str, _reference: &Value, _tolerance: f64) -> bool {
    false
}
/// Like `Scene::reachable_point`, for a node that is not interactive itself: the
/// centre first, then a fixed grid, taking the first point whose topmost interactive
/// node belongs to the same window.
fn covered_point(scene: &Scene, node: &Node) -> Option<(i32, i32)> {
    let b = node.painted_bounds();
    if b.area() == 0 {
        return None;
    }
    let sample = |i: u32, d: u32, origin: i32, extent: u32| {
        origin + ((u64::from(extent) * u64::from(2 * i + 1)) / u64::from(2 * d)) as i32
    };
    std::iter::once((b.x + b.width as i32 / 2, b.y + b.height as i32 / 2))
        .chain((0..7).flat_map(move |j| {
            (0..7).map(move |i| (sample(i, 7, b.x, b.width), sample(j, 7, b.y, b.height)))
        }))
        .find(|&(x, y)| {
            scene
                .hit_test(x, y)
                .is_some_and(|hit| node.window.is_none() || hit.window == node.window)
        })
}
fn describe(target: &Target) -> String {
    serde_json::to_string(target).unwrap_or_default()
}

impl Procedure {
    /// The machine `role` acts on: the role's own, or the procedure's for a phase that
    /// names no role.
    pub fn machine_of(&self, role: Option<&str>) -> Result<&str> {
        let machine = match role {
            Some(role) => {
                &self
                    .roles
                    .get(role)
                    .ok_or_else(|| invalid(format!("no role named {role:?}")))?
                    .machine
            }
            None => &self.machine,
        };
        if machine.is_empty() {
            return Err(invalid(
                "a phase names no role and the procedure no machine",
            ));
        }
        Ok(machine)
    }
}
impl Check {
    fn file(&self) -> Option<(Option<&str>, &str)> {
        match self {
            Self::FileEquals { machine, path, .. }
            | Self::FileContains { machine, path, .. }
            | Self::FileHasLine { machine, path, .. }
            | Self::FileLacks { machine, path, .. }
            | Self::FileJson { machine, path, .. }
            | Self::CadMatches { machine, path, .. } => Some((machine.as_deref(), path)),
            Self::State(_) | Self::GitFile { .. } => None,
        }
    }
}

impl World {
    /// Write the instance's setup files as each machine's own user.
    pub fn stage(&mut self, instance: &Instance) -> Result<()> {
        let procedure = &instance.procedure;
        for file in &procedure.setup {
            let machine = file.machine.as_deref().unwrap_or(&procedure.machine);
            if let Some((parent, _)) = file.path.rsplit_once('/') {
                if !parent.is_empty() {
                    self.runtime_mut()
                        .create_directory(machine, "owner", parent)?;
                }
            }
            self.runtime_mut()
                .write_file(machine, "owner", &file.path, file.content.as_bytes())?;
        }
        Ok(())
    }
    /// Judge the instance's checks against the world as it is now.
    pub fn verify(&self, instance: &Instance) -> Verification {
        let mut inspection = None;
        let read = |machine: Option<&str>, path: &str| {
            self.runtime()
                .read_file(machine.unwrap_or(&instance.procedure.machine), path)
                .ok()
                .map(|b| String::from_utf8_lossy(&b).into_owned())
        };
        let checks: Vec<CheckResult> = instance
            .procedure
            .verify
            .iter()
            .map(|check| {
                let file = check.file().and_then(|(machine, path)| read(machine, path));
                let passed = match check {
                    Check::FileEquals { content, .. } => file.as_ref() == Some(content),
                    Check::FileContains { text, .. } => {
                        file.is_some_and(|c| c.contains(text.as_str()))
                    }
                    Check::FileHasLine { line, .. } => file.is_some_and(|c| {
                        c.lines()
                            .any(|l| glob(line, l.strip_suffix('\r').unwrap_or(l)))
                    }),
                    Check::FileLacks { text, .. } => {
                        file.is_some_and(|c| !c.contains(text.as_str()))
                    }
                    Check::FileJson { pointer, value, .. } => file.is_some_and(|c| {
                        serde_json::from_str::<Value>(&c)
                            .is_ok_and(|v| v.pointer(pointer) == Some(value))
                    }),
                    Check::CadMatches {
                        reference,
                        tolerance,
                        ..
                    } => file.is_some_and(|c| cad_matches(&c, reference, *tolerance)),
                    Check::GitFile {
                        service,
                        repo,
                        git_ref,
                        path,
                        content,
                    } => {
                        let state = inspection.get_or_insert_with(|| self.inspect());
                        let repo = state.pointer(&format!(
                            "/state/services/{service}/state/repositories/{repo}"
                        ));
                        let commit = repo
                            .and_then(|r| r.get("refs")?.get(git_ref.as_str())?.as_str())
                            .and_then(|hash| repo?.get("objects")?.get(hash));
                        let file =
                            commit.map(|c| c.get("files").and_then(|f| f.get(path.as_str())));
                        match (file, content) {
                            (Some(Some(Value::String(have))), Some(want)) => have == want,
                            (Some(None), None) => true,
                            _ => false,
                        }
                    }
                    Check::State(predicate) => {
                        predicate.evaluate(inspection.get_or_insert_with(|| self.inspect()))
                    }
                };
                CheckResult {
                    check: check.clone(),
                    passed,
                }
            })
            .collect();
        Verification {
            passed: checks.iter().all(|c| c.passed),
            checks,
        }
    }
    /// Perform a one-person instance through `session` and record it. The world is
    /// left in the state the demonstration ends in; fork first to keep the start.
    pub fn project(
        &mut self,
        session: &str,
        instance: &Instance,
        options: ProjectOptions,
    ) -> Result<Demonstration> {
        let mut sessions: BTreeMap<String, String> = instance
            .procedure
            .roles
            .keys()
            .map(|role| (role.clone(), session.to_owned()))
            .collect();
        sessions.insert(String::new(), session.to_owned());
        self.project_team(&sessions, instance, options)
    }
    /// Perform a team instance, each role's phases through that role's session in
    /// `sessions`, in the order the procedure gives them. A phase with no role uses the
    /// session under the empty name.
    pub fn project_team(
        &mut self,
        sessions: &BTreeMap<String, String>,
        instance: &Instance,
        options: ProjectOptions,
    ) -> Result<Demonstration> {
        let procedure = &instance.procedure;
        let mut assigned = Vec::with_capacity(procedure.phases.len());
        for phase in &procedure.phases {
            let role = phase.role.as_deref().unwrap_or("");
            let session = sessions.get(role).ok_or_else(|| {
                invalid(format!("no session for role {role:?} of {}", procedure.id))
            })?;
            self.validate_session(session)?;
            let machine = procedure.machine_of(phase.role.as_deref())?.to_owned();
            assigned.push((session.clone(), machine));
        }
        let mut run = Run {
            world: self,
            session: String::new(),
            machine: String::new(),
            role: None,
            options,
            steps: Vec::new(),
            counts: Counts {
                phases: procedure.phases.len(),
                ..Counts::default()
            },
        };
        let mut failure = None;
        'phases: for (index, (phase, (session, machine))) in
            procedure.phases.iter().zip(assigned).enumerate()
        {
            run.session = session;
            run.machine = machine;
            run.role = phase.role.clone();
            let outcome = match options.mode {
                Mode::Gui => phase
                    .gui
                    .iter()
                    .try_for_each(|gesture| run.gesture(index, &phase.label, gesture)),
                Mode::Tool => phase.tool.iter().try_for_each(|call| {
                    let action = ActionEnvelope::new(
                        call.family.clone(),
                        call.op.clone(),
                        run.machine.clone(),
                        call.payload.clone(),
                    );
                    run.counts.tool_actions += 1;
                    run.send(index, &phase.label, action, None, 0)
                }),
            };
            if let Err(message) = outcome {
                failure = Some(format!("phase {index} ({}): {message}", phase.label));
                break 'phases;
            }
        }
        let Run { steps, counts, .. } = run;
        Ok(Demonstration {
            procedure: procedure.id.clone(),
            instruction: procedure.instruction.clone(),
            seed: instance.seed,
            options,
            completed: failure.is_none(),
            failure,
            verification: self.verify(instance),
            state_hash: self.state_hash()?,
            steps,
            counts,
        })
    }
}

struct Run<'a> {
    world: &'a mut World,
    session: String,
    machine: String,
    role: Option<String>,
    options: ProjectOptions,
    steps: Vec<Step>,
    counts: Counts,
}
impl Run<'_> {
    fn scene(&self) -> std::result::Result<Scene, String> {
        let mut scene = self
            .world
            .scene(&self.session, self.options.width, self.options.height)
            .map_err(|e| e.to_string())?;
        if scene.digest == 0 {
            scene.stamp();
        }
        Ok(scene)
    }
    fn send(
        &mut self,
        phase: usize,
        label: &str,
        action: ActionEnvelope,
        target: Option<Resolved>,
        scene: u64,
    ) -> std::result::Result<(), String> {
        self.counts.actions += 1;
        self.counts.action_bytes += serde_json::to_string(&action).map_or(0, |s| s.len());
        if let Some(role) = &self.role {
            *self.counts.roles.entry(role.clone()).or_default() += 1;
        }
        let result = self.world.step(&self.session, vec![action.clone()]);
        let (success, error) = match result {
            Ok(mut r) if !r.outcomes.is_empty() => {
                let o = r.outcomes.swap_remove(0);
                (o.success, o.error)
            }
            Ok(_) => (false, Some(invalid("the step returned no outcome"))),
            Err(e) => (false, Some(e)),
        };
        let message = error.as_ref().map(|e| e.to_string());
        self.steps.push(Step {
            phase,
            label: label.to_owned(),
            role: self.role.clone(),
            session: self.session.clone(),
            action,
            target,
            scene,
            success,
            error,
        });
        match (success, message) {
            (true, _) => Ok(()),
            (false, m) => Err(m.unwrap_or_else(|| "the action failed".into())),
        }
    }
    fn action(&self, family: &str, op: &str, payload: Value) -> ActionEnvelope {
        ActionEnvelope::new(family, op, self.machine.clone(), payload)
    }
    fn gesture(
        &mut self,
        phase: usize,
        label: &str,
        gesture: &Gesture,
    ) -> std::result::Result<(), String> {
        let (width, height) = (self.options.width, self.options.height);
        match gesture {
            Gesture::Click { target, at } | Gesture::DoubleClick { target, at } => {
                let scene = self.scene()?;
                let resolved = target.resolve_at(&scene, *at).ok_or_else(|| {
                    format!(
                        "nothing on screen a click reaches matches {}",
                        describe(target)
                    )
                })?;
                let op = match gesture {
                    Gesture::DoubleClick { .. } => "double_click",
                    _ => "click",
                };
                let action = self.action(
                    "pointer.v1",
                    op,
                    json!({"x": resolved.x, "y": resolved.y, "width": width, "height": height}),
                );
                self.counts.pointer_actions += 1;
                self.counts.observations += 1;
                self.send(phase, label, action, Some(resolved), scene.digest)
            }
            Gesture::Drag {
                from,
                from_at,
                to,
                to_at,
            } => {
                let scene = self.scene()?;
                let find = |t: &Target, at: Option<[u32; 2]>| {
                    t.resolve_at(&scene, at).ok_or_else(|| {
                        format!("nothing on screen a click reaches matches {}", describe(t))
                    })
                };
                let (start, end) = (find(from, *from_at)?, find(to, *to_at)?);
                let point =
                    |r: &Resolved| json!({"x": r.x, "y": r.y, "width": width, "height": height});
                let (down, up) = (point(&start), point(&end));
                self.counts.pointer_actions += 1;
                self.counts.observations += 1;
                self.send(
                    phase,
                    label,
                    self.action("pointer.v1", "down", down),
                    Some(start),
                    scene.digest,
                )?;
                for (op, at) in [("move", up.clone()), ("up", up)] {
                    let digest = self.scene()?.digest;
                    self.counts.pointer_actions += 1;
                    self.counts.observations += 1;
                    self.send(
                        phase,
                        label,
                        self.action("pointer.v1", op, at),
                        Some(end.clone()),
                        digest,
                    )?;
                }
                Ok(())
            }
            Gesture::Scroll { target, delta_y } => {
                let scene = self.scene()?;
                let resolved = target.resolve(&scene).ok_or_else(|| {
                    format!(
                        "nothing on screen a click reaches matches {}",
                        describe(target)
                    )
                })?;
                let action = self.action(
                    "pointer.v1",
                    "wheel",
                    json!({"x": resolved.x, "y": resolved.y, "width": width, "height": height, "delta_y": delta_y}),
                );
                self.counts.pointer_actions += 1;
                self.counts.observations += 1;
                self.send(phase, label, action, Some(resolved), scene.digest)
            }
            Gesture::ClickModel { point } => {
                let scene = self.scene()?;
                let (x, y) = self.model_point(&scene, *point)?;
                let hit = scene
                    .hit_test(x, y)
                    .ok_or_else(|| format!("model point {point:?} is not on a control"))?;
                let resolved = resolved(&scene, hit, hit, x, y);
                let action = self.action(
                    "pointer.v1",
                    "click",
                    json!({"x": x, "y": y, "width": width, "height": height}),
                );
                self.counts.pointer_actions += 1;
                self.counts.observations += 1;
                self.send(phase, label, action, Some(resolved), scene.digest)
            }
            Gesture::Key { key } => self.key(phase, label, key),
            Gesture::Type {
                text,
                verbatim: true,
            } => {
                self.counts.typed_characters += text.chars().count();
                let digest = self.scene()?.digest;
                let action = self.action("keyboard.v1", "type", json!({"text": text}));
                self.counts.keyboard_actions += 1;
                self.counts.observations += 1;
                self.send(phase, label, action, None, digest)
            }
            Gesture::Type { text, .. } => {
                for (i, line) in text.split('\n').enumerate() {
                    if i > 0 {
                        self.key(phase, label, "Enter")?;
                    }
                    self.counts.typed_characters += line.chars().count();
                    let runs: Vec<String> = match self.options.typing {
                        Typing::Text if line.is_empty() => vec![],
                        Typing::Text => vec![line.to_owned()],
                        Typing::Keys => line.chars().map(String::from).collect(),
                    };
                    for run in runs {
                        let digest = self.scene()?.digest;
                        let action = self.action("keyboard.v1", "type", json!({"text": run}));
                        self.counts.keyboard_actions += 1;
                        self.counts.observations += 1;
                        self.send(phase, label, action, None, digest)?;
                    }
                }
                Ok(())
            }
        }
    }
    /// Where a model point appears in the focused FreeCAD window's 3D view, through the
    /// camera that window holds. The demonstrator reads the camera as the person's eye
    /// would; the actor never receives it.
    #[cfg(feature = "freecad")]
    fn model_point(&self, scene: &Scene, p: [f64; 3]) -> std::result::Result<(i32, i32), String> {
        let session = self
            .world
            .interfaces()
            .session(&self.session)
            .map_err(|e| e.to_string())?;
        let desktop = session
            .machines
            .get(&self.machine)
            .map(|m| serde_json::to_value(&m.desktop))
            .ok_or("the session has no such machine")?
            .map_err(|e| e.to_string())?;
        let focused = scene
            .windows
            .iter()
            .filter(|w| w.app == "freecad")
            .max_by_key(|w| (w.focused, w.z))
            .ok_or("no FreeCAD window is open")?;
        let state = desktop["windows"]
            .as_object()
            .and_then(|ws| {
                ws.iter().find(|(key, w)| {
                    *key == &focused.id.to_string() || w["id"] == json!(focused.id)
                })
            })
            .map(|(_, w)| &w["state"])
            .ok_or("the FreeCAD window has no state")?;
        let camera: cw_cad::view::Camera =
            serde_json::from_value(state["camera"].clone()).map_err(|e| e.to_string())?;
        let view = scene
            .nodes
            .iter()
            .filter(|n| n.window == Some(focused.id))
            .find(|n| {
                n.interaction
                    .as_deref()
                    .is_some_and(|i| i.contains(":content:freecad:view:"))
            })
            .map(Node::painted_bounds)
            .ok_or("the FreeCAD window shows no 3D view")?;
        let (x, y, _) = camera
            .project(cw_cad::math::v3(p[0], p[1], p[2]), view.width, view.height)
            .ok_or("the point is behind the camera")?;
        Ok((view.x + x.round() as i32, view.y + y.round() as i32))
    }
    #[cfg(not(feature = "freecad"))]
    fn model_point(&self, _: &Scene, _: [f64; 3]) -> std::result::Result<(i32, i32), String> {
        Err("this build has no CAD engine".into())
    }
    fn key(&mut self, phase: usize, label: &str, key: &str) -> std::result::Result<(), String> {
        let digest = self.scene()?.digest;
        let action = self.action("keyboard.v1", "key", json!({"key": key}));
        self.counts.keyboard_actions += 1;
        self.counts.observations += 1;
        self.send(phase, label, action, None, digest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn glob_matches_runs() {
        assert!(glob("window:*:content:open:*", "window:3:content:open:12"));
        assert!(glob("shell:launch:files", "shell:launch:files"));
        assert!(!glob("shell:launch:files", "shell:launch:files2"));
        assert!(glob("*save*", "window:0:content:sheet:save:calc"));
        assert!(!glob("a*b", "ab-a"));
        assert!(glob("a*a", "aa"));
        assert!(!glob("aa*aa", "aaa"));
    }
    #[test]
    fn placeholders_resolve_or_refuse() {
        let values = BTreeMap::from([("client".to_owned(), "Okafor".to_owned())]);
        assert_eq!(
            substitute("Dear {{ client }},", &values).unwrap(),
            "Dear Okafor,"
        );
        assert!(substitute("{{fee}}", &values).is_err());
        assert!(substitute("{{client", &values).is_err());
    }
    #[test]
    fn instantiation_is_a_function_of_the_seed() {
        let procedure = Procedure::from_json(
            r#"{"id":"t","instruction":"Bill {{client}} {{total}}","machine":"m",
                "params":[{"name":"client","kind":"choice","choices":["A","B","C"]},
                          {"name":"a","kind":"integer","min":1,"max":9},
                          {"name":"b","kind":"integer","min":10,"max":90},
                          {"name":"total","kind":"sum","of":["a","b"]}],
                "phases":[],"verify":[]}"#,
        )
        .unwrap();
        let none = BTreeMap::new();
        let one = procedure.instantiate(3, &none).unwrap();
        assert_eq!(one, procedure.instantiate(3, &none).unwrap());
        let total: i64 =
            one.values["a"].parse::<i64>().unwrap() + one.values["b"].parse::<i64>().unwrap();
        assert_eq!(one.values["total"], total.to_string());
        assert_eq!(
            one.procedure.instruction,
            format!("Bill {} {}", one.values["client"], total)
        );
        // Overriding one value leaves the others where the seed put them.
        let pinned = procedure
            .instantiate(3, &BTreeMap::from([("client".to_owned(), "Z".to_owned())]))
            .unwrap();
        assert_eq!(pinned.values["a"], one.values["a"]);
        assert_eq!(pinned.values["client"], "Z");
        assert!(procedure
            .instantiate(3, &BTreeMap::from([("nope".to_owned(), "1".to_owned())]))
            .is_err());
        let seeds: std::collections::BTreeSet<_> = (0..64)
            .map(|s| procedure.instantiate(s, &none).unwrap().values)
            .collect();
        assert!(seeds.len() > 32, "64 seeds gave {} episodes", seeds.len());
    }
}
