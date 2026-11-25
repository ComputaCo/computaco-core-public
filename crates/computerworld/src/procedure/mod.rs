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
    /// The desktop each machine shows (`metadata.desktop_themes`), for a world that
    /// boots without one: a procedure on a screen needs a screen. Applied to the world
    /// definition before it boots, by [`Procedure::prepare`].
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub desktops: BTreeMap<String, String>,
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
    /// One of several records drawn together, so linked values stay linked: a person's
    /// account, machine and home folder. Each field is the parameter `<name>.<field>`;
    /// `<name>` itself is the record's `id` field, or its position when it has none.
    Record {
        choices: Vec<BTreeMap<String, String>>,
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
    #[serde(default)]
    pub content: String,
    /// The file's bytes in base64 instead of `content`: an image, a recording, a
    /// database.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base64: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Phase {
    /// The step as the expert would name it: "Record the retainer in the letter".
    pub label: String,
    /// Who does it, in a team procedure. A phase with no role, no gestures and no tool
    /// calls is the world's turn (its captures and owner writes), in any procedure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    pub gui: Vec<Gesture>,
    /// The same step as tool calls. A phase that only moves around the screen (opening
    /// a folder) has none.
    #[serde(default)]
    pub tool: Vec<ToolCall>,
    /// Values the world gave out during the phase (the number of the pull request just
    /// opened, the id of the mail just sent), each bound to a name once the phase's
    /// actions are done, in either mode. Later gestures, tool calls, captures and checks
    /// use it as `{{@name}}` (`{{#@name}}` for a JSON number), so a procedure names what
    /// it made rather than the id it would have had in a fresh world.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capture: Vec<Capture>,
    /// What the world's owner writes once the phase's actions are done and its captures
    /// bound, in either mode: the part of the work nobody at these machines does (another
    /// store's bid on the request just posted). A phase may be nothing else.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub owner: Vec<ServicePatch>,
}
/// One write of the world's owner to a service's state (`Phase::owner`): an RFC 7386 merge
/// patch, made with [`World::patch_service_state`], so the world's journal and trajectory
/// carry it. Keys and values may name captures (`{{@name}}`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServicePatch {
    pub service: String,
    pub patch: Value,
}
/// One runtime value bound to a name (`Phase::capture`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capture {
    pub name: String,
    pub from: Source,
    /// Where the value sits in the source's text: `{{?}}` is the value, an identifier
    /// (letters, digits, `-`, `_`, and a `.` between them), and `{{*}}` any run of
    /// characters. The pattern may match anywhere in a candidate. Without one, the
    /// candidate's whole text is the value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    /// Take the last candidate that matches instead of the first: the newest of a
    /// collection (object keys in natural order, `mail-9` before `mail-10`), the last
    /// line of a file.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub last: bool,
}
/// Where a capture reads its value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Source {
    /// `World::inspect()`: the value at a pointer, or one element of a collection
    /// selected by content.
    State(Select),
    /// A file on a machine (the role's own when absent): the whole text, or each line
    /// with a pattern.
    File {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        machine: Option<String>,
        path: String,
    },
    /// The text painted on the role's screen, node by node, top of the scene's order
    /// first; a pattern is required. `app` limits it to one application's windows.
    Scene {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        app: Option<String>,
    },
    /// The value the phase's last action returned, at `pointer` inside it.
    Outcome {
        #[serde(default)]
        pointer: String,
    },
}
/// A value in `World::inspect()`, or, with `where`, the elements of the collection at
/// `pointer` (an array or an object) whose fields match: each key of `where` is a
/// pointer inside the element, each value what it must hold. A string matches with
/// `{{*}}` standing for any run of characters, and an array field matches when any of
/// its items does.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Select {
    pub pointer: String,
    #[serde(rename = "where", default, skip_serializing_if = "Option::is_none")]
    pub matching: Option<BTreeMap<String, Value>>,
    /// With `where`: the pointer inside the selected element whose value is captured;
    /// its key (or array index) when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCall {
    pub family: String,
    pub op: String,
    #[serde(default)]
    pub payload: Value,
}
fn is_left(button: &u8) -> bool {
    *button == 0
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
        /// The button: 0 the left (the default), 2 the right, which asks a control for
        /// its context menu.
        #[serde(default, skip_serializing_if = "is_left")]
        button: u8,
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
        /// Where inside the target the wheel turns, as for a click: a 3D view zooms
        /// about the pointer.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<[u32; 2]>,
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
    /// Let world time pass with nobody acting: a review queue works through its
    /// business days, a letter arrives, a deadline lapses. The units add up; a tick is a
    /// microsecond. Recorded as a step whose `advance` is the ticks, which a replay
    /// reproduces with `World::advance` rather than by sending an action.
    Wait {
        #[serde(default)]
        days: u64,
        #[serde(default)]
        hours: u64,
        #[serde(default)]
        minutes: u64,
        #[serde(default)]
        seconds: u64,
        #[serde(default)]
        ticks: u64,
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
    /// The whole file equals `pattern`, where each `{{*}}` stands for any run of
    /// characters: the identifiers an application generates (KiCad's UUIDs, a
    /// timestamp), in a file whose own text uses `*` (Gerber).
    FileMatches {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        machine: Option<String>,
        path: String,
        pattern: String,
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
    /// The file is a plot exported by KiCad's simulator as CSV (`;`-separated, a first
    /// column `time` or `frequency`), and the schematic at `schematic`, simulated by the
    /// eda engine with `command`, gives every column it holds: each value, at the
    /// file's own x, within `tolerance` of its column's swing (AC columns are
    /// `V(/out) (mag)` in dB and `V(/out) (phase)` in degrees, as KiCad writes them).
    /// Real KiCad's export (ngspice) and the simulated one are judged alike.
    SpiceMatches {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        machine: Option<String>,
        path: String,
        schematic: String,
        command: String,
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
    /// The file's bytes hash to `sha256` (lower-case hex): a binary output, an exported
    /// image or a render, checked exactly.
    FileSha256 {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        machine: Option<String>,
        path: String,
        sha256: String,
    },
    /// A predicate over `World::inspect()`.
    State(Predicate),
    /// Some element of the collection at `pointer` in `World::inspect()` matches
    /// `where` (as for [`Select`]), or, with `count`, exactly that many do: the pull
    /// request whose head is the branch, the mail whose body names the order, by what
    /// they hold rather than by the key a fresh world would have given them.
    Find {
        pointer: String,
        #[serde(rename = "where", default)]
        matching: BTreeMap<String, Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        count: Option<usize>,
    },
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
    /// The values the phases captured, by name (`Phase::capture`): what the checks'
    /// `{{@name}}` placeholders were resolved with.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub captures: BTreeMap<String, String>,
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
    /// Ticks of world time the owner let pass at this step (a `wait`), in place of
    /// sending `action`: replay it with `World::advance(advance)`.
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub advance: u64,
    /// The owner's write this step made (`Phase::owner`), with its captures filled in. A
    /// replay makes it with `World::patch_service_state` rather than by sending an action.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<ServicePatch>,
}
fn is_zero(value: &usize) -> bool {
    *value == 0
}
fn is_zero_u64(value: &u64) -> bool {
    *value == 0
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
    /// World time let pass by `wait` gestures, in ticks.
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub waited: u64,
    /// The owner's writes to services (`Phase::owner`): the world's part, not an action.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub owner_writes: usize,
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

/// Which placeholders a substitution fills: the parameters when an instance is drawn
/// (captured `{{@name}}`s pass through untouched), or the captures while it runs.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pass {
    Params,
    Captures,
}
impl Pass {
    /// The key `name` is looked up under in this pass, or `None` to leave it as it is.
    fn key(self, name: &str) -> Option<&str> {
        // `{{#@name}}` is a captured number inside a longer string: still a capture.
        let bare = name.strip_prefix('#').unwrap_or(name);
        match (self, bare.strip_prefix('@')) {
            (Self::Params, None) => Some(name),
            (Self::Captures, Some(captured)) => Some(captured),
            _ => None,
        }
    }
    fn missing(self, name: &str) -> SimError {
        match self {
            Self::Params => invalid(format!("no parameter named {name:?}")),
            Self::Captures => invalid(format!("nothing was captured as {name:?}")),
        }
    }
}

/// Replace every `{{name}}` in `text`. A placeholder with no value is an error, so a
/// misspelt parameter cannot quietly put braces into a legal letter.
fn substitute(text: &str, values: &BTreeMap<String, String>) -> Result<String> {
    substitute_in(text, values, Pass::Params)
}
fn substitute_in(text: &str, values: &BTreeMap<String, String>, pass: Pass) -> Result<String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let end = after
            .find("}}")
            .ok_or_else(|| invalid(format!("unclosed placeholder in {text:?}")))?;
        let name = after[..end].trim();
        // `{{*}}` is the file_matches wildcard and `{{?}}` a capture pattern's value,
        // not parameters; a capture waits for its own pass.
        let Some(key) = Some(name)
            .filter(|n| *n != "*" && *n != "?")
            .and_then(|n| pass.key(n))
        else {
            out.push_str(&rest[start..start + 2 + end + 2]);
            rest = &after[end + 2..];
            continue;
        };
        let value = values.get(key).ok_or_else(|| pass.missing(name))?;
        out.push_str(value);
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    Ok(out)
}
fn substitute_all(value: &mut Value, values: &BTreeMap<String, String>, pass: Pass) -> Result<()> {
    match value {
        // A string that is exactly `{{#name}}` becomes the value as a JSON number, for
        // checks on services that store amounts and quantities as numbers.
        Value::String(s)
            if s.starts_with("{{#")
                && s.ends_with("}}")
                && !s[3..].contains("{{")
                && pass.key(s[3..s.len() - 2].trim()).is_some() =>
        {
            let name = s[3..s.len() - 2].trim();
            let key = pass.key(name).unwrap_or(name);
            let text = values.get(key).ok_or_else(|| pass.missing(name))?;
            *value = serde_json::from_str::<serde_json::Number>(text.trim())
                .map(Value::Number)
                .map_err(|_| invalid(format!("{name} = {text:?} is not a number")))?;
        }
        Value::String(s) => *s = substitute_in(s, values, pass)?,
        Value::Array(items) => {
            for item in items {
                substitute_all(item, values, pass)?;
            }
        }
        Value::Object(map) => {
            // A key may name a value too: the record an owner's patch writes under.
            if map.keys().any(|k| k.contains("{{")) {
                let mut renamed = serde_json::Map::new();
                for (key, item) in std::mem::take(map) {
                    renamed.insert(substitute_in(&key, values, pass)?, item);
                }
                *map = renamed;
            }
            for item in map.values_mut() {
                substitute_all(item, values, pass)?;
            }
        }
        _ => {}
    }
    Ok(())
}
/// `item` with its `{{@name}}` placeholders filled from `captures`; `item` itself (no
/// copy made through JSON) when it has none.
fn bind<T>(item: &T, captures: &BTreeMap<String, String>) -> Result<T>
where
    T: Serialize + serde::de::DeserializeOwned + Clone,
{
    let mut value = serde_json::to_value(item).map_err(|e| invalid(e.to_string()))?;
    if !mentions_capture(&value) {
        return Ok(item.clone());
    }
    substitute_all(&mut value, captures, Pass::Captures)?;
    serde_json::from_value(value).map_err(|e| invalid(e.to_string()))
}
fn mentions_capture(value: &Value) -> bool {
    match value {
        Value::String(s) => s.contains("{{@") || s.contains("{{#@"),
        Value::Array(items) => items.iter().any(mentions_capture),
        Value::Object(map) => map
            .iter()
            .any(|(k, v)| k.contains("{{@") || k.contains("{{#@") || mentions_capture(v)),
        _ => false,
    }
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
                ParamSpec::Record { choices } if choices.is_empty() => {
                    return Err(invalid(format!("{} has no choices", param.name)));
                }
                ParamSpec::Record { choices } => {
                    let index = (roll % choices.len() as u64) as usize;
                    let record = &choices[index];
                    for (field, value) in record {
                        let key = format!("{}.{field}", param.name);
                        values.insert(key.clone(), overrides.get(&key).unwrap_or(value).clone());
                    }
                    record
                        .get("id")
                        .cloned()
                        .unwrap_or_else(|| index.to_string())
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
        substitute_all(&mut body, &values, Pass::Params)?;
        let procedure = serde_json::from_value(body).map_err(|e| invalid(e.to_string()))?;
        Ok(Instance {
            seed,
            values,
            procedure,
        })
    }
}

fn glob(pattern: &str, text: &str) -> bool {
    wildcard(pattern, "*", text)
}
/// `text` equals `pattern` with each `star` standing for any run of characters.
fn wildcard(pattern: &str, star: &str, text: &str) -> bool {
    let mut parts = pattern.split(star);
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
/// Whether a field of a collection's element holds what a `where` asks: a string by
/// pattern (`{{*}}` any run), anything else by value, an array when any item does.
fn holds(found: &Value, want: &Value) -> bool {
    let direct = match (found, want) {
        (Value::String(have), Value::String(pattern)) => wildcard(pattern, "{{*}}", have),
        (Value::Number(_) | Value::Bool(_), Value::String(pattern)) => {
            wildcard(pattern, "{{*}}", &found.to_string())
        }
        _ => cw_evaluation::json_equal(found, want),
    };
    direct
        || matches!((found, want), (Value::Array(items), w) if !w.is_array()
            && items.iter().any(|item| holds(item, want)))
}
/// Compare keys as a person reads them: runs of digits by number, so `mail-9` comes
/// before `mail-10` and `999` before `1000`.
fn natural(a: &str, b: &str) -> std::cmp::Ordering {
    fn runs(s: &str) -> Vec<(bool, &str)> {
        let mut out = Vec::new();
        let mut start = 0;
        let bytes = s.as_bytes();
        for i in 1..=bytes.len() {
            if i == bytes.len() || bytes[i].is_ascii_digit() != bytes[start].is_ascii_digit() {
                out.push((bytes[start].is_ascii_digit(), &s[start..i]));
                start = i;
            }
        }
        out
    }
    let (x, y) = (runs(a), runs(b));
    for ((dx, rx), (dy, ry)) in x.iter().zip(&y) {
        let order = if *dx && *dy {
            let (tx, ty) = (rx.trim_start_matches('0'), ry.trim_start_matches('0'));
            tx.len().cmp(&ty.len()).then(tx.cmp(ty))
        } else {
            rx.cmp(ry)
        };
        if order.is_ne() {
            return order;
        }
    }
    x.len().cmp(&y.len()).then(a.cmp(b))
}
/// The elements of the collection at `pointer` whose fields hold what `matching` asks,
/// with their keys (array indices for an array), in order: an array's own, an object's
/// keys in natural order.
fn select<'v>(
    state: &'v Value,
    pointer: &str,
    matching: &BTreeMap<String, Value>,
) -> Vec<(String, &'v Value)> {
    let mut elements: Vec<(String, &Value)> = match state.pointer(pointer) {
        Some(Value::Array(items)) => items
            .iter()
            .enumerate()
            .map(|(i, v)| (i.to_string(), v))
            .collect(),
        Some(Value::Object(map)) => map.iter().map(|(k, v)| (k.clone(), v)).collect(),
        _ => Vec::new(),
    };
    if matches!(state.pointer(pointer), Some(Value::Object(_))) {
        elements.sort_by(|a, b| natural(&a.0, &b.0));
    }
    elements.retain(|(_, element)| {
        matching
            .iter()
            .all(|(at, want)| element.pointer(at).is_some_and(|found| holds(found, want)))
    });
    elements
}
/// A JSON value as the text a capture binds: a string as itself, anything else as JSON.
fn text_of(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}
/// The identifier `pattern`'s `{{?}}` stands for where the pattern first matches in
/// `text` (anywhere in it), with `{{*}}` matching any run of characters.
fn extract(pattern: &str, text: &str) -> Option<String> {
    #[derive(Debug)]
    enum Token<'p> {
        Literal(&'p str),
        Any,
        Value,
    }
    let mut tokens = Vec::new();
    let mut rest = pattern;
    while let Some(start) = rest.find("{{") {
        let (token, len) = if rest[start..].starts_with("{{*}}") {
            (Token::Any, 5)
        } else if rest[start..].starts_with("{{?}}") {
            (Token::Value, 5)
        } else {
            (Token::Literal(&rest[start..start + 2]), 2)
        };
        if start > 0 {
            tokens.push(Token::Literal(&rest[..start]));
        }
        tokens.push(token);
        rest = &rest[start + len..];
    }
    if !rest.is_empty() {
        tokens.push(Token::Literal(rest));
    }
    fn identifier(text: &str) -> usize {
        let chars: Vec<(usize, char)> = text.char_indices().collect();
        let word = |c: char| c.is_alphanumeric() || c == '-' || c == '_';
        let mut end = 0;
        for (i, &(at, c)) in chars.iter().enumerate() {
            let inner_dot = c == '.' && end > 0 && chars.get(i + 1).is_some_and(|(_, n)| word(*n));
            if !(word(c) || inner_dot) {
                break;
            }
            end = at + c.len_utf8();
        }
        end
    }
    fn walk(tokens: &[Token], text: &str, found: &mut Option<String>) -> bool {
        let Some((first, rest)) = tokens.split_first() else {
            return true;
        };
        match first {
            Token::Literal(l) => text
                .strip_prefix(l)
                .is_some_and(|after| walk(rest, after, found)),
            Token::Any => text
                .char_indices()
                .map(|(i, _)| i)
                .chain([text.len()])
                .any(|i| walk(rest, &text[i..], found)),
            Token::Value => {
                let longest = identifier(text);
                let ends: Vec<usize> = text[..longest]
                    .char_indices()
                    .map(|(i, c)| i + c.len_utf8())
                    .collect();
                ends.into_iter().rev().any(|end| {
                    walk(rest, &text[end..], found) && {
                        found.get_or_insert_with(|| text[..end].to_owned());
                        true
                    }
                })
            }
        }
    }
    if tokens.iter().filter(|t| matches!(t, Token::Value)).count() != 1 {
        return None;
    }
    text.char_indices()
        .map(|(i, _)| i)
        .chain([text.len()])
        .find_map(|i| {
            let mut found = None;
            walk(&tokens, &text[i..], &mut found)
                .then_some(found)
                .flatten()
        })
}
impl Target {
    fn matches(&self, scene: &Scene, node: &Node) -> bool {
        let semantic = node.semantic.as_ref();
        let role = semantic.map(|s| s.role.as_str());
        let label = semantic.map(|s| s.label.as_str());
        self.role.as_deref().is_none_or(|r| role == Some(r))
            && self.label.as_deref().is_none_or(|l| {
                // `Matters/` names a folder; GNOME's Files and file chooser label it `Matters`.
                label == Some(l)
                    || l.len() > 1 && l.ends_with('/') && label == Some(l.trim_end_matches('/'))
            })
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

/// How a build judges what only a closed engine can judge: a saved CAD part against its
/// reference, a KiCad simulator export against the schematic's simulation, and where a
/// model point appears in a FreeCAD window's 3D view. A build carries no such engine and
/// never passes one of these checks until a crate that has it sets [`set_judges`].
#[derive(Clone, Copy)]
pub struct Judges {
    /// The saved FreeCAD document (`saved`) against its reference op-tree, its bodies'
    /// volumes within `tolerance`.
    pub cad_matches: fn(saved: &str, reference: &Value, tolerance: f64) -> bool,
    /// A KiCad simulator CSV export against the eda engine's simulation of `schematic`
    /// run with `command`, every column within `tolerance` of its swing.
    pub spice_matches: fn(csv: &str, schematic: &str, command: &str, tolerance: f64) -> bool,
    /// Model point `p` through the FreeCAD window's camera (its state's `camera`), in a
    /// view `width` by `height`: the point's offset in the view.
    pub cad_project: CadProject,
}
/// Where model point `p` lands through a FreeCAD window's camera (its state's `camera`) in a
/// view `width` by `height`.
pub type CadProject = fn(
    camera: &Value,
    p: [f64; 3],
    width: u32,
    height: u32,
) -> std::result::Result<(f64, f64), String>;
static JUDGES: std::sync::RwLock<Option<Judges>> = std::sync::RwLock::new(None);
/// Judge with `judges` from now on.
pub fn set_judges(judges: Judges) {
    *JUDGES.write().unwrap_or_else(|e| e.into_inner()) = Some(judges);
}
fn judges() -> Option<Judges> {
    *JUDGES.read().unwrap_or_else(|e| e.into_inner())
}
fn cad_matches(saved: &str, reference: &Value, tolerance: f64) -> bool {
    judges().is_some_and(|j| (j.cad_matches)(saved, reference, tolerance))
}
fn spice_matches(csv: &str, schematic: &str, command: &str, tolerance: f64) -> bool {
    judges().is_some_and(|j| (j.spice_matches)(csv, schematic, command, tolerance))
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
    /// Make `definition` ready for this procedure before the world boots: give each
    /// machine in `desktops` its desktop. Themes already set for other machines stay.
    pub fn prepare(&self, definition: &mut crate::WorldDefinition) {
        if self.desktops.is_empty() {
            return;
        }
        if !definition.metadata.is_object() {
            definition.metadata = json!({});
        }
        let themes = definition.metadata["desktop_themes"].take();
        let mut themes = match themes {
            Value::Object(map) => map,
            _ => serde_json::Map::new(),
        };
        for (machine, theme) in &self.desktops {
            themes.insert(machine.clone(), json!(theme));
        }
        definition.metadata["desktop_themes"] = Value::Object(themes);
    }
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
impl Phase {
    /// The world's turn: a phase with no role, no gestures and no tool calls, only
    /// captures and owner writes. Nobody acts in it, so it needs no session or machine.
    pub fn is_the_worlds(&self) -> bool {
        self.role.is_none() && self.gui.is_empty() && self.tool.is_empty()
    }
}
impl Check {
    fn file(&self) -> Option<(Option<&str>, &str)> {
        match self {
            Self::FileEquals { machine, path, .. }
            | Self::FileContains { machine, path, .. }
            | Self::FileHasLine { machine, path, .. }
            | Self::FileLacks { machine, path, .. }
            | Self::FileMatches { machine, path, .. }
            | Self::FileSha256 { machine, path, .. }
            | Self::FileJson { machine, path, .. }
            | Self::CadMatches { machine, path, .. }
            | Self::SpiceMatches { machine, path, .. } => Some((machine.as_deref(), path)),
            Self::State(_) | Self::GitFile { .. } | Self::Find { .. } => None,
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
            let bytes = match &file.base64 {
                Some(text) => crate::decode_base64(text)
                    .ok()
                    .ok_or_else(|| invalid(format!("{} is not valid base64", file.path)))?,
                None => file.content.clone().into_bytes(),
            };
            self.runtime_mut()
                .write_file(machine, "owner", &file.path, &bytes)?;
        }
        Ok(())
    }
    /// Judge the instance's checks against the world as it is now. A check that names
    /// a captured value (`{{@name}}`) fails here: use [`World::verify_with`] and the
    /// demonstration's `captures`.
    pub fn verify(&self, instance: &Instance) -> Verification {
        self.verify_with(instance, &BTreeMap::new())
    }
    /// Judge the instance's checks with `captures` filling their `{{@name}}`
    /// placeholders; a check whose capture is missing fails.
    pub fn verify_with(
        &self,
        instance: &Instance,
        captures: &BTreeMap<String, String>,
    ) -> Verification {
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
                let Ok(check) = bind(check, captures) else {
                    return CheckResult {
                        check: check.clone(),
                        passed: false,
                    };
                };
                let check = &check;
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
                    Check::FileSha256 {
                        machine,
                        path,
                        sha256,
                        ..
                    } => {
                        use sha2::Digest;
                        self.runtime()
                            .read_file(
                                machine.as_deref().unwrap_or(&instance.procedure.machine),
                                path,
                            )
                            .is_ok_and(|bytes| {
                                let digest = sha2::Sha256::digest(&bytes);
                                let hex: String =
                                    digest.iter().map(|b| format!("{b:02x}")).collect();
                                hex == sha256.to_ascii_lowercase()
                            })
                    }
                    Check::FileMatches { pattern, .. } => {
                        file.is_some_and(|c| wildcard(pattern, "{{*}}", &c))
                    }
                    Check::FileJson { pointer, value, .. } => file.is_some_and(|c| {
                        serde_json::from_str::<Value>(&c).is_ok_and(|v| {
                            v.pointer(pointer)
                                .is_some_and(|found| cw_evaluation::json_equal(found, value))
                        })
                    }),
                    Check::CadMatches {
                        reference,
                        tolerance,
                        ..
                    } => file.is_some_and(|c| cad_matches(&c, reference, *tolerance)),
                    Check::SpiceMatches {
                        machine,
                        schematic,
                        command,
                        tolerance,
                        ..
                    } => file.is_some_and(|c| {
                        read(machine.as_deref(), schematic)
                            .is_some_and(|sch| spice_matches(&c, &sch, command, *tolerance))
                    }),
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
                    Check::Find {
                        pointer,
                        matching,
                        count,
                    } => {
                        let state = inspection.get_or_insert_with(|| self.inspect());
                        let found = select(state, pointer, matching).len();
                        count.map_or(found > 0, |want| found == want)
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
            if phase.is_the_worlds() {
                assigned.push(None);
                continue;
            }
            let role = phase.role.as_deref().unwrap_or("");
            let session = sessions.get(role).ok_or_else(|| {
                invalid(format!("no session for role {role:?} of {}", procedure.id))
            })?;
            self.validate_session(session)?;
            let machine = procedure.machine_of(phase.role.as_deref())?.to_owned();
            assigned.push(Some((session.clone(), machine)));
        }
        // The world's turn is recorded on the screen of whoever acted last; before
        // anybody has, on the first actor's.
        let (first_session, first_machine) = assigned
            .iter()
            .flatten()
            .next()
            .cloned()
            .unwrap_or_else(|| {
                (
                    sessions.get("").cloned().unwrap_or_default(),
                    procedure.machine.clone(),
                )
            });
        let mut run = Run {
            world: self,
            session: first_session,
            machine: first_machine,
            role: None,
            options,
            steps: Vec::new(),
            counts: Counts {
                phases: procedure.phases.len(),
                ..Counts::default()
            },
            captures: BTreeMap::new(),
            returned: Value::Null,
        };
        let mut failure = None;
        'phases: for (index, (phase, actor)) in procedure.phases.iter().zip(assigned).enumerate() {
            if let Some((session, machine)) = actor {
                run.session = session;
                run.machine = machine;
            }
            run.role = phase.role.clone();
            run.returned = Value::Null;
            let outcome = match options.mode {
                Mode::Gui => phase.gui.iter().try_for_each(|gesture| {
                    let gesture = bind(gesture, &run.captures).map_err(|e| e.message)?;
                    run.gesture(index, &phase.label, &gesture)
                }),
                Mode::Tool => run.tool_phase(index, phase),
            };
            let outcome = outcome
                .and_then(|()| run.bind_captures(phase))
                .and_then(|()| run.owner_writes(index, phase));
            if let Err(message) = outcome {
                failure = Some(format!("phase {index} ({}): {message}", phase.label));
                break 'phases;
            }
        }
        let Run {
            steps,
            counts,
            captures,
            ..
        } = run;
        Ok(Demonstration {
            procedure: procedure.id.clone(),
            instruction: procedure.instruction.clone(),
            seed: instance.seed,
            options,
            completed: failure.is_none(),
            failure,
            verification: self.verify_with(instance, &captures),
            captures,
            state_hash: self.state_hash()?,
            steps,
            counts,
        })
    }
}

impl World {
    /// Perform one gesture through `session`, on the machine it has in focus, and
    /// return the steps it took (a type with newlines is several). The same grounding a
    /// procedure gets: a target is found on the live screen and clicked where a click
    /// reaches it. This is how a policy that is not a script (a language model playing
    /// a person) acts like one, through what is on its screen rather than coordinates.
    pub fn perform(
        &mut self,
        session: &str,
        gesture: &Gesture,
        options: ProjectOptions,
    ) -> Result<Vec<Step>> {
        let machine = self.interfaces().session(session)?.focused_machine.clone();
        let mut run = Run {
            world: self,
            session: session.to_owned(),
            machine,
            role: None,
            options,
            steps: Vec::new(),
            counts: Counts::default(),
            captures: BTreeMap::new(),
            returned: Value::Null,
        };
        let outcome = run.gesture(0, "", gesture);
        let steps = run.steps;
        match outcome {
            // A gesture whose target is not on screen took no step: say so plainly.
            Err(message) if steps.is_empty() => Err(SimError::new("not_found", message)),
            _ => Ok(steps),
        }
    }
}

/// A procedure a phase at a time, for a caller that performs some phases itself: cwfid plays
/// one role's gestures on a real application and on the simulator in lockstep, while the
/// other roles' phases happen in the world through their tool paths, off screen, so what
/// they make (a pull request, a mail) is there when the role it was made for looks.
impl World {
    /// Perform phase `index` of `procedure` through its tool path as `session`, on the
    /// machine its role acts on, then bind its captures into `captures` and make its owner
    /// writes.
    pub fn perform_phase_tools(
        &mut self,
        session: &str,
        procedure: &Procedure,
        index: usize,
        options: ProjectOptions,
        captures: &mut BTreeMap<String, String>,
    ) -> Result<()> {
        let phase = procedure
            .phases
            .get(index)
            .ok_or_else(|| invalid(format!("no phase {index}")))?;
        let mut run = self.phase_run(session, procedure, phase, options, captures)?;
        let outcome = run
            .tool_phase(index, phase)
            .and_then(|()| run.bind_captures(phase))
            .and_then(|()| run.owner_writes(index, phase));
        *captures = std::mem::take(&mut run.captures);
        outcome.map_err(|m| invalid(format!("phase {index} ({}): {m}", phase.label)))
    }
    /// Bind phase `index`'s captures into `captures`, read from the world as it is now: after
    /// its gestures were performed some other way. A capture that cannot be read is an error
    /// and binds nothing; the others are still bound. With every capture bound, the phase's
    /// owner writes are made.
    pub fn bind_phase_captures(
        &mut self,
        session: &str,
        procedure: &Procedure,
        index: usize,
        options: ProjectOptions,
        captures: &mut BTreeMap<String, String>,
    ) -> Result<()> {
        let phase = procedure
            .phases
            .get(index)
            .ok_or_else(|| invalid(format!("no phase {index}")))?;
        let mut run = self.phase_run(session, procedure, phase, options, captures)?;
        let mut failed = Vec::new();
        for capture in &phase.capture {
            let one = Phase {
                capture: vec![capture.clone()],
                ..phase.clone()
            };
            if let Err(e) = run.bind_captures(&one) {
                failed.push(e);
            }
        }
        if failed.is_empty() {
            if let Err(e) = run.owner_writes(index, phase) {
                failed.push(e);
            }
        }
        *captures = std::mem::take(&mut run.captures);
        if failed.is_empty() {
            Ok(())
        } else {
            Err(invalid(format!(
                "phase {index} ({}): {}",
                phase.label,
                failed.join("; ")
            )))
        }
    }
    fn phase_run(
        &mut self,
        session: &str,
        procedure: &Procedure,
        phase: &Phase,
        options: ProjectOptions,
        captures: &BTreeMap<String, String>,
    ) -> Result<Run<'_>> {
        let machine = match procedure.machine_of(phase.role.as_deref()) {
            Ok(machine) => machine.to_owned(),
            Err(_) if phase.is_the_worlds() => String::new(),
            Err(e) => return Err(e),
        };
        Ok(Run {
            world: self,
            session: session.to_owned(),
            machine,
            role: phase.role.clone(),
            options,
            steps: Vec::new(),
            counts: Counts::default(),
            captures: captures.clone(),
            returned: Value::Null,
        })
    }
}

/// `item` (a gesture, a tool call) with its `{{@name}}` placeholders filled from `captures`;
/// an error names a capture that is not there.
pub fn bind_captures<T>(item: &T, captures: &BTreeMap<String, String>) -> Result<T>
where
    T: Serialize + serde::de::DeserializeOwned + Clone,
{
    bind(item, captures)
}

/// How long a wait is: the `wait` gesture's fields, and the payload of a tool path's
/// `{"family": "clock", "op": "advance"}` call.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct WaitUnits {
    days: u64,
    hours: u64,
    minutes: u64,
    seconds: u64,
    ticks: u64,
}
impl WaitUnits {
    fn ticks(self) -> Option<u64> {
        const SECOND: u64 = 1_000_000;
        [
            (self.days, 86_400 * SECOND),
            (self.hours, 3_600 * SECOND),
            (self.minutes, 60 * SECOND),
            (self.seconds, SECOND),
            (self.ticks, 1),
        ]
        .iter()
        .try_fold(0u64, |sum, (n, unit)| {
            sum.checked_add(n.checked_mul(*unit)?)
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
    /// Values bound by the phases' captures so far.
    captures: BTreeMap<String, String>,
    /// What the last action of the current phase returned.
    returned: Value,
}
impl Run<'_> {
    /// Perform `phase` (number `index`) through its tool path.
    fn tool_phase(&mut self, index: usize, phase: &Phase) -> std::result::Result<(), String> {
        phase.tool.iter().try_for_each(|call| {
            let call = bind(call, &self.captures).map_err(|e| e.message)?;
            // Time passing has no actor action; the tool path names it the way
            // the screen path does, as the owner's clock.
            if call.family == "clock" && call.op == "advance" {
                let units: WaitUnits = serde_json::from_value(call.payload.clone())
                    .map_err(|e| format!("clock advance: {e}"))?;
                return self.wait(index, &phase.label, units);
            }
            let action = ActionEnvelope::new(
                call.family.clone(),
                call.op.clone(),
                self.machine.clone(),
                call.payload.clone(),
            );
            self.counts.tool_actions += 1;
            self.send(index, &phase.label, action, None, 0)
        })
    }
    /// Bind `phase`'s captures, each read from the world as it is now.
    fn bind_captures(&mut self, phase: &Phase) -> std::result::Result<(), String> {
        phase.capture.iter().try_for_each(|capture| {
            let capture = bind(capture, &self.captures).map_err(|e| e.message)?;
            let value = self
                .capture(&capture)
                .map_err(|e| format!("capture {}: {e}", capture.name))?;
            self.captures.insert(capture.name.clone(), value);
            Ok(())
        })
    }
    /// Make `phase`'s owner writes, each recorded as a step a replay makes again.
    fn owner_writes(&mut self, index: usize, phase: &Phase) -> std::result::Result<(), String> {
        phase.owner.iter().try_for_each(|write| {
            let write = bind(write, &self.captures).map_err(|e| e.message)?;
            // A merge patch that is not an object replaces the whole state it lands on.
            let outcome = if write.patch.is_object() {
                self.world.patch_service_state(&write.service, &write.patch)
            } else {
                Err(invalid(format!(
                    "the owner's patch to {} is not an object",
                    write.service
                )))
            };
            self.counts.owner_writes += 1;
            let error = outcome.err();
            let message = error.as_ref().map(|e| e.to_string());
            let payload = json!({"service": write.service, "patch": write.patch});
            self.steps.push(Step {
                phase: index,
                label: phase.label.clone(),
                role: self.role.clone(),
                session: self.session.clone(),
                action: self.action("owner", "patch_service_state", payload),
                target: None,
                scene: 0,
                success: error.is_none(),
                error,
                advance: 0,
                owner: Some(write),
            });
            message.map_or(Ok(()), Err)
        })
    }
    /// The value `capture` binds, read from the world as it is now.
    fn capture(&self, capture: &Capture) -> std::result::Result<String, String> {
        let candidates: Vec<String> = match &capture.from {
            Source::State(Select {
                pointer,
                matching: None,
                ..
            }) => {
                let state = self.world.inspect();
                let value = state
                    .pointer(pointer)
                    .ok_or_else(|| format!("nothing in the world's state at {pointer}"))?;
                vec![text_of(value)]
            }
            Source::State(Select {
                pointer,
                matching: Some(matching),
                field,
            }) => {
                let state = self.world.inspect();
                select(&state, pointer, matching)
                    .into_iter()
                    .filter_map(|(key, element)| match field {
                        Some(at) => element.pointer(at).map(text_of),
                        None => Some(key),
                    })
                    .collect()
            }
            Source::File { machine, path } => {
                let machine = machine.as_deref().unwrap_or(&self.machine);
                let bytes = self
                    .world
                    .runtime()
                    .read_file(machine, path)
                    .map_err(|e| format!("{machine}:{path}: {e}"))?;
                let text = String::from_utf8_lossy(&bytes).into_owned();
                if capture.pattern.is_some() {
                    text.lines().map(str::to_owned).collect()
                } else {
                    vec![text.trim_end_matches(['\r', '\n']).to_owned()]
                }
            }
            Source::Scene { app } => {
                if capture.pattern.is_none() {
                    return Err("a scene capture needs a pattern".into());
                }
                let scene = self.scene()?;
                scene
                    .nodes
                    .iter()
                    .filter(|node| {
                        app.as_deref().is_none_or(|app| {
                            node.window
                                .and_then(|w| scene.window(w))
                                .is_some_and(|w| w.app == app)
                        })
                    })
                    .filter_map(|node| node.painted_text().map(str::to_owned))
                    .collect()
            }
            Source::Outcome { pointer } => {
                let value = self
                    .returned
                    .pointer(pointer)
                    .ok_or_else(|| format!("the last action returned nothing at {pointer:?}"))?;
                vec![text_of(value)]
            }
        };
        let mut values = candidates.iter().filter_map(|text| match &capture.pattern {
            Some(pattern) => extract(pattern, text),
            None => Some(text.clone()),
        });
        let value = if capture.last {
            values.next_back()
        } else {
            values.next()
        };
        value.ok_or_else(|| match &capture.pattern {
            Some(pattern) => format!("nothing in its source matches {pattern:?}"),
            None => "its source holds nothing that matches".to_owned(),
        })
    }
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
                self.returned = o.value;
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
            advance: 0,
            owner: None,
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
            Gesture::Click { target, at, .. } | Gesture::DoubleClick { target, at } => {
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
                let mut payload =
                    json!({"x": resolved.x, "y": resolved.y, "width": width, "height": height});
                if let Gesture::Click {
                    button: b @ 1.., ..
                } = gesture
                {
                    payload["button"] = json!(b);
                }
                let action = self.action("pointer.v1", op, payload);
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
            Gesture::Scroll {
                target,
                delta_y,
                at,
            } => {
                let scene = self.scene()?;
                let resolved = target.resolve_at(&scene, *at).ok_or_else(|| {
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
            Gesture::Wait {
                days,
                hours,
                minutes,
                seconds,
                ticks,
            } => self.wait(
                phase,
                label,
                WaitUnits {
                    days: *days,
                    hours: *hours,
                    minutes: *minutes,
                    seconds: *seconds,
                    ticks: *ticks,
                },
            ),
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
        let project = judges().ok_or("this build has no CAD engine")?.cad_project;
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
        let (x, y) = project(&state["camera"], p, view.width, view.height)?;
        Ok((view.x + x.round() as i32, view.y + y.round() as i32))
    }
    /// Let world time pass and record it as an `advance` step.
    fn wait(
        &mut self,
        phase: usize,
        label: &str,
        units: WaitUnits,
    ) -> std::result::Result<(), String> {
        let total = units.ticks().ok_or("wait overflows the clock")?;
        let outcome = self.world.advance(total);
        self.counts.waited += total;
        let error = outcome.err();
        let message = error.as_ref().map(|e| e.to_string());
        self.steps.push(Step {
            phase,
            label: label.to_owned(),
            role: self.role.clone(),
            session: self.session.clone(),
            action: self.action("clock", "advance", json!({"ticks": total})),
            target: None,
            scene: 0,
            success: error.is_none(),
            error,
            advance: total,
            owner: None,
        });
        message.map_or(Ok(()), Err)
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
    fn a_wildcard_matches_generated_identifiers_only() {
        let pattern = "(uuid \"{{*}}\")\nG04 *\n";
        assert!(wildcard(pattern, "{{*}}", "(uuid \"9f1c-77\")\nG04 *\n"));
        assert!(!wildcard(pattern, "{{*}}", "(uuid \"9f1c\")\nG04 X*\n"));
        let values = BTreeMap::new();
        assert_eq!(substitute("a {{*}} b", &values).unwrap(), "a {{*}} b");
    }
    #[test]
    fn base64_decodes_binary() {
        assert_eq!(
            crate::decode_base64("AAEC/w==").unwrap(),
            vec![0, 1, 2, 255]
        );
        assert_eq!(crate::decode_base64("aGVsbG8").unwrap(), b"hello");
        assert!(crate::decode_base64("not base64!").is_err());
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
    #[test]
    fn a_typed_placeholder_becomes_a_number() {
        let procedure = Procedure::from_json(
            r#"{"id":"n","instruction":"pay {{amount}}","machine":"m",
                "params":[{"name":"cents","kind":"integer","min":100,"max":100},
                          {"name":"amount","kind":"expr","expr":"cents / 100","decimals":2}],
                "phases":[],"verify":[{"state":{"All":[
                    {"Equals":{"pointer":"/a","value":"{{#amount}}"}},
                    {"Equals":{"pointer":"/c","value":"{{#cents}}"}},
                    {"Equals":{"pointer":"/t","value":"{{amount}} USD"}}]}}]}"#,
        )
        .unwrap();
        let one = procedure.instantiate(1, &BTreeMap::new()).unwrap();
        let state = json!({"a": 1.00, "c": 100, "t": "1.00 USD"});
        let Check::State(p) = &one.procedure.verify[0] else {
            panic!()
        };
        assert!(p.evaluate(&state), "{p:?}");
    }
    #[test]
    fn a_capture_placeholder_waits_for_its_pass() {
        let values = BTreeMap::from([("branch".to_owned(), "fix".to_owned())]);
        let text = "PR {{@pr}} on {{branch}}, {{*}}";
        let drawn = substitute(text, &values).unwrap();
        assert_eq!(drawn, "PR {{@pr}} on fix, {{*}}");
        let captured = BTreeMap::from([("pr".to_owned(), "8".to_owned())]);
        assert_eq!(
            substitute_in(&drawn, &captured, Pass::Captures).unwrap(),
            "PR 8 on fix, {{*}}"
        );
        assert!(substitute_in(&drawn, &BTreeMap::new(), Pass::Captures).is_err());
        let mut number = json!({"n": "{{#@pr}}", "s": "#{{#@pr}}"});
        substitute_all(&mut number, &values, Pass::Params).unwrap();
        assert_eq!(number, json!({"n": "{{#@pr}}", "s": "#{{#@pr}}"}));
        substitute_all(&mut number, &captured, Pass::Captures).unwrap();
        assert_eq!(number, json!({"n": 8, "s": "#8"}));
    }
    #[test]
    fn a_pattern_extracts_one_identifier() {
        assert_eq!(extract("PR #{{?}}", "Open PR #8 now").as_deref(), Some("8"));
        assert_eq!(
            extract("order {{?}},", "shop order 5001, 3 x R1").as_deref(),
            Some("5001")
        );
        assert_eq!(
            extract("Order {{?}}", "Order 5001. Thanks").as_deref(),
            Some("5001")
        );
        assert_eq!(
            extract("read-{{?}}-body", "window:3:content:read-mail-4-body").as_deref(),
            Some("mail-4")
        );
        assert_eq!(
            extract("v{{?}} {{*}}ok", "v1.2.0 build ok").as_deref(),
            Some("1.2.0")
        );
        assert_eq!(extract("PR #{{?}}", "no number"), None);
        assert_eq!(extract("no value", "no value"), None);
    }
    #[test]
    fn a_collection_is_searched_by_content() {
        let state = json!({"messages": {
            "mail-9": {"sender": "nina", "to": ["casey@x"], "body": "Quote: 3 rovers"},
            "mail-10": {"sender": "nina", "to": ["casey@x"], "body": "Quote: 4 rovers"},
            "mail-2": {"sender": "omar", "to": ["lin@y"], "body": "PO", "n": 7}},
            "prs": [{"number": 3, "head": "refs/heads/a"}, {"number": 8, "head": "refs/heads/b"}]});
        let by = |pairs: &[(&str, Value)]| -> BTreeMap<String, Value> {
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), v.clone()))
                .collect()
        };
        let nina = select(
            &state,
            "/messages",
            &by(&[("/sender", json!("nina")), ("/body", json!("Quote: {{*}}"))]),
        );
        let keys: Vec<&str> = nina.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["mail-9", "mail-10"]);
        assert_eq!(
            select(&state, "/messages", &by(&[("/to", json!("lin@y"))])).len(),
            1
        );
        assert_eq!(
            select(&state, "/messages", &by(&[("/n", json!("7"))])).len(),
            1
        );
        let pr = select(&state, "/prs", &by(&[("/head", json!("refs/heads/b"))]));
        assert_eq!(pr[0].0, "1");
        assert_eq!(pr[0].1["number"], 8);
        assert!(select(&state, "/missing", &BTreeMap::new()).is_empty());
        assert!(natural("mail-9", "mail-10").is_lt());
        assert!(natural("999", "1000").is_lt());
    }
    #[test]
    fn a_record_keeps_linked_values_together() {
        let procedure = Procedure::from_json(
            r#"{"id":"r","instruction":"{{engineer}} on {{engineer.machine}}","machine":"m",
                "params":[{"name":"engineer","kind":"record","choices":[
                    {"id":"carol","machine":"carol-ubuntu","home":"/home/carol"},
                    {"id":"bob","machine":"bob-windows","home":"C:/Users/bob"}]}],
                "roles":{"engineer":{"machine":"{{engineer.machine}}","user":"{{engineer}}"}},
                "phases":[],"verify":[]}"#,
        )
        .unwrap();
        for seed in 0..16 {
            let one = procedure.instantiate(seed, &BTreeMap::new()).unwrap();
            let role = &one.procedure.roles["engineer"];
            let home = &one.values["engineer.home"];
            match role.user.as_str() {
                "carol" => assert_eq!(
                    (role.machine.as_str(), home.as_str()),
                    ("carol-ubuntu", "/home/carol")
                ),
                "bob" => assert_eq!(
                    (role.machine.as_str(), home.as_str()),
                    ("bob-windows", "C:/Users/bob")
                ),
                other => panic!("drew {other}"),
            }
        }
        let pinned = BTreeMap::from([("engineer.home".to_owned(), "/srv".to_owned())]);
        assert_eq!(
            procedure.instantiate(1, &pinned).unwrap().values["engineer.home"],
            "/srv"
        );
    }
}
