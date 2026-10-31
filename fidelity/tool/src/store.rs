//! The on-disk store, in the layout `cw_fidelity::store` fixes.
use crate::pngio::{decode_png, encode_png};
use crate::util::{err, now_iso, read_json, write_json, Result};
use cw_fidelity::cascade::TrajectoryComparison;
use cw_fidelity::store::*;
use cw_fidelity::{Annotation, FrameRef, Image, Tolerances, Trajectory};
use std::path::{Path, PathBuf};

pub struct Store {
    pub root: PathBuf,
}
impl Store {
    pub fn open(root: &Path) -> Result<Self> {
        for d in [TRAJECTORIES, OBJECTS, COMPARISONS, ANNOTATIONS] {
            std::fs::create_dir_all(root.join(d))
                .map_err(|e| format!("{}: {e}", root.display()))?;
        }
        Ok(Self {
            root: root.to_path_buf(),
        })
    }
    pub fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }
    pub fn put_frame(&self, img: &Image) -> Result<FrameRef> {
        let sha = img.sha256();
        let blob = blob_path(&sha, "png");
        let p = self.path(&blob);
        if !p.exists() {
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(&p, encode_png(img)?).map_err(|e| e.to_string())?;
        }
        Ok(FrameRef {
            width: img.width,
            height: img.height,
            sha256: sha,
            blob,
        })
    }
    pub fn load_frame(&self, f: &FrameRef) -> Option<Image> {
        let bytes = std::fs::read(self.path(&f.blob)).ok()?;
        decode_png(&bytes).ok()
    }
    pub fn save_trajectory(&self, t: &mut Trajectory) -> Result<String> {
        if t.created.is_empty() {
            t.created = now_iso();
        }
        if t.id.is_empty() {
            t.id = t.content_id();
        }
        write_json(&self.path(&trajectory_path(&t.id)), t)?;
        Ok(t.id.clone())
    }
    pub fn load_trajectory(&self, id: &str) -> Result<Trajectory> {
        let id = self.resolve_id(id)?;
        read_json(&self.path(&trajectory_path(&id)))
    }
    /// A full id, or a unique prefix of one.
    pub fn resolve_id(&self, id: &str) -> Result<String> {
        let all = self.trajectory_ids()?;
        if all.iter().any(|x| x == id) {
            return Ok(id.to_owned());
        }
        let matches: Vec<&String> = all.iter().filter(|x| x.starts_with(id)).collect();
        match matches.len() {
            1 => Ok(matches[0].clone()),
            0 => err(format!("no trajectory {id}")),
            _ => err(format!(
                "{id} is ambiguous: {}",
                matches
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        }
    }
    pub fn trajectory_ids(&self) -> Result<Vec<String>> {
        let mut ids: Vec<String> = std::fs::read_dir(self.path(TRAJECTORIES))
            .map_err(|e| e.to_string())?
            .flatten()
            .filter_map(|e| {
                e.path()
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
            })
            .collect();
        ids.sort();
        Ok(ids)
    }
    pub fn trajectories(&self) -> Result<Vec<Trajectory>> {
        self.trajectory_ids()?
            .iter()
            .map(|id| self.load_trajectory(id))
            .collect()
    }
    pub fn save_comparison(&self, c: &mut TrajectoryComparison) -> Result<PathBuf> {
        if c.created.is_empty() {
            c.created = now_iso();
        }
        let p = self.path(&comparison_path(&c.reference, &c.simulated));
        write_json(&p, c)?;
        Ok(p)
    }
    pub fn comparisons(&self) -> Result<Vec<TrajectoryComparison>> {
        let mut out: Vec<TrajectoryComparison> = Vec::new();
        for e in std::fs::read_dir(self.path(COMPARISONS))
            .map_err(|e| e.to_string())?
            .flatten()
        {
            if e.path().extension().is_some_and(|x| x == "json") {
                match read_json(&e.path()) {
                    Ok(v) => out.push(v),
                    Err(err) => eprintln!("cwfid: skipping {err}"),
                }
            }
        }
        out.sort_by(|a, b| a.created.cmp(&b.created));
        Ok(out)
    }
    pub fn save_annotation(&self, a: &mut Annotation) -> Result<String> {
        if a.created.is_empty() {
            a.created = now_iso();
        }
        if a.id.is_empty() {
            a.id = annotation_id(a);
        }
        write_json(&self.path(&annotation_path(&a.id)), a)?;
        Ok(a.id.clone())
    }
    pub fn annotations(&self) -> Result<Vec<Annotation>> {
        let mut out: Vec<Annotation> = Vec::new();
        for e in std::fs::read_dir(self.path(ANNOTATIONS))
            .map_err(|e| e.to_string())?
            .flatten()
        {
            if e.path().extension().is_some_and(|x| x == "json") {
                match read_json(&e.path()) {
                    Ok(v) => out.push(v),
                    Err(err) => eprintln!("cwfid: skipping {err}"),
                }
            }
        }
        out.sort_by(|a, b| a.created.cmp(&b.created));
        Ok(out)
    }
    pub fn tolerances(&self) -> Result<Tolerances> {
        let p = self.path(TOLERANCES);
        if p.exists() {
            read_json(&p)
        } else {
            Ok(Tolerances::default())
        }
    }
    pub fn save_tolerances(&self, t: &Tolerances) -> Result<()> {
        write_json(&self.path(TOLERANCES), t)
    }
    pub fn rebuild_index(&self) -> Result<Index> {
        let mut index = Index {
            schema_version: cw_fidelity::SCHEMA_VERSION,
            built: now_iso(),
            ..Default::default()
        };
        for t in self.trajectories()? {
            index.trajectories.push(TrajectoryEntry::of(&t));
        }
        for c in self.comparisons()? {
            index.comparisons.push(ComparisonEntry {
                reference: c.reference.clone(),
                simulated: c.simulated.clone(),
                app: c.app.clone(),
                same_class: c.same_class,
                first_divergence: c.first_divergence,
                overall: c.fidelity.overall,
                divergent_steps: c.summary.divergent_steps,
                steps: c.summary.steps_compared,
                created: c.created.clone(),
            });
        }
        index.annotations = self.annotations()?.len() as u32;
        write_json(&self.path(INDEX), &index)?;
        Ok(index)
    }
}
