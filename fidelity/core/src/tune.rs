//! Fitting tolerances to human verdicts, by statistics alone: for each metric, the
//! cut point that best separates findings annotated `Matters` from the rest (maximum
//! Youden's J = TPR − FPR), with its support, so a threshold from three labels is not
//! mistaken for one from three hundred.
use crate::cascade::TrajectoryComparison;
use crate::schema::{Annotation, Tolerances, Verdict};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Labelled {
    pub metric: String,
    pub value: f32,
    pub matters: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Larger values are worse (ΔE, chamfer, bounds offsets).
    Above,
    /// Smaller values are worse (SSIM).
    Below,
}
pub fn direction_of(metric: &str) -> Direction {
    if metric == "texture.ssim" {
        Direction::Below
    } else {
        Direction::Above
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Threshold {
    pub metric: String,
    pub direction: Direction,
    /// A finding with value past this (in `direction`) matters.
    pub cut: f32,
    pub youden_j: f32,
    pub tpr: f32,
    pub fpr: f32,
    pub positives: u32,
    pub negatives: u32,
}

/// Join annotations to the findings they judge and pull out every metric value.
pub fn samples(comparisons: &[TrajectoryComparison], annotations: &[Annotation]) -> Vec<Labelled> {
    let mut out = Vec::new();
    for a in annotations {
        let Some(c) = comparisons
            .iter()
            .find(|c| c.reference == a.reference && c.simulated == a.simulated)
        else {
            continue;
        };
        let Some(step) = c.steps.iter().find(|s| s.step == Some(a.step)) else {
            continue;
        };
        let matters = a.verdict == Verdict::Matters;
        let findings: Vec<_> = match a.finding {
            Some(id) => step.findings.iter().filter(|f| f.id == id).collect(),
            None => step
                .findings
                .iter()
                .filter(|f| {
                    a.node.as_ref().is_none_or(|n| {
                        f.path_a.as_ref() == Some(n) || f.path_b.as_ref() == Some(n)
                    })
                })
                .collect(),
        };
        for f in findings {
            for (k, v) in &f.metrics {
                out.push(Labelled {
                    metric: k.clone(),
                    value: *v,
                    matters,
                });
            }
        }
    }
    out
}

/// One threshold per metric that has both positive and negative labels.
pub fn tune(samples: &[Labelled]) -> Vec<Threshold> {
    let mut by_metric: BTreeMap<&str, Vec<&Labelled>> = BTreeMap::new();
    for s in samples {
        by_metric.entry(s.metric.as_str()).or_default().push(s);
    }
    let mut out = Vec::new();
    for (metric, list) in by_metric {
        let dir = direction_of(metric);
        let mut vals: Vec<(f32, bool)> = list
            .iter()
            .map(|l| {
                (
                    if dir == Direction::Below {
                        -l.value
                    } else {
                        l.value
                    },
                    l.matters,
                )
            })
            .collect();
        vals.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let pos = vals.iter().filter(|v| v.1).count() as f32;
        let neg = vals.len() as f32 - pos;
        if pos == 0.0 || neg == 0.0 {
            continue;
        }
        // Candidate cuts are midpoints between consecutive distinct values; a sample
        // is predicted "matters" when its value is above the cut.
        let mut best: Option<(f32, f32, f32, f32)> = None; // (j, cut, tpr, fpr)
        for i in 0..vals.len().saturating_sub(1) {
            if vals[i].0 == vals[i + 1].0 {
                continue;
            }
            let cut = (vals[i].0 + vals[i + 1].0) / 2.0;
            let tp = vals[i + 1..].iter().filter(|v| v.1).count() as f32;
            let fp = vals[i + 1..].len() as f32 - tp;
            let (tpr, fpr) = (tp / pos, fp / neg);
            let j = tpr - fpr;
            if best.is_none_or(|b| j > b.0) {
                best = Some((j, cut, tpr, fpr));
            }
        }
        if let Some((j, cut, tpr, fpr)) = best {
            out.push(Threshold {
                metric: metric.to_owned(),
                direction: dir,
                cut: if dir == Direction::Below { -cut } else { cut },
                youden_j: j,
                tpr,
                fpr,
                positives: pos as u32,
                negatives: neg as u32,
            });
        }
    }
    out
}

/// Write the tuned cuts into a tolerance set, for thresholds with enough labels.
pub fn apply(tol: &mut Tolerances, thresholds: &[Threshold], min_support: u32) -> Vec<String> {
    let mut applied = Vec::new();
    for t in thresholds {
        if t.positives < min_support || t.negatives < min_support {
            continue;
        }
        let field = match t.metric.as_str() {
            "bounds.max" => {
                tol.bounds_px = t.cut.round() as i32;
                "bounds_px"
            }
            "bounds.iou_gap" => {
                tol.bounds_rel = t.cut;
                "bounds_rel"
            }
            "color.delta_e_mean" => {
                tol.delta_e_mean = t.cut;
                "delta_e_mean"
            }
            "color.delta_e_p95" => {
                tol.delta_e_p95 = t.cut;
                "delta_e_p95"
            }
            "texture.ssim" => {
                tol.ssim_min = t.cut;
                "ssim_min"
            }
            "shape.chamfer" => {
                tol.chamfer_px = t.cut;
                "chamfer_px"
            }
            _ => continue,
        };
        applied.push(format!(
            "{field} = {:.3} (J {:.2}, {}+/{}−)",
            t.cut, t.youden_j, t.positives, t.negatives
        ));
    }
    applied
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn separates_cleanly() {
        let mut s = Vec::new();
        for v in [1.0, 2.0, 2.5] {
            s.push(Labelled {
                metric: "color.delta_e_mean".into(),
                value: v,
                matters: false,
            });
        }
        for v in [6.0, 7.0, 9.0] {
            s.push(Labelled {
                metric: "color.delta_e_mean".into(),
                value: v,
                matters: true,
            });
        }
        for v in [0.95, 0.97] {
            s.push(Labelled {
                metric: "texture.ssim".into(),
                value: v,
                matters: false,
            });
        }
        for v in [0.6, 0.7] {
            s.push(Labelled {
                metric: "texture.ssim".into(),
                value: v,
                matters: true,
            });
        }
        let t = tune(&s);
        let de = t.iter().find(|t| t.metric == "color.delta_e_mean").unwrap();
        assert!((de.cut - 4.25).abs() < 1e-5);
        assert_eq!(de.youden_j, 1.0);
        let ss = t.iter().find(|t| t.metric == "texture.ssim").unwrap();
        assert_eq!(ss.direction, Direction::Below);
        assert!((ss.cut - 0.825).abs() < 1e-5);
        let mut tol = Tolerances::default();
        let applied = apply(&mut tol, &t, 2);
        assert_eq!(applied.len(), 2);
        assert!((tol.delta_e_mean - 4.25).abs() < 1e-5);
        assert!((tol.ssim_min - 0.825).abs() < 1e-5);
        assert!(apply(&mut tol, &t, 5).is_empty());
    }
}
