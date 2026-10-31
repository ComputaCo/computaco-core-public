//! Coverage: which (application, screen context, stimulus kind)
//! cells have a reference capture, which have been compared, and which diverge. The
//! sampling priority points the next captures at the cells that buy the most.
use crate::cascade::TrajectoryComparison;
use crate::normalize::{normalize, NormalizeOptions};
use crate::schema::*;
use crate::text::normalize_name;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CellKey {
    pub app: String,
    /// The screen the stimulus was chosen from: the front window's application kind
    /// and normalized title, the title shortened to its first four words.
    pub context: String,
    pub stimulus: String,
}
impl CellKey {
    pub fn to_string_key(&self) -> String {
        format!("{}|{}|{}", self.app, self.context, self.stimulus)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CellStats {
    pub reference_steps: u32,
    pub simulated_steps: u32,
    pub compared: u32,
    pub divergent: u32,
    /// Annotated as mattering.
    pub matters: u32,
    #[serde(default)]
    pub last_reference: String,
    #[serde(default)]
    pub last_compared: String,
    /// Trajectory ids that touched this cell, most recent last, at most eight.
    #[serde(default)]
    pub trajectories: Vec<String>,
    /// Fingerprint classes of the reference captures in this cell.
    #[serde(default)]
    pub classes: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Cell {
    pub key: CellKey,
    pub stats: CellStats,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Coverage {
    pub cells: BTreeMap<String, Cell>,
}

/// The context of a screen: the front window's title, shortened. The application is
/// the trajectory's, not the window's bus name, so both sides land in one cell.
pub fn context_of(o: &Observation) -> String {
    let Some(tree) = &o.semantic else {
        return "?".into();
    };
    let n = normalize(
        tree,
        &NormalizeOptions {
            flatten: true,
            ..NormalizeOptions::default()
        },
    );
    let windows: Vec<&SemNode> = n
        .root
        .children
        .iter()
        .filter(|c| is_window_role(&c.role))
        .collect();
    let Some(front) = windows.last() else {
        return "desktop".into();
    };
    // "name — Schematic Editor", "*name [sheet] — Schematic Editor", "file - gedit":
    // the application's part is after the dash; the document's part changes per run.
    let name = front.name.trim_start_matches('*');
    let name = name
        .rsplit_once(" — ")
        .or_else(|| name.rsplit_once(" - "))
        .map(|(_, after)| after)
        .unwrap_or(name);
    let title: Vec<&str> = name.split_whitespace().take(4).collect();
    let title = normalize_name(&title.join(" "));
    if title.is_empty() {
        front
            .attrs
            .get("app")
            .cloned()
            .unwrap_or_else(|| "window".into())
    } else {
        title
    }
}

impl Coverage {
    fn cell(&mut self, key: CellKey) -> &mut CellStats {
        let k = key.to_string_key();
        &mut self
            .cells
            .entry(k)
            .or_insert_with(|| Cell {
                key,
                stats: CellStats::default(),
            })
            .stats
    }
    /// Count a trajectory's steps. The context of a step is the screen before it.
    pub fn add_trajectory(&mut self, t: &Trajectory) {
        let class = t.fingerprint.class();
        let mut previous = &t.initial;
        for s in &t.steps {
            let key = CellKey {
                app: t.app.clone(),
                context: context_of(previous),
                stimulus: s.stimulus.kind().to_owned(),
            };
            let c = self.cell(key);
            match t.side {
                Side::Reference => {
                    c.reference_steps += 1;
                    if !c.classes.contains(&class) {
                        c.classes.push(class.clone());
                    }
                    if t.created > c.last_reference {
                        c.last_reference = t.created.clone();
                    }
                }
                Side::Simulated => c.simulated_steps += 1,
            }
            if !c.trajectories.contains(&t.id) {
                c.trajectories.push(t.id.clone());
                if c.trajectories.len() > 8 {
                    c.trajectories.remove(0);
                }
            }
            previous = &s.observation;
        }
    }
    /// Count a comparison's steps against the reference trajectory's cells.
    pub fn add_comparison(
        &mut self,
        reference: &Trajectory,
        c: &TrajectoryComparison,
        annotations: &[Annotation],
    ) {
        let mut previous = &reference.initial;
        for s in &reference.steps {
            let key = CellKey {
                app: reference.app.clone(),
                context: context_of(previous),
                stimulus: s.stimulus.kind().to_owned(),
            };
            let cell = self.cell(key);
            if let Some(sc) = c.steps.iter().find(|x| x.step == Some(s.index)) {
                cell.compared += 1;
                if sc.divergent {
                    cell.divergent += 1;
                }
                if c.created > cell.last_compared {
                    cell.last_compared = c.created.clone();
                }
                cell.matters += annotations
                    .iter()
                    .filter(|a| {
                        a.reference == c.reference
                            && a.simulated == c.simulated
                            && a.step == s.index
                            && a.verdict == Verdict::Matters
                    })
                    .count() as u32;
            }
            previous = &s.observation;
        }
    }
    pub fn priorities(&self) -> Vec<Priority> {
        let mut out: Vec<Priority> = self.cells.values().map(Priority::of).collect();
        out.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap()
                .then_with(|| a.key.cmp(&b.key))
        });
        out
    }
    pub fn totals(&self) -> Totals {
        let mut t = Totals::default();
        for c in self.cells.values() {
            t.cells += 1;
            if c.stats.reference_steps > 0 {
                t.with_reference += 1;
            }
            if c.stats.compared > 0 {
                t.compared += 1;
            }
            if c.stats.divergent > 0 {
                t.diverging += 1;
            }
            t.reference_steps += c.stats.reference_steps;
            t.simulated_steps += c.stats.simulated_steps;
        }
        t
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Totals {
    pub cells: u32,
    pub with_reference: u32,
    pub compared: u32,
    pub diverging: u32,
    pub reference_steps: u32,
    pub simulated_steps: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Priority {
    pub key: CellKey,
    pub score: f32,
    pub reason: String,
    pub stats: CellStats,
}
impl Priority {
    pub fn of(c: &Cell) -> Self {
        let s = &c.stats;
        let (score, reason) = if s.reference_steps == 0 {
            (
                3.0 + (s.simulated_steps as f32).ln_1p() * 0.1,
                "no reference capture".to_owned(),
            )
        } else if s.compared == 0 {
            (2.5, "reference never compared".to_owned())
        } else if s.divergent > 0 {
            let rate = s.divergent as f32 / s.compared as f32;
            (
                2.0 + rate,
                format!("{} of {} compared steps diverge", s.divergent, s.compared),
            )
        } else {
            (
                (1.0 - (s.compared as f32 / 20.0).min(1.0)).max(0.05),
                format!("{} compared steps agree", s.compared),
            )
        };
        Self {
            key: c.key.clone(),
            score,
            reason,
            stats: c.stats.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn traj(side: Side, n: usize, title: &str) -> Trajectory {
        let fp = Fingerprint {
            app: "kicad".into(),
            ..Default::default()
        };
        let mut t = Trajectory::new(side, fp);
        t.id = format!("t_{side:?}{n}");
        t.created = "2026-10-30T00:00:00Z".into();
        let mut tree = SemTree::desktop(100, 100);
        let mut w = SemNode::new("window", title, Rect::new(0, 0, 100, 100));
        w.attrs.insert("app".into(), "kicad".into());
        w.children
            .push(SemNode::new("button", "Save", Rect::new(1, 1, 10, 10)));
        tree.root.children.push(w);
        t.initial = Observation {
            semantic: Some(tree),
            ..Default::default()
        };
        for i in 0..n {
            t.steps.push(Step {
                index: i as u32,
                phase: None,
                label: None,
                stimulus: Stimulus::click(1, 1),
                target: None,
                resolved: None,
                ok: true,
                error: None,
                observation: t.initial.clone(),
            });
        }
        t
    }

    #[test]
    fn cells_and_priorities() {
        let mut cov = Coverage::default();
        cov.add_trajectory(&traj(Side::Simulated, 3, "Schematic Editor"));
        cov.add_trajectory(&traj(Side::Reference, 2, "PCB Editor"));
        assert_eq!(cov.cells.len(), 2);
        let p = cov.priorities();
        assert_eq!(p[0].key.context, "Schematic Editor");
        assert!(p[0].reason.contains("no reference"));
        assert!(p[1].reason.contains("never compared"));
        let t = cov.totals();
        assert_eq!(t.with_reference, 1);
    }
}
