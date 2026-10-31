//! The ontology: what both a real desktop and ComputerWorld can emit about one
//! interaction, so the two can be compared step for step.
//!
//! Everything here is observable from outside the implementation. The reference side
//! reads it from the accessibility bus, the X server and the sandbox file system; the
//! simulated side from `Scene::accessibility`, the renderer and the virtual file
//! system. Nothing names an implementation's internals.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SCHEMA_VERSION: u32 = 1;

/// Axis-aligned rectangle in screen pixels; `w` and `h` are never negative.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord,
)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}
impl Rect {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self {
            x,
            y,
            w: w.max(0),
            h: h.max(0),
        }
    }
    pub fn right(&self) -> i32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }
    pub fn area(&self) -> i64 {
        self.w as i64 * self.h as i64
    }
    pub fn is_empty(&self) -> bool {
        self.w == 0 || self.h == 0
    }
    pub fn center(&self) -> (i32, i32) {
        (self.x + self.w / 2, self.y + self.h / 2)
    }
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.right() && y < self.bottom()
    }
    pub fn intersection(&self, o: &Rect) -> Option<Rect> {
        let x = self.x.max(o.x);
        let y = self.y.max(o.y);
        let r = self.right().min(o.right());
        let b = self.bottom().min(o.bottom());
        (r > x && b > y).then(|| Rect::new(x, y, r - x, b - y))
    }
    pub fn union(&self, o: &Rect) -> Rect {
        if self.is_empty() {
            return *o;
        }
        if o.is_empty() {
            return *self;
        }
        let x = self.x.min(o.x);
        let y = self.y.min(o.y);
        Rect::new(
            x,
            y,
            self.right().max(o.right()) - x,
            self.bottom().max(o.bottom()) - y,
        )
    }
    /// Intersection over union; 0 for disjoint or empty rectangles.
    pub fn iou(&self, o: &Rect) -> f32 {
        let inter = self.intersection(o).map(|r| r.area()).unwrap_or(0);
        let union = self.area() + o.area() - inter;
        if union <= 0 {
            0.0
        } else {
            inter as f32 / union as f32
        }
    }
    pub fn translate(&self, dx: i32, dy: i32) -> Rect {
        Rect::new(self.x + dx, self.y + dy, self.w, self.h)
    }
}

/// Which implementation produced a trajectory.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    /// The real operating system and application, captured through host facilities.
    Reference,
    /// ComputerWorld.
    Simulated,
}

/// Screen the trajectory was captured at.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Display {
    pub width: u32,
    pub height: u32,
    /// Dots per inch the toolkit laid out at; 96 when unknown.
    #[serde(default = "default_dpi")]
    pub dpi: u32,
    /// UI scale factor; 1.0 when unknown.
    #[serde(default = "default_scale")]
    pub scale: f32,
}
fn default_dpi() -> u32 {
    96
}
fn default_scale() -> f32 {
    1.0
}
impl Default for Display {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 800,
            dpi: 96,
            scale: 1.0,
        }
    }
}

/// The environment a capture ran in. Comparisons are only meaningful between a
/// reference and a simulation whose fingerprints fall in the same `class`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Fingerprint {
    /// `linux`, `windows`, `macos`, or the ComputerWorld profile family.
    pub os_family: String,
    #[serde(default)]
    pub os_version: String,
    /// Application id shared by both sides: `kicad`, `freecad`, `libreoffice-calc`, …
    pub app: String,
    #[serde(default)]
    pub app_version: String,
    #[serde(default)]
    pub display: Display,
    /// `gtk3`, `wx`, `qt`, `cw-native`, …
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toolkit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    /// SHA-256 of the sorted list of font families available to the toolkit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fonts: Option<String>,
    /// SHA-256 of the application's configuration files as they were at launch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_hash: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, String>,
}
impl Fingerprint {
    /// Equivalence class for comparison: the fields that change what is on screen,
    /// with the application version reduced to its major and minor components.
    pub fn class(&self) -> String {
        let version = major_minor(&self.app_version);
        let key = format!(
            "{}|{}|{}|{}x{}|{}|{}|{}|{}",
            self.app,
            version,
            self.display.scale,
            self.display.width,
            self.display.height,
            self.theme.as_deref().unwrap_or(""),
            self.locale.as_deref().unwrap_or(""),
            self.config_hash.as_deref().unwrap_or(""),
            self.display.dpi,
        );
        format!("c_{}", &crate::hash::sha256_hex(key.as_bytes())[..16])
    }
}
/// `8.0.4-1ubuntu2` → `8.0`; `9` → `9`; empty stays empty.
pub fn major_minor(version: &str) -> String {
    let core: String = version
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let mut parts = core.split('.').filter(|p| !p.is_empty());
    match (parts.next(), parts.next()) {
        (Some(a), Some(b)) => format!("{a}.{b}"),
        (Some(a), None) => a.to_owned(),
        _ => String::new(),
    }
}

