//! The store's layout and index records. Pure: paths and records only, so the same
//! layout can be read by the tool, by a script, or by a person with `ls`.
//!
//! ```text
//! <store>/
//!   trajectories/<id>.json          one Trajectory, semantic trees inline
//!   objects/<aa>/<sha256>.png       frames, content-addressed
//!   comparisons/<ref>__<sim>.json   one TrajectoryComparison
//!   annotations/<id>.json           one Annotation
//!   tolerances.json                 the Tolerances in force (optional)
//!   index.json                      rebuilt by `cwfid index`
//! ```
use crate::schema::*;
use serde::{Deserialize, Serialize};

pub const TRAJECTORIES: &str = "trajectories";
pub const OBJECTS: &str = "objects";
pub const COMPARISONS: &str = "comparisons";
pub const ANNOTATIONS: &str = "annotations";
pub const TOLERANCES: &str = "tolerances.json";
pub const INDEX: &str = "index.json";

pub fn trajectory_path(id: &str) -> String {
    format!("{TRAJECTORIES}/{id}.json")
}
pub fn blob_path(sha256: &str, ext: &str) -> String {
    format!(
        "{OBJECTS}/{}/{sha256}.{ext}",
        &sha256[..2.min(sha256.len())]
    )
}
pub fn comparison_id(reference: &str, simulated: &str) -> String {
    format!("{reference}__{simulated}")
}
pub fn comparison_path(reference: &str, simulated: &str) -> String {
    format!("{COMPARISONS}/{}.json", comparison_id(reference, simulated))
}
pub fn annotation_path(id: &str) -> String {
    format!("{ANNOTATIONS}/{id}.json")
}
pub fn annotation_id(a: &Annotation) -> String {
    let key = format!(
        "{}|{}|{}|{:?}|{:?}|{}|{}",
        a.reference, a.simulated, a.step, a.finding, a.node, a.author, a.created
    );
    format!("a_{}", &crate::hash::sha256_hex(key.as_bytes())[..16])
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrajectoryEntry {
    pub id: String,
    pub side: Side,
    pub app: String,
    pub class: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub procedure: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    pub steps: u32,
    pub frames: u32,
    pub created: String,
    pub display: String,
    #[serde(default)]
    pub source: String,
}
impl TrajectoryEntry {
    pub fn of(t: &Trajectory) -> Self {
        let frames = t.initial.frame.iter().count() as u32
            + t.steps
                .iter()
                .filter(|s| s.observation.frame.is_some())
                .count() as u32;
        Self {
            id: t.id.clone(),
            side: t.side,
            app: t.app.clone(),
            class: t.fingerprint.class(),
            procedure: t.procedure.as_ref().map(|p| p.id.clone()),
            seed: t.procedure.as_ref().map(|p| p.seed),
            steps: t.steps.len() as u32,
            frames,
            created: t.created.clone(),
            display: format!(
                "{}x{}",
                t.fingerprint.display.width, t.fingerprint.display.height
            ),
            source: t.source.kind.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ComparisonEntry {
    pub reference: String,
    pub simulated: String,
    pub app: String,
    pub same_class: bool,
    pub first_divergence: Option<crate::cascade::At>,
    pub overall: f32,
    pub divergent_steps: u32,
    pub steps: u32,
    pub created: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Index {
    pub schema_version: u32,
    pub trajectories: Vec<TrajectoryEntry>,
    pub comparisons: Vec<ComparisonEntry>,
    pub annotations: u32,
    #[serde(default)]
    pub built: String,
}
