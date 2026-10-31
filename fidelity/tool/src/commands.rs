//! The offline commands over a store: compare, report, coverage, inspect, annotate,
//! tune, index, ls, tree, show.
use crate::pngio::encode_png;
use crate::store::Store;
use crate::util::{err, now_iso, Result};
use cw_fidelity::cascade::{
    compare_trajectories, render_text, CompareConfig, TrajectoryComparison,
};
use cw_fidelity::coverage::Coverage;
use cw_fidelity::normalize::{normalize, NormalizeOptions};
use cw_fidelity::tune::{apply, samples, tune};
use cw_fidelity::*;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub fn compare(
    store: &Store,
    reference: &str,
    simulated: &str,
    cfg: &CompareConfig,
    save: bool,
) -> Result<TrajectoryComparison> {
    let a = store.load_trajectory(reference)?;
    let b = store.load_trajectory(simulated)?;
    if a.side != Side::Reference {
        return err(format!("{} is not a reference trajectory", a.id));
    }
    let load = |f: &FrameRef| store.load_frame(f);
    let mut c = compare_trajectories(&a, &b, &load, cfg);
    c.created = now_iso();
    if save {
        store.save_comparison(&mut c)?;
    }
    Ok(c)
}

pub fn report(
    store: &Store,
    reference: Option<&str>,
    simulated: Option<&str>,
) -> Result<Vec<TrajectoryComparison>> {
    let all = store.comparisons()?;
    let reference = reference.map(|r| store.resolve_id(r)).transpose()?;
    let simulated = simulated.map(|r| store.resolve_id(r)).transpose()?;
    Ok(all
        .into_iter()
        .filter(|c| {
            reference.as_ref().is_none_or(|r| &c.reference == r)
                && simulated.as_ref().is_none_or(|s| &c.simulated == s)
        })
        .collect())
}

pub fn report_text(cs: &[TrajectoryComparison]) -> String {
    cs.iter().map(render_text).collect::<Vec<_>>().join("\n")
}

pub fn coverage(store: &Store) -> Result<Coverage> {
    let mut cov = Coverage::default();
    let trajectories = store.trajectories()?;
    let annotations = store.annotations()?;
    for t in &trajectories {
        cov.add_trajectory(t);
    }
    for c in store.comparisons()? {
        if let Some(r) = trajectories.iter().find(|t| t.id == c.reference) {
            cov.add_comparison(r, &c, &annotations);
        }
    }
    Ok(cov)
}

pub fn coverage_text(cov: &Coverage, limit: usize) -> String {
    let t = cov.totals();
    let mut s = format!("cells {}  with reference {}  compared {}  diverging {}  reference steps {}  simulated steps {}\n", t.cells, t.with_reference, t.compared, t.diverging, t.reference_steps, t.simulated_steps);
    for p in cov.priorities().into_iter().take(limit) {
        s.push_str(&format!(
            "  {:5.2}  {:<10} {:<28} {:<20} {}\n",
            p.score,
            p.key.app,
            p.key.context.chars().take(28).collect::<String>(),
            p.key.stimulus,
            p.reason
        ));
    }
    s
}

/// What `inspect` writes for a person (or Claude) to look at: the normalized trees,
/// the frames, and crops of every finding's rectangles.
#[derive(serde::Serialize)]
pub struct Inspection {
    pub reference: String,
    pub simulated: String,
    pub step: Option<u32>,
    pub out: PathBuf,
    pub files: Vec<String>,
    pub findings: Vec<cw_fidelity::Finding>,
    pub stimulus: Option<String>,
}

fn write_png(path: &Path, img: &Image) -> Result<()> {
    std::fs::write(path, encode_png(img)?).map_err(|e| format!("{}: {e}", path.display()))
}

/// Draw a 2-pixel outline of `r` in `img`.
fn outline(img: &mut Image, r: &Rect, color: [u8; 3]) {
    let Some(r) = r.intersection(&img.rect()) else {
        return;
    };
    for y in r.y..r.bottom() {
        for x in r.x..r.right() {
            let edge = x < r.x + 2 || x >= r.right() - 2 || y < r.y + 2 || y >= r.bottom() - 2;
            if edge {
                let i = ((y as u32 * img.width + x as u32) * 4) as usize;
                img.rgba[i..i + 3].copy_from_slice(&color);
            }
        }
    }
}

