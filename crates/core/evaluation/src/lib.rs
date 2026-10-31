//! Optional evaluation policy. Private task definitions are never part of actor observations.
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Predicate {
    Equals { pointer: String, value: Value },
    Exists { pointer: String },
    All(Vec<Predicate>),
    Any(Vec<Predicate>),
    Not(Box<Predicate>),
}
impl Predicate {
    pub fn evaluate(&self, inspection: &Value) -> bool {
        match self {
            Self::Equals { pointer, value } => inspection
                .pointer(pointer)
                .is_some_and(|found| json_equal(found, value)),
            Self::Exists { pointer } => inspection.pointer(pointer).is_some(),
            Self::All(p) => p.iter().all(|v| v.evaluate(inspection)),
            Self::Any(p) => p.iter().any(|v| v.evaluate(inspection)),
            Self::Not(p) => !p.evaluate(inspection),
        }
    }
}
/// JSON equality where numbers compare by value: `6` equals `6.0`. A JavaScript
/// client cannot tell the two apart (it has one number type), so a check written or
/// relayed through one must not depend on which spelling reached the engine.
pub fn json_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            match (x.as_i64(), y.as_i64(), x.as_u64(), y.as_u64()) {
                (Some(i), Some(j), _, _) => i == j,
                (_, _, Some(i), Some(j)) => i == j,
                _ => x.as_f64() == y.as_f64(),
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| json_equal(a, b))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, a)| y.get(k).is_some_and(|b| json_equal(a, b)))
        }
        _ => a == b,
    }
}
/// Created and retained by the evaluator. Never install task secrets in world metadata.
#[derive(Debug, Clone)]
pub struct Task {
    objective: String,
    predicate: Predicate,
    success_reward: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Evaluation {
    pub success: bool,
    pub reward: f64,
}
impl Task {
    pub fn new(objective: impl Into<String>, predicate: Predicate, success_reward: f64) -> Self {
        Self {
            objective: objective.into(),
            predicate,
            success_reward,
        }
    }
    pub fn objective(&self) -> &str {
        &self.objective
    }
    pub fn evaluate(&self, inspection: &Value) -> Evaluation {
        let success = self.predicate.evaluate(inspection);
        Evaluation {
            success,
            reward: if success { self.success_reward } else { 0.0 },
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numbers_compare_by_value() {
        let state = serde_json::json!({"length": 6.0, "list": [1, 2.5], "big": u64::MAX});
        let equals = |pointer: &str, value: serde_json::Value| {
            Predicate::Equals {
                pointer: pointer.into(),
                value,
            }
            .evaluate(&state)
        };
        assert!(equals("/length", serde_json::json!(6)));
        assert!(equals("/list", serde_json::json!([1.0, 2.5])));
        assert!(!equals("/length", serde_json::json!(6.5)));
        assert!(!equals("/length", serde_json::json!("6")));
        assert!(equals("/big", serde_json::json!(u64::MAX)));
        assert!(!equals("/big", serde_json::json!(u64::MAX - 1)));
    }
    #[test]
    fn predicates_are_read_only() {
        let state = serde_json::json!({"file":"ready","secret":"private"});
        let task = Task::new(
            "finish",
            Predicate::Equals {
                pointer: "/file".into(),
                value: "ready".into(),
            },
            1.0,
        );
        let before = state.clone();
        assert!(task.evaluate(&state).success);
        assert_eq!(state, before);
        assert!(!serde_json::to_string(&task.evaluate(&state))
            .unwrap()
            .contains("private"));
    }
}