/// What a gesture was aimed at, as a person finds it on screen. Mirrors the
/// procedure grammar in `computerworld::procedure::Target`, plus `at`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Target {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interaction: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    /// Point inside the target in thousandths of its width and height.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<[u32; 2]>,
    /// Title of the owning window, `*` matching any run of characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<String>,
    /// Among the nodes that match, which to take: `smallest` (the default, the most
    /// specific), `largest` (a canvas), or `first` in tree order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pick: Option<String>,
}
impl Target {
    pub fn is_empty(&self) -> bool {
        self.role.is_none()
            && self.label.is_none()
            && self.text.is_none()
            && self.interaction.is_none()
            && self.app.is_none()
            && self.window.is_none()
    }
}

/// A window-manager operation on the active window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowOp {
    Maximize,
    Restore,
    Minimize,
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PointerOp {
    Move,
    Down,
    Up,
    Click,
    DoubleClick,
    Wheel,
}

/// The input the actor gave. Coordinates are in the trajectory's display.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Stimulus {
    Pointer {
        op: PointerOp,
        x: i32,
        y: i32,
        #[serde(default = "default_button")]
        button: u8,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        modifiers: Vec<String>,
        #[serde(default, skip_serializing_if = "is_zero_i64")]
        delta_x: i64,
        #[serde(default, skip_serializing_if = "is_zero_i64")]
        delta_y: i64,
    },
    /// Press at one point, move, release at another.
    Drag {
        x: i32,
        y: i32,
        to_x: i32,
        to_y: i32,
        #[serde(default = "default_button")]
        button: u8,
    },
    /// A window-manager operation on the active window: what a click on the title
    /// bar's buttons does, expressed so both sides can perform it their own way.
    Window { op: WindowOp },
    /// One key chord in the procedure spelling: `Ctrl+s`, `Escape`, `ArrowLeft`.
    Key { key: String },
    /// Text entered as typed; a newline is Enter.
    Type { text: String },
    /// Nothing happens for this long: on the reference side wall-clock
    /// milliseconds, on the simulated side logical ticks (microseconds).
    Wait { ticks: u64 },
}
fn default_button() -> u8 {
    1
}
fn is_zero_i64(v: &i64) -> bool {
    *v == 0
}
impl Stimulus {
    pub fn kind(&self) -> &'static str {
        match self {
            Stimulus::Pointer {
                op: PointerOp::Move,
                ..
            } => "pointer.move",
            Stimulus::Pointer {
                op: PointerOp::Down,
                ..
            } => "pointer.down",
            Stimulus::Pointer {
                op: PointerOp::Up, ..
            } => "pointer.up",
            Stimulus::Pointer {
                op: PointerOp::Click,
                ..
            } => "pointer.click",
            Stimulus::Pointer {
                op: PointerOp::DoubleClick,
                ..
            } => "pointer.double_click",
            Stimulus::Pointer {
                op: PointerOp::Wheel,
                ..
            } => "pointer.wheel",
            Stimulus::Drag { .. } => "pointer.drag",
            Stimulus::Window { .. } => "window",
            Stimulus::Key { .. } => "key",
            Stimulus::Type { .. } => "type",
            Stimulus::Wait { .. } => "wait",
        }
    }
    pub fn click(x: i32, y: i32) -> Self {
        Stimulus::Pointer {
            op: PointerOp::Click,
            x,
            y,
            button: 1,
            modifiers: vec![],
            delta_x: 0,
            delta_y: 0,
        }
    }
}

/// Where a target resolved on this side: the node and the point the pointer went to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resolution {
    pub path: NodePath,
    pub role: String,
    pub name: String,
    pub bounds: Rect,
    pub x: i32,
    pub y: i32,
}

/// Position of a node in a `SemTree`: child indices from the root.
pub type NodePath = Vec<u32>;

/// Accessibility state. `None` means not applicable to the role.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct States {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub focusable: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub focused: bool,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expanded: Option<bool>,
}
fn default_true() -> bool {
    true
}
fn is_false(v: &bool) -> bool {
    !*v
}
impl States {
    pub fn visible_enabled() -> Self {
        Self {
            enabled: true,
            visible: true,
            ..Default::default()
        }
    }
}

