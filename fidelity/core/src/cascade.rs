//! The divergence cascade: cheapest check first, each level only where the level
//! before found a difference, every finding localized to a node path or a rectangle.
//!
//! 1. Stimulus: did both sides accept the same input and resolve the same target?
//! 2. Hash: are the normalized semantic trees equal in content? in bounds?
//! 3. Tree: edit script between the trees, each op a finding at a path.
//! 4. Widgets: for matched controls with frames on both sides, crop each at its own
//!    bounds and measure colour, texture and shape.
//! 5. Frame: where no tree can say (a canvas, or no tree at all), locate the change by
//!    block difference.
//! 6. Effects: files, clipboard, settle.
use crate::normalize::{normalize, tree_hash_hex, HashFields, NormalizeOptions};
use crate::pixels::{block_diff, chamfer, delta_e, differing_fraction, ssim, Image};
use crate::schema::*;
use crate::treediff::{diff, EditOp};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Stimulus,
    Hash,
    Tree,
    Widgets,
    Frame,
    Effects,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum FindingKind {
    /// The stimuli at this index differ (a replay drifted).
    StimulusMismatch,
    /// One side delivered the stimulus and the other could not.
    StimulusFailed,
    /// The target resolved to a different role or name.
    TargetMismatch,
    WindowMissing,
    WindowExtra,
    NodeMissing,
    NodeExtra,
    NodeMoved,
    /// A field of a matched node differs (name, value, state).
    NodeChanged,
    BoundsOff,
    ColorOff,
    TextureOff,
    ShapeOff,
    /// A region of the frame differs with no node to pin it on.
    FrameRegion,
    /// A matched canvas differs; the region inside it is given.
    CanvasRegion,
    FileEffect,
    ClipboardEffect,
    /// A side was still changing when observed.
    SettleUnstable,
    /// The trajectories have different lengths.
    LengthMismatch,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    pub id: u32,
    pub kind: FindingKind,
    pub level: Level,
    /// Exceeds tolerance: this is a divergence, not a measurement.
    pub over: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_a: Option<NodePath>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_b: Option<NodePath>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Absolute screen rectangles on each side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rect_a: Option<Rect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rect_b: Option<Rect>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metrics: BTreeMap<String, f32>,
    pub detail: String,
}