pub fn inspect(
    store: &Store,
    reference: &str,
    simulated: &str,
    step: Option<u32>,
    out: &Path,
    cfg: &CompareConfig,
    only_finding: Option<u32>,
) -> Result<Inspection> {
    let a = store.load_trajectory(reference)?;
    let b = store.load_trajectory(simulated)?;
    let oa = a
        .observation_at(step)
        .ok_or("no such step in the reference")?;
    let ob = b
        .observation_at(step)
        .ok_or("no such step in the simulation")?;
    let sa = step.and_then(|i| a.steps.get(i as usize));
    let sb = step.and_then(|i| b.steps.get(i as usize));
    let fa = oa.frame.as_ref().and_then(|f| store.load_frame(f));
    let fb = ob.frame.as_ref().and_then(|f| store.load_frame(f));
    let mut c = cw_fidelity::compare_step(
        step,
        oa,
        ob,
        fa.as_ref(),
        fb.as_ref(),
        sa,
        sb,
        &CompareConfig {
            keep_all_widgets: true,
            ..cfg.clone()
        },
    );
    if let Some(id) = only_finding {
        c.findings.retain(|f| f.id == id);
    }
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut files = Vec::new();
    let mut save = |name: &str, v: &Value| -> Result<()> {
        let p = out.join(name);
        std::fs::write(
            &p,
            serde_json::to_string_pretty(v).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        files.push(name.to_owned());
        Ok(())
    };
    if let Some(t) = &oa.semantic {
        save(
            "reference.tree.json",
            &serde_json::to_value(normalize(t, &cfg.normalize)).unwrap(),
        )?;
        save(
            "reference.tree.raw.json",
            &serde_json::to_value(normalize(t, &NormalizeOptions::faithful())).unwrap(),
        )?;
    }
    if let Some(t) = &ob.semantic {
        save(
            "simulated.tree.json",
            &serde_json::to_value(normalize(t, &cfg.normalize)).unwrap(),
        )?;
        save(
            "simulated.tree.raw.json",
            &serde_json::to_value(normalize(t, &NormalizeOptions::faithful())).unwrap(),
        )?;
    }
    save("comparison.json", &serde_json::to_value(&c).unwrap())?;
    if let (Some(fa), Some(fb)) = (&fa, &fb) {
        let mut ma = fa.clone();
        let mut mb = fb.resize(fa.width, fa.height);
        for f in &c.findings {
            if !f.over {
                continue;
            }
            let color = [255, 40, 40];
            if let Some(r) = &f.rect_a {
                outline(&mut ma, r, color);
                if let Some(crop) = fa.crop(r) {
                    let name = format!("finding-{:02}.reference.png", f.id);
                    write_png(&out.join(&name), &crop)?;
                    files.push(name);
                }
            }
            if let Some(r) = &f.rect_b {
                outline(&mut mb, r, [40, 120, 255]);
                if let Some(crop) = fb.crop(r) {
                    let name = format!("finding-{:02}.simulated.png", f.id);
                    write_png(&out.join(&name), &crop)?;
                    files.push(name);
                }
            }
        }
        write_png(&out.join("reference.png"), fa)?;
        write_png(&out.join("simulated.png"), fb)?;
        write_png(&out.join("reference.marked.png"), &ma)?;
        write_png(&out.join("simulated.marked.png"), &mb)?;
        // Side by side.
        let w = fa.width + mb.width + 8;
        let h = fa.height.max(mb.height);
        let mut side = Image::blank(w, h);
        for p in side.rgba.as_chunks_mut::<4>().0 {
            p.copy_from_slice(&[32, 32, 32, 255]);
        }
        blit(&mut side, &ma, 0, 0);
        blit(&mut side, &mb, fa.width as i32 + 8, 0);
        write_png(&out.join("side-by-side.png"), &side)?;
        files.extend(
            [
                "reference.png",
                "simulated.png",
                "reference.marked.png",
                "simulated.marked.png",
                "side-by-side.png",
            ]
            .map(String::from),
        );
        // Difference heat map on the reference's grid.
        let (blocks, _) = cw_fidelity::pixels::block_diff(fa, fb, cfg.frame_block, 0.0);
        let mut heat = Image::blank(fa.width, fa.height);
        for bl in blocks {
            let v = (bl.score * 4.0).min(255.0) as u8;
            if let Some(r) = bl.rect.intersection(&heat.rect()) {
                for y in r.y..r.bottom() {
                    for x in r.x..r.right() {
                        let i = ((y as u32 * heat.width + x as u32) * 4) as usize;
                        heat.rgba[i..i + 4].copy_from_slice(&[v, 255 - v, 0, 255]);
                    }
                }
            }
        }
        write_png(&out.join("heatmap.png"), &heat)?;
        files.push("heatmap.png".into());
    }
    Ok(Inspection {
        reference: a.id,
        simulated: b.id,
        step,
        out: out.to_path_buf(),
        files,
        stimulus: sa.map(|s| cw_fidelity::cascade::describe(&s.stimulus)),
        findings: c.findings,
    })
}

fn blit(dst: &mut Image, src: &Image, ox: i32, oy: i32) {
    for y in 0..src.height {
        let dy = oy + y as i32;
        if dy < 0 || dy >= dst.height as i32 {
            continue;
        }
        for x in 0..src.width {
            let dx = ox + x as i32;
            if dx < 0 || dx >= dst.width as i32 {
                continue;
            }
            let si = ((y * src.width + x) * 4) as usize;
            let di = ((dy as u32 * dst.width + dx as u32) * 4) as usize;
            dst.rgba[di..di + 4].copy_from_slice(&src.rgba[si..si + 4]);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn annotate(
    store: &Store,
    reference: &str,
    simulated: &str,
    step: u32,
    verdict: Verdict,
    finding: Option<u32>,
    node: Option<NodePath>,
    region: Option<Rect>,
    note: &str,
    author: &str,
) -> Result<Annotation> {
    let reference = store.resolve_id(reference)?;
    let simulated = store.resolve_id(simulated)?;
    let mut a = Annotation {
        id: String::new(),
        reference,
        simulated,
        step,
        finding,
        node,
        region,
        verdict,
        note: note.to_owned(),
        author: author.to_owned(),
        created: now_iso(),
    };
    store.save_annotation(&mut a)?;
    Ok(a)
}

#[derive(serde::Serialize)]
pub struct Tuning {
    pub samples: usize,
    pub thresholds: Vec<cw_fidelity::tune::Threshold>,
    pub applied: Vec<String>,
    pub tolerances: Tolerances,
}
pub fn tune_store(store: &Store, apply_it: bool, min_support: u32) -> Result<Tuning> {
    let comparisons = store.comparisons()?;
    let annotations = store.annotations()?;
    let s = samples(&comparisons, &annotations);
    let thresholds = tune(&s);
    let mut tol = store.tolerances()?;
    let applied = if apply_it {
        let a = apply(&mut tol, &thresholds, min_support);
        store.save_tolerances(&tol)?;
        a
    } else {
        let mut copy = tol.clone();
        apply(&mut copy, &thresholds, min_support)
    };
    Ok(Tuning {
        samples: s.len(),
        thresholds,
        applied,
        tolerances: tol,
    })
}

pub fn ls(store: &Store) -> Result<Value> {
    let index = store.rebuild_index()?;
    Ok(json!(index))
}

pub fn ls_text(store: &Store) -> Result<String> {
    let index = store.rebuild_index()?;
    let mut s = String::new();
    s.push_str(&format!(
        "{:<20} {:<10} {:<12} {:<14} {:<10} {:>5} {:>6} {:<10} {}\n",
        "id", "side", "source", "app", "class", "steps", "frames", "display", "procedure/seed"
    ));
    for t in &index.trajectories {
        s.push_str(&format!(
            "{:<20} {:<10} {:<12} {:<14} {:<10} {:>5} {:>6} {:<10} {}{}\n",
            t.id,
            format!("{:?}", t.side).to_lowercase(),
            t.source,
            t.app,
            &t.class[..10.min(t.class.len())],
            t.steps,
            t.frames,
            t.display,
            t.procedure.as_deref().unwrap_or("-"),
            t.seed.map(|s| format!(" #{s}")).unwrap_or_default()
        ));
    }
    if !index.comparisons.is_empty() {
        s.push_str(&format!(
            "\n{:<20} {:<20} {:<8} {:<9} {:<8} {}\n",
            "reference", "simulated", "overall", "divergent", "first", "class"
        ));
        for c in &index.comparisons {
            let first = cw_fidelity::cascade::At::describe(c.first_divergence);
            s.push_str(&format!(
                "{:<20} {:<20} {:<8.3} {:<9} {:<8} {}\n",
                c.reference,
                c.simulated,
                c.overall,
                format!("{}/{}", c.divergent_steps, c.steps),
                first,
                if c.same_class { "same" } else { "differs" }
            ));
        }
    }
    s.push_str(&format!("\n{} annotations\n", index.annotations));
    Ok(s)
}

/// A normalized tree rendered as indented text, one node per line.
pub fn tree_text(t: &SemTree) -> String {
    fn go(n: &SemNode, depth: usize, path: &mut Vec<u32>, out: &mut String) {
        let indent = "  ".repeat(depth);
        let mut flags = String::new();
        if n.states.focused {
            flags.push_str(" [focused]");
        }
        if n.states.checked == Some(true) {
            flags.push_str(" [checked]");
        }
        if n.states.selected == Some(true) {
            flags.push_str(" [selected]");
        }
        if n.states.expanded == Some(true) {
            flags.push_str(" [expanded]");
        }
        if !n.states.enabled {
            flags.push_str(" [disabled]");
        }
        let p = if path.is_empty() {
            "root".to_owned()
        } else {
            path.iter()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(".")
        };
        out.push_str(&format!(
            "{indent}{p:<10} {:<12} {:?}{} @{},{} {}x{}{}\n",
            n.role,
            n.name,
            n.value
                .as_ref()
                .map(|v| format!(" = {v:?}"))
                .unwrap_or_default(),
            n.bounds.x,
            n.bounds.y,
            n.bounds.w,
            n.bounds.h,
            flags
        ));
        for (i, c) in n.children.iter().enumerate() {
            path.push(i as u32);
            go(c, depth + 1, path, out);
            path.pop();
        }
    }
    let mut out = String::new();
    go(&t.root, 0, &mut Vec::new(), &mut out);
    out
}

pub fn show_tree(
    store: &Store,
    id: &str,
    step: Option<u32>,
    raw: bool,
    opts: &NormalizeOptions,
) -> Result<SemTree> {
    let t = store.load_trajectory(id)?;
    let o = t.observation_at(step).ok_or("no such step")?;
    let tree = o.semantic.as_ref().ok_or("no semantic tree at this step")?;
    Ok(if raw {
        normalize(tree, &NormalizeOptions::faithful())
    } else {
        normalize(tree, opts)
    })
}

/// Export one frame to a PNG path.
pub fn frame(store: &Store, id: &str, step: Option<u32>, out: &Path) -> Result<()> {
    let t = store.load_trajectory(id)?;
    let o = t.observation_at(step).ok_or("no such step")?;
    let f = o.frame.as_ref().ok_or("no frame at this step")?;
    let img = store.load_frame(f).ok_or("frame blob missing")?;
    write_png(out, &img)
}

/// A trajectory's steps, one line each.
pub fn steps_text(t: &Trajectory) -> String {
    let mut s = format!(
        "{} {:?} {} {}x{} {} steps, created {}\n",
        t.id,
        t.side,
        t.app,
        t.fingerprint.display.width,
        t.fingerprint.display.height,
        t.steps.len(),
        t.created
    );
    if let Some(p) = &t.procedure {
        s.push_str(&format!(
            "procedure {} seed {} world {}\n",
            p.id,
            p.seed,
            p.world.as_deref().unwrap_or("-")
        ));
    }
    for st in &t.steps {
        let target = st
            .target
            .as_ref()
            .map(|t| format!(" -> {}", serde_json::to_string(t).unwrap_or_default()))
            .unwrap_or_default();
        let hit = st
            .resolved
            .as_ref()
            .map(|r| format!(" hit {} {:?}", r.role, r.name))
            .unwrap_or_default();
        s.push_str(&format!(
            "{:>4} {} {:<30}{}{}{}\n",
            st.index,
            if st.ok { "ok " } else { "ERR" },
            cw_fidelity::cascade::describe(&st.stimulus),
            target,
            hit,
            st.error
                .as_ref()
                .map(|e| format!("  !! {e}"))
                .unwrap_or_default()
        ));
    }
    s
}