/// One node of the semantic tree. Roles use the canonical vocabulary in
/// [`crate::normalize::canonical_role`].
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SemNode {
    pub role: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default)]
    pub states: States,
    #[serde(default)]
    pub bounds: Rect,
    /// Implementation-side identity (an AT-SPI path, a scene interaction id), carried
    /// for inspection only and never compared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    /// Side facts that some roles carry: `app` kind and `document` of a window, …
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attrs: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<SemNode>,
}
impl SemNode {
    pub fn new(role: impl Into<String>, name: impl Into<String>, bounds: Rect) -> Self {
        Self {
            role: role.into(),
            name: name.into(),
            bounds,
            states: States::visible_enabled(),
            ..Default::default()
        }
    }
    pub fn size(&self) -> usize {
        1 + self.children.iter().map(SemNode::size).sum::<usize>()
    }
    pub fn depth(&self) -> usize {
        1 + self.children.iter().map(SemNode::depth).max().unwrap_or(0)
    }
    pub fn get(&self, path: &[u32]) -> Option<&SemNode> {
        let mut node = self;
        for &i in path {
            node = node.children.get(i as usize)?;
        }
        Some(node)
    }
    pub fn get_mut(&mut self, path: &[u32]) -> Option<&mut SemNode> {
        let mut node = self;
        for &i in path {
            node = node.children.get_mut(i as usize)?;
        }
        Some(node)
    }
    /// Every node with its path, in pre-order.
    pub fn walk(&self) -> Vec<(NodePath, &SemNode)> {
        let mut out = Vec::with_capacity(self.size());
        fn go<'a>(n: &'a SemNode, path: &mut NodePath, out: &mut Vec<(NodePath, &'a SemNode)>) {
            out.push((path.clone(), n));
            for (i, c) in n.children.iter().enumerate() {
                path.push(i as u32);
                go(c, path, out);
                path.pop();
            }
        }
        go(self, &mut Vec::new(), &mut out);
        out
    }
    /// Nearest ancestor-or-self window along `path`, as (path, node).
    pub fn window_of<'a>(&'a self, path: &[u32]) -> Option<(NodePath, &'a SemNode)> {
        let mut node = self;
        let mut best = None;
        if is_window_role(&self.role) {
            best = Some((Vec::new(), node));
        }
        for (depth, &i) in path.iter().enumerate() {
            node = node.children.get(i as usize)?;
            if is_window_role(&node.role) {
                best = Some((path[..=depth].to_vec(), node));
            }
        }
        best
    }
}
pub fn is_window_role(role: &str) -> bool {
    matches!(role, "window" | "dialog" | "frame" | "alert" | "tooltip")
}

/// The whole screen's semantics: a root of role `desktop` whose children are the
/// windows, in stacking order bottom to top.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SemTree {
    pub root: SemNode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus: Option<NodePath>,
    /// Which window is in front, as a path from the root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_window: Option<NodePath>,
}
impl SemTree {
    pub fn desktop(width: u32, height: u32) -> Self {
        let mut root = SemNode::new("desktop", "", Rect::new(0, 0, width as i32, height as i32));
        root.states = States::visible_enabled();
        Self {
            root,
            focus: None,
            active_window: None,
        }
    }
    pub fn size(&self) -> usize {
        self.root.size()
    }
}

/// A stored frame: pixels live in the object store by hash.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameRef {
    pub width: u32,
    pub height: u32,
    /// SHA-256 of the raw RGBA bytes, row-major, no padding.
    pub sha256: String,
    /// Object key in the store (`objects/ab/abcdef….png`).
    pub blob: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum FileChangeKind {
    Created,
    Modified,
    Deleted,
}
/// A file the step changed under the sandbox home, path relative to it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub struct FileChange {
    pub path: String,
    pub kind: FileChangeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default)]
    pub size: u64,
}

/// Side effects of a step beyond the screen.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Effects {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<FileChange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clipboard: Option<String>,
    /// Titles of the top-level windows after the step, bottom to top.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub window_titles: Vec<String>,
}

/// How the screen came to rest before it was observed.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Settle {
    /// Frames sampled until the content hash stopped changing.
    pub frames: u32,
    /// Wall-clock (reference) or logical (simulated) time waited, in ms or ticks.
    pub waited: u64,
    /// False when the budget ran out while the screen was still changing.
    pub stable: bool,
}

/// Everything observed after one stimulus.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Observation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic: Option<SemTree>,
    /// Hash of the normalized semantic tree, so a comparison can skip equal steps
    /// without loading either tree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<FrameRef>,
    #[serde(default)]
    pub effects: Effects,
    #[serde(default)]
    pub settle: Settle,
    /// Reference: milliseconds since the trajectory started. Simulated: world tick.
    #[serde(default)]
    pub time: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub index: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub stimulus: Stimulus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<Target>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved: Option<Resolution>,
    /// The stimulus was delivered (a target was found, the action was accepted).
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub observation: Observation,
}

/// How a trajectory was obtained. Gold comes from more than one place: this tool
/// driving the real application in Xvfb, a computer-use session on a real machine, a
/// video, an rrweb recording of a site. Each yields a different subset of the channels,
/// and a comparison runs the levels the data allows.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Source {
    /// `atspi-xvfb`, `computer-use`, `video`, `rrweb`, `simulator`, `import`.
    pub kind: String,
    /// Where it came from: a recording path, a session id, a URL.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub provenance: String,
    /// The program and version that wrote it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tool: String,
}