/// Six numbers in 0..=1, 1 being indistinguishable, and their weighted mean.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct FidelityVector {
    pub semantic: f32,
    pub layout: f32,
    pub color: f32,
    pub shape: f32,
    pub texture: f32,
    pub effects: f32,
    pub overall: f32,
}
impl Default for FidelityVector {
    fn default() -> Self {
        Self::perfect()
    }
}
impl FidelityVector {
    pub fn perfect() -> Self {
        Self {
            semantic: 1.0,
            layout: 1.0,
            color: 1.0,
            shape: 1.0,
            texture: 1.0,
            effects: 1.0,
            overall: 1.0,
        }
    }
    pub fn finish(mut self) -> Self {
        self.overall = (0.35 * self.semantic
            + 0.2 * self.layout
            + 0.1 * self.color
            + 0.15 * self.shape
            + 0.1 * self.texture
            + 0.1 * self.effects)
            .clamp(0.0, 1.0);
        self
    }
    pub fn mean(all: &[FidelityVector]) -> Self {
        if all.is_empty() {
            return Self::perfect();
        }
        let n = all.len() as f32;
        let mut v = Self {
            semantic: 0.0,
            layout: 0.0,
            color: 0.0,
            shape: 0.0,
            texture: 0.0,
            effects: 0.0,
            overall: 0.0,
        };
        for f in all {
            v.semantic += f.semantic / n;
            v.layout += f.layout / n;
            v.color += f.color / n;
            v.shape += f.shape / n;
            v.texture += f.texture / n;
            v.effects += f.effects / n;
        }
        v.finish()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WidgetMetrics {
    pub path_a: NodePath,
    pub path_b: NodePath,
    pub role: String,
    pub name: String,
    pub rect_a: Rect,
    pub rect_b: Rect,
    pub iou: f32,
    pub delta_e_mean: f32,
    pub delta_e_p95: f32,
    pub ssim: f32,
    pub chamfer: f32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TreeSummary {
    pub distance: f32,
    pub relative: f32,
    pub ops: usize,
    pub matched: usize,
    pub size_a: usize,
    pub size_b: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StepComparison {
    /// None for the initial observation, else the step index.
    pub step: Option<u32>,
    pub levels: Vec<Level>,
    pub hash_equal: bool,
    pub bounds_equal: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tree: Option<TreeSummary>,
    pub widgets_compared: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub widgets: Vec<WidgetMetrics>,
    pub findings: Vec<Finding>,
    pub divergent: bool,
    pub fidelity: FidelityVector,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CompareConfig {
    pub tolerances: Tolerances,
    pub normalize: NormalizeOptions,
    /// Run colour, texture and shape metrics on matched widgets.
    pub pixel_metrics: bool,
    /// Keep every widget's metrics in the result, not only the ones over tolerance.
    pub keep_all_widgets: bool,
    /// Block size for whole-frame localization.
    pub frame_block: u32,
    /// Mean absolute RGB difference a block must exceed to count.
    pub frame_block_threshold: f32,
    /// Sobel threshold for edge maps.
    pub edge_threshold: f32,
    /// Compare file contents by hash, not just presence and kind.
    pub strict_file_content: bool,
    /// Stop comparing steps after the first divergence.
    pub stop_at_first: bool,
}
impl Default for CompareConfig {
    fn default() -> Self {
        Self {
            tolerances: Tolerances::default(),
            normalize: NormalizeOptions::default(),
            pixel_metrics: true,
            keep_all_widgets: false,
            frame_block: 16,
            frame_block_threshold: 6.0,
            edge_threshold: 32.0,
            strict_file_content: false,
            stop_at_first: false,
        }
    }
}

struct Ctx {
    findings: Vec<Finding>,
    next: u32,
}
impl Ctx {
    fn add(&mut self, kind: FindingKind, level: Level, over: bool, detail: String) -> &mut Finding {
        self.findings.push(Finding {
            id: self.next,
            kind,
            level,
            over,
            path_a: None,
            path_b: None,
            window: None,
            role: None,
            name: None,
            rect_a: None,
            rect_b: None,
            metrics: BTreeMap::new(),
            detail,
        });
        self.next += 1;
        self.findings.last_mut().unwrap()
    }
}

/// Scale every rectangle of a tree, for a side captured at another display size.
pub fn scale_tree(t: &mut SemTree, sx: f32, sy: f32) {
    fn go(n: &mut SemNode, sx: f32, sy: f32) {
        n.bounds = Rect::new(
            (n.bounds.x as f32 * sx).round() as i32,
            (n.bounds.y as f32 * sy).round() as i32,
            (n.bounds.w as f32 * sx).round() as i32,
            (n.bounds.h as f32 * sy).round() as i32,
        );
        for c in &mut n.children {
            go(c, sx, sy);
        }
    }
    go(&mut t.root, sx, sy);
}

/// Absolute bounds of a node of a normalized (window-relative) tree.
fn absolute(tree: &SemTree, path: &[u32], relative: bool) -> Option<Rect> {
    let node = tree.root.get(path)?;
    if !relative || is_window_role(&node.role) || node.role == "desktop" {
        return Some(node.bounds);
    }
    let (_, w) = tree.root.window_of(path)?;
    Some(node.bounds.translate(w.bounds.x, w.bounds.y))
}

fn ignored(n: &SemNode, tol: &Tolerances) -> bool {
    tol.ignore_roles.iter().any(|r| r == &n.role)
        || tol
            .ignore_names
            .iter()
            .any(|p| crate::text::glob_matches(p, &n.name))
}

/// Compare the observations after one stimulus. `frame_*` are the decoded frames when
/// the caller has them; `step_*` the steps whose observations these are.
#[allow(clippy::too_many_arguments)]
pub fn compare_step(
    index: Option<u32>,
    a: &Observation,
    b: &Observation,
    frame_a: Option<&Image>,
    frame_b: Option<&Image>,
    step_a: Option<&Step>,
    step_b: Option<&Step>,
    cfg: &CompareConfig,
) -> StepComparison {
    let mut ctx = Ctx {
        findings: Vec::new(),
        next: 0,
    };
    let mut out = StepComparison {
        step: index,
        fidelity: FidelityVector::perfect(),
        ..Default::default()
    };
    let tol = &cfg.tolerances;

    // 1. Stimulus.
    if let (Some(sa), Some(sb)) = (step_a, step_b) {
        out.levels.push(Level::Stimulus);
        // A reference delivers a simulator interaction id its own way (a key for a toolbar
        // button); aimed at the same id, the two inputs are the same intent.
        let same_intent = match (&sa.target, &sb.target) {
            (Some(ta), Some(tb)) => ta.interaction.is_some() && ta.interaction == tb.interaction,
            _ => false,
        };
        if !same_intent && !same_stimulus(&sa.stimulus, &sb.stimulus) {
            ctx.add(
                FindingKind::StimulusMismatch,
                Level::Stimulus,
                true,
                format!("{} vs {}", describe(&sa.stimulus), describe(&sb.stimulus)),
            );
        }
        if sa.ok != sb.ok {
            let f = ctx.add(
                FindingKind::StimulusFailed,
                Level::Stimulus,
                true,
                format!(
                    "reference ok={}, simulated ok={}{}",
                    sa.ok,
                    sb.ok,
                    sb.error
                        .as_deref()
                        .map(|e| format!(": {e}"))
                        .unwrap_or_default()
                ),
            );
            f.name = sa
                .target
                .as_ref()
                .and_then(|t| t.label.clone().or(t.text.clone()));
            f.role = sa.target.as_ref().and_then(|t| t.role.clone());
        }
        if let (Some(ra), Some(rb)) = (&sa.resolved, &sb.resolved) {
            let role_ok = crate::normalize::canonical_role(&ra.role)
                == crate::normalize::canonical_role(&rb.role);
            let name_ok = crate::text::similarity(
                &crate::text::normalize_name(&ra.name),
                &crate::text::normalize_name(&rb.name),
            ) >= tol.name_similarity_min;
            if !role_ok || !name_ok {
                let f = ctx.add(
                    FindingKind::TargetMismatch,
                    Level::Stimulus,
                    true,
                    format!(
                        "reference hit {} '{}', simulated hit {} '{}'",
                        ra.role, ra.name, rb.role, rb.name
                    ),
                );
                f.rect_a = Some(ra.bounds);
                f.rect_b = Some(rb.bounds);
                f.role = Some(ra.role.clone());
                f.name = Some(ra.name.clone());
            }
        }
    }

    // Trees, scaled to a's display when the frames differ in size.
    let (sx, sy) = match (a.frame.as_ref(), b.frame.as_ref()) {
        (Some(fa), Some(fb)) if fa.width != fb.width || fa.height != fb.height => (
            fa.width as f32 / fb.width as f32,
            fa.height as f32 / fb.height as f32,
        ),
        _ => (1.0, 1.0),
    };
    let na = a.semantic.as_ref().map(|t| normalize(t, &cfg.normalize));
    let nb = b.semantic.as_ref().map(|t| {
        let mut t = t.clone();
        if sx != 1.0 || sy != 1.0 {
            scale_tree(&mut t, sx, sy);
        }
        normalize(&t, &cfg.normalize)
    });
    let relative = cfg.normalize.window_relative;
    let mut layout_scores: Vec<f32> = Vec::new();
    let mut matched_pairs: Vec<(NodePath, NodePath)> = Vec::new();
    let mut semantic_score = 1.0f32;

    if let (Some(ta), Some(tb)) = (&na, &nb) {
        // 2. Hash.
        out.levels.push(Level::Hash);
        out.hash_equal =
            tree_hash_hex(ta, HashFields::Content) == tree_hash_hex(tb, HashFields::Content);
        out.bounds_equal = out.hash_equal
            && tree_hash_hex(ta, HashFields::Full) == tree_hash_hex(tb, HashFields::Full);
        if out.bounds_equal {
            // Everything matches node for node.
            matched_pairs.extend(ta.root.walk().into_iter().map(|(p, _)| (p.clone(), p)));
            layout_scores.push(1.0);
        } else {
            // 3. Tree.
            out.levels.push(Level::Tree);
            let d = diff(ta, tb, tol);
            out.tree = Some(TreeSummary {
                distance: d.distance,
                relative: d.relative(),
                ops: d.ops.len(),
                matched: d.matched.len(),
                size_a: d.size_a,
                size_b: d.size_b,
            });
            semantic_score = (1.0 - d.relative()).clamp(0.0, 1.0);
            matched_pairs.extend(d.matched.iter().cloned());
            for op in &d.ops {
                match op {
                    EditOp::Delete {
                        path,
                        role,
                        name,
                        size,
                    } => {
                        let node = ta.root.get(path);
                        if node.is_some_and(|n| ignored(n, tol)) {
                            continue;
                        }
                        let kind = if is_window_role(role) {
                            FindingKind::WindowMissing
                        } else {
                            FindingKind::NodeMissing
                        };
                        let f = ctx.add(
                            kind,
                            Level::Tree,
                            true,
                            format!("{role} '{name}' ({size} nodes) in the reference only"),
                        );
                        f.path_a = Some(path.clone());
                        f.role = Some(role.clone());
                        f.name = Some(name.clone());
                        f.rect_a = absolute(ta, path, relative);
                        f.window = ta.root.window_of(path).map(|(_, w)| w.name.clone());
                    }
                    EditOp::Insert {
                        path,
                        role,
                        name,
                        size,
                    } => {
                        let node = tb.root.get(path);
                        if node.is_some_and(|n| ignored(n, tol)) {
                            continue;
                        }
                        let kind = if is_window_role(role) {
                            FindingKind::WindowExtra
                        } else {
                            FindingKind::NodeExtra
                        };
                        let f = ctx.add(
                            kind,
                            Level::Tree,
                            true,
                            format!("{role} '{name}' ({size} nodes) in the simulation only"),
                        );
                        f.path_b = Some(path.clone());
                        f.role = Some(role.clone());
                        f.name = Some(name.clone());
                        f.rect_b = absolute(tb, path, relative);
                        f.window = tb.root.window_of(path).map(|(_, w)| w.name.clone());
                    }
                    EditOp::Move {
                        from,
                        to,
                        role,
                        name,
                    } => {
                        let f = ctx.add(
                            FindingKind::NodeMoved,
                            Level::Tree,
                            true,
                            format!("{role} '{name}' moved"),
                        );
                        f.path_a = Some(from.clone());
                        f.path_b = Some(to.clone());
                        f.role = Some(role.clone());
                        f.name = Some(name.clone());
                        f.rect_a = absolute(ta, from, relative);
                        f.rect_b = absolute(tb, to, relative);
                    }
                    EditOp::Update {
                        path_a,
                        path_b,
                        field,
                        from,
                        to,
                    } => {
                        let (Some(xa), Some(xb)) = (ta.root.get(path_a), tb.root.get(path_b))
                        else {
                            continue;
                        };
                        if ignored(xa, tol) {
                            continue;
                        }
                        if field == "bounds" {
                            let (ra, rb) = (xa.bounds, xb.bounds);
                            let dx = rb.x - ra.x;
                            let dy = rb.y - ra.y;
                            let dw = rb.w - ra.w;
                            let dh = rb.h - ra.h;
                            let max = dx.abs().max(dy.abs()).max(dw.abs()).max(dh.abs());
                            let iou = ra.iou(&rb);
                            let over = max > tol.bounds_px
                                && (1.0 - iou) > tol.bounds_rel
                                && (tol.window_geometry || !is_window_role(&xa.role));
                            let f = ctx.add(
                                FindingKind::BoundsOff,
                                Level::Tree,
                                over,
                                format!(
                                    "{} '{}' at {},{} {}×{} vs {},{} {}×{}",
                                    xa.role,
                                    xa.name,
                                    ra.x,
                                    ra.y,
                                    ra.w,
                                    ra.h,
                                    rb.x,
                                    rb.y,
                                    rb.w,
                                    rb.h
                                ),
                            );
                            f.path_a = Some(path_a.clone());
                            f.path_b = Some(path_b.clone());
                            f.role = Some(xa.role.clone());
                            f.name = Some(xa.name.clone());
                            f.rect_a = absolute(ta, path_a, relative);
                            f.rect_b = absolute(tb, path_b, relative);
                            f.window = ta.root.window_of(path_a).map(|(_, w)| w.name.clone());
                            f.metrics.insert("bounds.dx".into(), dx as f32);
                            f.metrics.insert("bounds.dy".into(), dy as f32);
                            f.metrics.insert("bounds.dw".into(), dw as f32);
                            f.metrics.insert("bounds.dh".into(), dh as f32);
                            f.metrics.insert("bounds.max".into(), max as f32);
                            f.metrics.insert("bounds.iou_gap".into(), 1.0 - iou);
                        } else {
                            let over = field != "focused";
                            let f = ctx.add(
                                FindingKind::NodeChanged,
                                Level::Tree,
                                over,
                                format!("{} '{}': {field} '{from}' vs '{to}'", xa.role, xa.name),
                            );
                            f.path_a = Some(path_a.clone());
                            f.path_b = Some(path_b.clone());
                            f.role = Some(xa.role.clone());
                            f.name = Some(xa.name.clone());
                            f.rect_a = absolute(ta, path_a, relative);
                            f.rect_b = absolute(tb, path_b, relative);
                            f.window = ta.root.window_of(path_a).map(|(_, w)| w.name.clone());
                            f.metrics
                                .insert("field.cost".into(), crate::treediff::field_cost(field));
                        }
                    }
                }
            }
            for (pa, pb) in &matched_pairs {
                if let (Some(xa), Some(xb)) = (ta.root.get(pa), tb.root.get(pb)) {
                    if xa.role != "desktop" {
                        layout_scores.push(xa.bounds.iou(&xb.bounds));
                    }
                }
            }
        }

        // 4. Widgets.
        if cfg.pixel_metrics {
            if let (Some(fa), Some(fb)) = (frame_a, frame_b) {
                out.levels.push(Level::Widgets);
                let fb_scaled;
                let fb: &Image = if fb.width != fa.width || fb.height != fa.height {
                    fb_scaled = fb.resize(fa.width, fa.height);
                    &fb_scaled
                } else {
                    fb
                };
                for (pa, pb) in &matched_pairs {
                    let (Some(xa), Some(_)) = (ta.root.get(pa), tb.root.get(pb)) else {
                        continue;
                    };
                    if xa.role == "desktop" || ignored(xa, tol) {
                        continue;
                    }
                    // Leaves and controls; a window's pixels are its children's and the
                    // frame level's, so it is not measured twice.
                    let leafish = (xa.children.is_empty()
                        || crate::normalize::is_interactive(&xa.role))
                        && !is_window_role(&xa.role);
                    if !leafish {
                        continue;
                    }
                    let (Some(ra), Some(rb)) =
                        (absolute(ta, pa, relative), absolute(tb, pb, relative))
                    else {
                        continue;
                    };
                    if ra.w < tol.min_crop_px
                        || ra.h < tol.min_crop_px
                        || rb.w < tol.min_crop_px
                        || rb.h < tol.min_crop_px
                    {
                        continue;
                    }
                    let (Some(ca), Some(cb)) = (fa.crop(&ra), fb.crop(&rb)) else {
                        continue;
                    };
                    out.widgets_compared += 1;
                    let de = delta_e(&ca, &cb);
                    let s = ssim(&ca, &cb);
                    let ch = chamfer(&ca, &cb, cfg.edge_threshold);
                    let m = WidgetMetrics {
                        path_a: pa.clone(),
                        path_b: pb.clone(),
                        role: xa.role.clone(),
                        name: xa.name.clone(),
                        rect_a: ra,
                        rect_b: rb,
                        iou: ra.iou(&rb),
                        delta_e_mean: de.mean,
                        delta_e_p95: de.p95,
                        ssim: s,
                        chamfer: ch.mean,
                    };
                    let color_over = de.mean > tol.delta_e_mean || de.p95 > tol.delta_e_p95;
                    let texture_over = s < tol.ssim_min;
                    let shape_over = ch.mean > tol.chamfer_px;
                    if xa.role == "canvas" && (color_over || texture_over || shape_over) {
                        let (blocks, region) =
                            block_diff(&ca, &cb, cfg.frame_block, cfg.frame_block_threshold);
                        let f = ctx.add(
                            FindingKind::CanvasRegion,
                            Level::Widgets,
                            true,
                            format!("canvas '{}' differs in {} blocks", xa.name, blocks.len()),
                        );
                        f.path_a = Some(pa.clone());
                        f.path_b = Some(pb.clone());
                        f.role = Some(xa.role.clone());
                        f.name = Some(xa.name.clone());
                        f.rect_a = region.map(|r| r.translate(ra.x, ra.y));
                        f.rect_b = region.map(|r| r.translate(rb.x, rb.y));
                        f.metrics.insert("color.delta_e_mean".into(), de.mean);
                        f.metrics.insert("texture.ssim".into(), s);
                        f.metrics.insert("shape.chamfer".into(), ch.mean);
                        f.metrics.insert("frame.blocks".into(), blocks.len() as f32);
                    } else {
                        let note =
                            |kind: FindingKind, over: bool, detail: String, ctx: &mut Ctx| {
                                let f = ctx.add(kind, Level::Widgets, over, detail);
                                f.path_a = Some(pa.clone());
                                f.path_b = Some(pb.clone());
                                f.role = Some(xa.role.clone());
                                f.name = Some(xa.name.clone());
                                f.rect_a = Some(ra);
                                f.rect_b = Some(rb);
                                f.window = ta.root.window_of(pa).map(|(_, w)| w.name.clone());
                                f.metrics.insert("color.delta_e_mean".into(), de.mean);
                                f.metrics.insert("color.delta_e_p95".into(), de.p95);
                                f.metrics.insert("texture.ssim".into(), s);
                                f.metrics.insert("shape.chamfer".into(), ch.mean);
                            };
                        if color_over {
                            note(
                                FindingKind::ColorOff,
                                true,
                                format!(
                                    "{} '{}': ΔE mean {:.1}, p95 {:.1}",
                                    xa.role, xa.name, de.mean, de.p95
                                ),
                                &mut ctx,
                            );
                        }
                        if texture_over {
                            note(
                                FindingKind::TextureOff,
                                true,
                                format!("{} '{}': SSIM {:.3}", xa.role, xa.name, s),
                                &mut ctx,
                            );
                        }
                        if shape_over {
                            note(
                                FindingKind::ShapeOff,
                                true,
                                format!("{} '{}': chamfer {:.2} px", xa.role, xa.name, ch.mean),
                                &mut ctx,
                            );
                        }
                    }
                    if cfg.keep_all_widgets || color_over || texture_over || shape_over {
                        out.widgets.push(m);
                    }
                }
            }
        }
    }

    // 5. Frame: when no semantic tree exists on a side, or the trees agree but the
    // pixels may not (a canvas drawn with no accessible node).
    let mut frame_score: Option<(f32, f32, f32)> = None; // (delta_e mean, ssim, differing fraction)
    if let (Some(fa), Some(fb)) = (frame_a, frame_b) {
        out.levels.push(Level::Frame);
        let de = delta_e(fa, fb);
        let s = ssim(fa, fb);
        let frac = differing_fraction(fa, fb);
        frame_score = Some((de.mean, s, frac));
        let no_tree = na.is_none() || nb.is_none();
        if no_tree || (out.bounds_equal && frac > 0.0) {
            let (blocks, region) = block_diff(fa, fb, cfg.frame_block, cfg.frame_block_threshold);
            if let Some(region) = region {
                let over = no_tree && (de.mean > tol.delta_e_mean || s < tol.ssim_min);
                let f = ctx.add(FindingKind::FrameRegion, Level::Frame, over, format!("{} blocks differ in {}×{} at {},{}; ΔE {:.1}, SSIM {:.3}, {:.1}% of pixels", blocks.len(), region.w, region.h, region.x, region.y, de.mean, s, frac * 100.0));
                f.rect_a = Some(region);
                f.rect_b = Some(region);
                f.metrics.insert("color.delta_e_mean".into(), de.mean);
                f.metrics.insert("texture.ssim".into(), s);
                f.metrics.insert("frame.differing".into(), frac);
                f.metrics.insert("frame.blocks".into(), blocks.len() as f32);
            }
        }
    }

    // 6. Effects.
    out.levels.push(Level::Effects);
    let mut effect_hits = 0usize;
    let mut effect_total = 0usize;
    {
        let fa: BTreeMap<&str, &FileChange> = a
            .effects
            .files
            .iter()
            .map(|f| (f.path.as_str(), f))
            .collect();
        let fb: BTreeMap<&str, &FileChange> = b
            .effects
            .files
            .iter()
            .map(|f| (f.path.as_str(), f))
            .collect();
        for (path, x) in &fa {
            effect_total += 1;
            match fb.get(path) {
                None => {
                    ctx.add(
                        FindingKind::FileEffect,
                        Level::Effects,
                        true,
                        format!("{path} {:?} in the reference only", x.kind),
                    );
                }
                Some(y) if y.kind != x.kind => {
                    ctx.add(
                        FindingKind::FileEffect,
                        Level::Effects,
                        true,
                        format!("{path}: {:?} vs {:?}", x.kind, y.kind),
                    );
                }
                Some(y) if cfg.strict_file_content && x.sha256 != y.sha256 => {
                    ctx.add(
                        FindingKind::FileEffect,
                        Level::Effects,
                        true,
                        format!("{path}: contents differ"),
                    );
                }
                Some(y) => {
                    if x.sha256 != y.sha256 && x.sha256.is_some() && y.sha256.is_some() {
                        ctx.add(
                            FindingKind::FileEffect,
                            Level::Effects,
                            false,
                            format!("{path}: contents differ ({} vs {} bytes)", x.size, y.size),
                        );
                    }
                    effect_hits += 1;
                }
            }
        }
        for path in fb.keys() {
            if !fa.contains_key(path) {
                effect_total += 1;
                ctx.add(
                    FindingKind::FileEffect,
                    Level::Effects,
                    true,
                    format!("{path} in the simulation only"),
                );
            }
        }
        if a.effects.clipboard != b.effects.clipboard {
            effect_total += 1;
            ctx.add(
                FindingKind::ClipboardEffect,
                Level::Effects,
                true,
                format!(
                    "clipboard '{}' vs '{}'",
                    a.effects.clipboard.as_deref().unwrap_or(""),
                    b.effects.clipboard.as_deref().unwrap_or("")
                ),
            );
        }
        if !a.settle.stable || !b.settle.stable {
            ctx.add(
                FindingKind::SettleUnstable,
                Level::Effects,
                false,
                format!(
                    "reference stable={}, simulated stable={}",
                    a.settle.stable, b.settle.stable
                ),
            );
        }
    }

    // Fidelity vector.
    let mut fv = FidelityVector::perfect();
    fv.semantic = semantic_score;
    fv.layout = if layout_scores.is_empty() {
        1.0
    } else {
        layout_scores.iter().sum::<f32>() / layout_scores.len() as f32
    };
    if !out.widgets.is_empty() && out.widgets_compared > 0 && cfg.keep_all_widgets {
        let n = out.widgets.len() as f32;
        fv.color = (1.0 - out.widgets.iter().map(|w| w.delta_e_mean).sum::<f32>() / n / 25.0)
            .clamp(0.0, 1.0);
        fv.shape =
            (1.0 - out.widgets.iter().map(|w| w.chamfer).sum::<f32>() / n / 5.0).clamp(0.0, 1.0);
        fv.texture = (out.widgets.iter().map(|w| w.ssim).sum::<f32>() / n).clamp(0.0, 1.0);
    } else if let Some((de, s, _)) = frame_score {
        fv.color = (1.0 - de / 25.0).clamp(0.0, 1.0);
        fv.texture = s.clamp(0.0, 1.0);
        fv.shape = fv.texture;
    }
    fv.effects = if effect_total == 0 {
        1.0
    } else {
        effect_hits as f32 / effect_total as f32
    };
    out.fidelity = fv.finish();
    out.divergent = ctx.findings.iter().any(|f| f.over);
    out.findings = ctx.findings;
    out
}

fn same_stimulus(a: &Stimulus, b: &Stimulus) -> bool {
    match (a, b) {
        (
            Stimulus::Pointer {
                op: oa,
                button: ba,
                modifiers: ma,
                delta_y: dya,
                ..
            },
            Stimulus::Pointer {
                op: ob,
                button: bb,
                modifiers: mb,
                delta_y: dyb,
                ..
            },
        ) => oa == ob && ba == bb && ma == mb && dya.signum() == dyb.signum(),
        (Stimulus::Drag { button: ba, .. }, Stimulus::Drag { button: bb, .. }) => ba == bb,
        (Stimulus::Window { op: a }, Stimulus::Window { op: b }) => a == b,
        (Stimulus::Key { key: ka }, Stimulus::Key { key: kb }) => ka.eq_ignore_ascii_case(kb),
        (Stimulus::Type { text: ta }, Stimulus::Type { text: tb }) => ta == tb,
        (Stimulus::Wait { .. }, Stimulus::Wait { .. }) => true,
        _ => false,
    }
}
pub fn describe(s: &Stimulus) -> String {
    match s {
        Stimulus::Pointer {
            op,
            x,
            y,
            button,
            delta_y,
            ..
        } => match op {
            PointerOp::Wheel => format!("wheel {delta_y} at {x},{y}"),
            _ => format!("{} b{button} at {x},{y}", s.kind()),
        },
        Stimulus::Drag {
            x, y, to_x, to_y, ..
        } => format!("drag {x},{y} to {to_x},{to_y}"),
        Stimulus::Window { op } => format!("window {op:?}").to_lowercase(),
        Stimulus::Key { key } => format!("key {key}"),
        Stimulus::Type { text } => format!("type {:?}", text.chars().take(24).collect::<String>()),
        Stimulus::Wait { ticks } => format!("wait {ticks}"),
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ComparisonSummary {
    pub steps_compared: u32,
    pub divergent_steps: u32,
    pub findings: u32,
    pub findings_over: u32,
    pub by_kind: BTreeMap<String, u32>,
    /// Up to twelve of the worst findings as (step, kind, detail).
    pub worst: Vec<(Option<u32>, String, String)>,
}

/// Where a comparison first diverged. (`Option<Option<u32>>` would not survive JSON.)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum At {
    Initial,
    Step(u32),
}
impl At {
    pub fn of(step: Option<u32>) -> Self {
        step.map_or(At::Initial, At::Step)
    }
    pub fn describe(at: Option<At>) -> String {
        match at {
            None => "none".to_owned(),
            Some(At::Initial) => "initial screen".to_owned(),
            Some(At::Step(i)) => format!("step {i}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrajectoryComparison {
    pub reference: String,
    pub simulated: String,
    pub app: String,
    /// Fingerprint classes agree; a comparison across classes is still produced but
    /// says so.
    pub same_class: bool,
    pub class_a: String,
    pub class_b: String,
    pub config: CompareConfig,
    pub steps: Vec<StepComparison>,
    pub first_divergence: Option<At>,
    pub fidelity: FidelityVector,
    pub summary: ComparisonSummary,
    #[serde(default)]
    pub created: String,
}

/// Compare two trajectories step for step. `frames` loads a stored frame.
pub fn compare_trajectories(
    a: &Trajectory,
    b: &Trajectory,
    frames: &dyn Fn(&FrameRef) -> Option<Image>,
    cfg: &CompareConfig,
) -> TrajectoryComparison {
    let mut steps = Vec::new();
    let load = |o: &Observation| o.frame.as_ref().and_then(frames);
    let fa = load(&a.initial);
    let fb = load(&b.initial);
    let initial = compare_step(
        None,
        &a.initial,
        &b.initial,
        fa.as_ref(),
        fb.as_ref(),
        None,
        None,
        cfg,
    );
    let mut stop = cfg.stop_at_first && initial.divergent;
    steps.push(initial);
    let n = a.steps.len().min(b.steps.len());
    for i in 0..n {
        if stop {
            break;
        }
        let (sa, sb) = (&a.steps[i], &b.steps[i]);
        let fa = load(&sa.observation);
        let fb = load(&sb.observation);
        let c = compare_step(
            Some(i as u32),
            &sa.observation,
            &sb.observation,
            fa.as_ref(),
            fb.as_ref(),
            Some(sa),
            Some(sb),
            cfg,
        );
        stop = cfg.stop_at_first && c.divergent;
        steps.push(c);
    }
    if a.steps.len() != b.steps.len() {
        let mut c = StepComparison {
            step: Some(n as u32),
            levels: vec![Level::Stimulus],
            divergent: true,
            fidelity: FidelityVector {
                semantic: 0.0,
                layout: 0.0,
                color: 0.0,
                shape: 0.0,
                texture: 0.0,
                effects: 0.0,
                overall: 0.0,
            },
            ..Default::default()
        };
        c.findings.push(Finding {
            id: 0,
            kind: FindingKind::LengthMismatch,
            level: Level::Stimulus,
            over: true,
            path_a: None,
            path_b: None,
            window: None,
            role: None,
            name: None,
            rect_a: None,
            rect_b: None,
            metrics: BTreeMap::new(),
            detail: format!(
                "reference has {} steps, simulated {}",
                a.steps.len(),
                b.steps.len()
            ),
        });
        steps.push(c);
    }
    let first_divergence = steps.iter().find(|s| s.divergent).map(|s| At::of(s.step));
    let fidelity = FidelityVector::mean(&steps.iter().map(|s| s.fidelity).collect::<Vec<_>>());
    let mut summary = ComparisonSummary {
        steps_compared: steps.len() as u32,
        ..Default::default()
    };
    let mut worst: Vec<(f32, Option<u32>, String, String)> = Vec::new();
    for s in &steps {
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
    TrajectoryComparison {
        reference: a.id.clone(),
        simulated: b.id.clone(),
        app: a.app.clone(),
        same_class: a.fingerprint.class() == b.fingerprint.class(),
        class_a: a.fingerprint.class(),
        class_b: b.fingerprint.class(),
        config: cfg.clone(),
        steps,
        first_divergence,
        fidelity,
        summary,
        created: String::new(),
    }
}

/// A rank for sorting findings, structural ones first.
pub fn severity(f: &Finding) -> f32 {
    let base = match f.kind {
        FindingKind::StimulusMismatch | FindingKind::LengthMismatch => 10.0,
        FindingKind::StimulusFailed | FindingKind::TargetMismatch => 9.0,
        FindingKind::WindowMissing | FindingKind::WindowExtra => 8.0,
        FindingKind::NodeMissing | FindingKind::NodeExtra => 6.0,
        FindingKind::NodeChanged | FindingKind::FileEffect | FindingKind::ClipboardEffect => 5.0,
        FindingKind::NodeMoved => 4.0,
        FindingKind::CanvasRegion => 3.5,
        FindingKind::BoundsOff => 3.0,
        FindingKind::ShapeOff => 2.5,
        FindingKind::ColorOff | FindingKind::TextureOff => 2.0,
        FindingKind::FrameRegion => 1.5,
        FindingKind::SettleUnstable => 0.5,
    };
    let magnitude = f.metrics.get("bounds.max").copied().unwrap_or(0.0) / 100.0
        + f.metrics.get("color.delta_e_mean").copied().unwrap_or(0.0) / 100.0;
    base + magnitude.min(0.9)
}

/// A terse human rendering for a terminal.
pub fn render_text(c: &TrajectoryComparison) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "{} vs {} ({}){}\n",
        c.reference,
        c.simulated,
        c.app,
        if c.same_class {
            ""
        } else {
            "  [different fingerprint classes]"
        }
    ));
    let f = &c.fidelity;
    s.push_str(&format!(
        "fidelity overall {:.3}  semantic {:.3}  layout {:.3}  color {:.3}  shape {:.3}  texture {:.3}  effects {:.3}\n",
        f.overall, f.semantic, f.layout, f.color, f.shape, f.texture, f.effects
    ));
    s.push_str(&format!(
        "steps {}  divergent {}  findings {} ({} over tolerance)  first divergence: {}\n",
        c.summary.steps_compared,
        c.summary.divergent_steps,
        c.summary.findings,
        c.summary.findings_over,
        At::describe(c.first_divergence)
    ));
    if !c.summary.by_kind.is_empty() {
        let kinds: Vec<String> = c
            .summary
            .by_kind
            .iter()
            .map(|(k, v)| format!("{k} {v}"))
            .collect();
        s.push_str(&format!("by kind: {}\n", kinds.join(", ")));
    }
    for (step, kind, detail) in &c.summary.worst {
        let at = match step {
            None => "initial".to_owned(),
            Some(i) => format!("step {i}"),
        };
        s.push_str(&format!("  {at:>10}  {kind:<16} {detail}\n"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(role: &str, name: &str, r: Rect) -> SemNode {
        SemNode::new(role, name, r)
    }
    fn screen(buttons: &[(&str, Rect)]) -> SemTree {
        let mut t = SemTree::desktop(200, 100);
        let mut w = SemNode::new("window", "App", Rect::new(0, 0, 200, 100));
        for (n, r) in buttons {
            w.children.push(leaf("button", n, *r));
        }
        t.root.children.push(w);
        t
    }
    fn obs(t: SemTree) -> Observation {
        Observation {
            semantic: Some(t),
            settle: Settle {
                frames: 1,
                waited: 0,
                stable: true,
            },
            ..Default::default()
        }
    }

    #[test]
    fn equal_screens_do_not_diverge() {
        let a = obs(screen(&[("Save", Rect::new(10, 10, 40, 20))]));
        let c = compare_step(
            None,
            &a,
            &a,
            None,
            None,
            None,
            None,
            &CompareConfig::default(),
        );
        assert!(!c.divergent);
        assert!(c.hash_equal && c.bounds_equal);
        assert_eq!(c.fidelity.overall, 1.0);
        assert!(c.findings.is_empty());
    }

    #[test]
    fn missing_button_is_localized() {
        let a = obs(screen(&[
            ("Save", Rect::new(10, 10, 40, 20)),
            ("Open", Rect::new(60, 10, 40, 20)),
        ]));
        let b = obs(screen(&[("Save", Rect::new(10, 10, 40, 20))]));
        let c = compare_step(
            Some(3),
            &a,
            &b,
            None,
            None,
            None,
            None,
            &CompareConfig::default(),
        );
        assert!(c.divergent);
        let f = c
            .findings
            .iter()
            .find(|f| f.kind == FindingKind::NodeMissing)
            .expect("missing node finding");
        assert_eq!(f.path_a, Some(vec![0, 1]));
        assert_eq!(f.name.as_deref(), Some("Open"));
        assert_eq!(f.rect_a, Some(Rect::new(60, 10, 40, 20)));
        assert_eq!(f.window.as_deref(), Some("App"));
        assert!(c.fidelity.semantic < 1.0);
    }

    #[test]
    fn one_pixel_shift_is_within_tolerance() {
        let a = obs(screen(&[("Save", Rect::new(10, 10, 40, 20))]));
        let b = obs(screen(&[("Save", Rect::new(11, 10, 40, 20))]));
        let c = compare_step(
            None,
            &a,
            &b,
            None,
            None,
            None,
            None,
            &CompareConfig::default(),
        );
        assert!(!c.divergent, "{:?}", c.findings);
        assert_eq!(c.findings.len(), 1);
        assert_eq!(c.findings[0].kind, FindingKind::BoundsOff);
        assert!(!c.findings[0].over);
    }

    #[test]
    fn large_shift_is_over_tolerance() {
        let a = obs(screen(&[("Save", Rect::new(10, 10, 40, 20))]));
        let b = obs(screen(&[("Save", Rect::new(60, 10, 40, 20))]));
        let c = compare_step(
            None,
            &a,
            &b,
            None,
            None,
            None,
            None,
            &CompareConfig::default(),
        );
        assert!(c.divergent);
        assert_eq!(c.findings[0].metrics["bounds.dx"], 50.0);
    }

    #[test]
    fn widgets_measured_when_frames_present() {
        let a = obs(screen(&[("Save", Rect::new(10, 10, 40, 20))]));
        let b = a.clone();
        let mut fa = Image::blank(200, 100);
        for p in fa.rgba.as_chunks_mut::<4>().0 {
            p.copy_from_slice(&[240, 240, 240, 255]);
        }
        let mut fb = fa.clone();
        // The simulated button is a different shade.
        for y in 10..30 {
            for x in 10..50 {
                let i = ((y * 200 + x) * 4) as usize;
                fa.rgba[i..i + 3].copy_from_slice(&[30, 60, 200]);
                fb.rgba[i..i + 3].copy_from_slice(&[30, 160, 200]);
            }
        }
        let c = compare_step(
            None,
            &a,
            &b,
            Some(&fa),
            Some(&fb),
            None,
            None,
            &CompareConfig::default(),
        );
        assert!(c.hash_equal);
        assert!(c.widgets_compared >= 1);
        let f = c
            .findings
            .iter()
            .find(|f| f.kind == FindingKind::ColorOff)
            .expect("colour finding");
        assert_eq!(f.rect_a, Some(Rect::new(10, 10, 40, 20)));
        assert!(f.metrics["color.delta_e_mean"] > 3.0);
        assert!(c.divergent);
    }

    #[test]
    fn frame_only_comparison_localizes() {
        let a = Observation {
            frame: Some(FrameRef {
                width: 64,
                height: 64,
                sha256: String::new(),
                blob: String::new(),
            }),
            ..Default::default()
        };
        let b = a.clone();
        let fa = Image::blank(64, 64);
        let mut fb = fa.clone();
        for y in 32..48 {
            for x in 32..48 {
                let i = ((y * 64 + x) * 4) as usize;
                fb.rgba[i..i + 3].copy_from_slice(&[255, 255, 255]);
            }
        }
        let c = compare_step(
            None,
            &a,
            &b,
            Some(&fa),
            Some(&fb),
            None,
            None,
            &CompareConfig::default(),
        );
        let f = c
            .findings
            .iter()
            .find(|f| f.kind == FindingKind::FrameRegion)
            .expect("region");
        let r = f.rect_a.unwrap();
        assert_eq!(r, Rect::new(32, 32, 16, 16));
    }

    #[test]
    fn trajectory_reports_first_divergence_and_length() {
        let fp = Fingerprint {
            app: "x".into(),
            ..Default::default()
        };
        let mut a = Trajectory::new(Side::Reference, fp.clone());
        let mut b = Trajectory::new(Side::Simulated, fp);
        a.initial = obs(screen(&[("Save", Rect::new(10, 10, 40, 20))]));
        b.initial = a.initial.clone();
        let step = |i: u32, o: Observation| Step {
            index: i,
            phase: None,
            label: None,
            stimulus: Stimulus::click(1, 1),
            target: None,
            resolved: None,
            ok: true,
            error: None,
            observation: o,
        };
        a.steps.push(step(0, a.initial.clone()));
        b.steps.push(step(0, a.initial.clone()));
        a.steps.push(step(
            1,
            obs(screen(&[
                ("Save", Rect::new(10, 10, 40, 20)),
                ("Quit", Rect::new(60, 10, 40, 20)),
            ])),
        ));
        b.steps.push(step(1, a.initial.clone()));
        a.steps.push(step(2, a.initial.clone()));
        let c = compare_trajectories(&a, &b, &|_| None, &CompareConfig::default());
        assert_eq!(c.first_divergence, Some(At::Step(1)));
        assert!(c
            .steps
            .last()
            .unwrap()
            .findings
            .iter()
            .any(|f| f.kind == FindingKind::LengthMismatch));
        assert_eq!(c.summary.divergent_steps, 2);
        let text = render_text(&c);
        assert!(text.contains("first divergence: step 1"));
    }
}