/// The procedure a trajectory performed, when it came from one.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ProcedureRef {
    pub id: String,
    pub seed: u64,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world: Option<String>,
    /// The procedure file, relative to the repository root when it is in it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Trajectory {
    pub schema_version: u32,
    /// `t_` + 16 hex of the content hash; assigned when stored.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    pub side: Side,
    pub app: String,
    pub fingerprint: Fingerprint,
    #[serde(default)]
    pub source: Source,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub procedure: Option<ProcedureRef>,
    /// ISO 8601 UTC, supplied by the tool.
    #[serde(default)]
    pub created: String,
    /// The screen before any stimulus.
    pub initial: Observation,
    pub steps: Vec<Step>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub notes: BTreeMap<String, String>,
}
impl Trajectory {
    pub fn new(side: Side, fingerprint: Fingerprint) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            id: String::new(),
            side,
            app: fingerprint.app.clone(),
            fingerprint,
            source: Source {
                kind: match side {
                    Side::Reference => "atspi-xvfb".into(),
                    Side::Simulated => "simulator".into(),
                },
                ..Default::default()
            },
            procedure: None,
            created: String::new(),
            initial: Observation::default(),
            steps: Vec::new(),
            notes: BTreeMap::new(),
        }
    }
    /// Observation after step `i`, or the initial one for `None`.
    pub fn observation_at(&self, step: Option<u32>) -> Option<&Observation> {
        match step {
            None => Some(&self.initial),
            Some(i) => self.steps.get(i as usize).map(|s| &s.observation),
        }
    }
    /// Content identity: hash of everything but `id` and `created`.
    pub fn content_id(&self) -> String {
        let mut copy = self.clone();
        copy.id.clear();
        copy.created.clear();
        let bytes = serde_json::to_vec(&copy).expect("trajectory serializes");
        format!("t_{}", &crate::hash::sha256_hex(&bytes)[..16])
    }
}

/// A person's (or a rule's) judgement about one divergence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// A real infidelity worth fixing in the simulation.
    Matters,
    /// Visible but acceptable (antialiasing, a theme's shade).
    Cosmetic,
    /// Not a fidelity question (a clock, a random id).
    Ignore,
    /// The reference capture was wrong (a stray tooltip, a timing artefact).
    RefNoise,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Annotation {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    /// The comparison this judges: reference and simulated trajectory ids.
    pub reference: String,
    pub simulated: String,
    pub step: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finding: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<NodePath>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<Rect>,
    pub verdict: Verdict,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub created: String,
}

/// Thresholds separating a divergence that matters from one that does not. The
/// defaults are deliberately strict; `cwfid tune` fits them to annotations.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tolerances {
    /// Absolute bounds offset or size difference per edge, in pixels.
    pub bounds_px: i32,
    /// Relative bounds difference (1 − IoU) allowed.
    pub bounds_rel: f32,
    /// Mean CIE76 ΔE over a widget crop.
    pub delta_e_mean: f32,
    /// 95th-percentile ΔE over a widget crop.
    pub delta_e_p95: f32,
    /// Minimum SSIM over a widget crop.
    pub ssim_min: f32,
    /// Mean symmetric chamfer distance between edge maps, in pixels.
    pub chamfer_px: f32,
    /// Minimum name similarity (normalized Levenshtein) for a renamed node to count
    /// as the same control.
    pub name_similarity_min: f32,
    /// Maximum tree edit distance relative to the larger tree's size.
    pub tree_distance_rel_max: f32,
    /// Roles never compared (status bars with clocks, tooltips).
    #[serde(default)]
    pub ignore_roles: Vec<String>,
    /// Names never compared; `*` matches any run of characters.
    #[serde(default)]
    pub ignore_names: Vec<String>,
    /// Smallest widget crop (pixels on a side) worth pixel metrics.
    #[serde(default = "default_min_crop")]
    pub min_crop_px: i32,
    /// Flag a window's own position and size. Off by default: where a window lands and
    /// how big it opens is the window manager's, and the children are compared relative
    /// to it anyway.
    #[serde(default)]
    pub window_geometry: bool,
}
fn default_min_crop() -> i32 {
    4
}
impl Default for Tolerances {
    fn default() -> Self {
        Self {
            bounds_px: 2,
            bounds_rel: 0.1,
            delta_e_mean: 3.0,
            delta_e_p95: 10.0,
            ssim_min: 0.9,
            chamfer_px: 1.5,
            name_similarity_min: 0.8,
            tree_distance_rel_max: 0.05,
            ignore_roles: vec!["tooltip".into(), "statusbar".into()],
            ignore_names: Vec::new(),
            min_crop_px: 4,
            window_geometry: false,
        }
    }
}
