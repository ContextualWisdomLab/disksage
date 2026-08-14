//! Rust-owned LLM-as-a-Judge calibration backed by fast-mlsirm.
//!
//! The local model still produces the bounded decision JSON. This module validates a paired
//! model/human label sample before an operator treats that judge as calibrated. It supports both
//! binary and polytomous scales and keeps all agreement arithmetic in fast-mlsirm's Rust core.

use mlsirm_core::agreement::validate_scoring;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SCHEMA_VERSION: u32 = 1;
pub const ENGINE: &str = "fast-mlsirm";
const MAX_CATEGORIES: u32 = 1_000;
const MAX_SAMPLES: usize = 100_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JudgeCalibrationEvidence {
    pub schema_version: u32,
    /// The exact local-model judgment this calibration sample evaluates.
    pub judgment_id: String,
    /// Number of ordered labels: 2 is true/false; values above 2 are polytomous.
    pub categories: u32,
    pub model_labels: Vec<u32>,
    pub human_labels: Vec<u32>,
    /// Optional double-scored human baseline for the degradation gate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human_baseline_a: Option<Vec<u32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human_baseline_b: Option<Vec<u32>>,
    /// Optional subgroup labels for the fairness SMD gate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subgroup: Option<Vec<u32>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JudgeCalibrationGate {
    pub name: String,
    pub value: f64,
    pub threshold: f64,
    pub pass: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JudgeCalibrationResult {
    pub schema_version: u32,
    pub engine: String,
    pub judgment_id: String,
    pub categories: u32,
    pub sample_count: usize,
    pub passed: bool,
    pub gates: Vec<JudgeCalibrationGate>,
    pub exact_agreement: f64,
    pub adjacent_agreement: f64,
}

fn validate_labels(labels: &[u32], categories: u32, name: &str) -> Result<(), String> {
    if labels.is_empty() || labels.len() > MAX_SAMPLES {
        return Err(format!("judge-calibration-{name}-length-invalid"));
    }
    if labels.iter().any(|label| *label >= categories) {
        return Err(format!("judge-calibration-{name}-label-out-of-range"));
    }
    Ok(())
}

fn compact_subgroups(labels: &[u32]) -> Result<Vec<u32>, String> {
    if labels.is_empty() || labels.len() > MAX_SAMPLES {
        return Err("judge-calibration-subgroup-length-invalid".into());
    }
    let mut mapping = BTreeMap::new();
    let mut compacted = Vec::with_capacity(labels.len());
    for label in labels {
        let next = mapping.len() as u32;
        let compact = *mapping.entry(*label).or_insert(next);
        compacted.push(compact);
    }
    Ok(compacted)
}

pub fn validate(evidence: &JudgeCalibrationEvidence) -> Result<JudgeCalibrationResult, String> {
    if evidence.schema_version != SCHEMA_VERSION {
        return Err("judge-calibration-schema-version-unsupported".into());
    }
    if evidence.judgment_id.len() != 64
        || !evidence
            .judgment_id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("judge-calibration-judgment-id-invalid".into());
    }
    if !(2..=MAX_CATEGORIES).contains(&evidence.categories) {
        return Err("judge-calibration-category-count-invalid".into());
    }
    validate_labels(&evidence.model_labels, evidence.categories, "model")?;
    validate_labels(&evidence.human_labels, evidence.categories, "human")?;
    if evidence.model_labels.len() != evidence.human_labels.len() {
        return Err("judge-calibration-paired-length-mismatch".into());
    }

    let human_baseline = match (&evidence.human_baseline_a, &evidence.human_baseline_b) {
        (None, None) => None,
        (Some(a), Some(b)) => {
            validate_labels(a, evidence.categories, "human-baseline-a")?;
            validate_labels(b, evidence.categories, "human-baseline-b")?;
            if a.len() != evidence.model_labels.len() || b.len() != evidence.model_labels.len() {
                return Err("judge-calibration-human-baseline-length-mismatch".into());
            }
            Some((a.as_slice(), b.as_slice()))
        }
        _ => return Err("judge-calibration-human-baseline-incomplete".into()),
    };
    let subgroup = match &evidence.subgroup {
        None => None,
        Some(labels) => {
            if labels.len() != evidence.model_labels.len() {
                return Err("judge-calibration-subgroup-length-mismatch".into());
            }
            Some(compact_subgroups(labels)?.into_boxed_slice())
        }
    };

    let verdict = validate_scoring(
        &evidence.model_labels,
        &evidence.human_labels,
        evidence.categories as usize,
        human_baseline,
        subgroup.as_deref(),
    )
    .map_err(|error| format!("judge-calibration-fast-mlsirm:{error}"))?;

    Ok(result_from_verdict(evidence, verdict))
}

fn result_from_verdict(
    evidence: &JudgeCalibrationEvidence,
    verdict: mlsirm_core::agreement::ValidationVerdict,
) -> JudgeCalibrationResult {
    JudgeCalibrationResult {
        schema_version: SCHEMA_VERSION,
        engine: ENGINE.into(),
        judgment_id: evidence.judgment_id.clone(),
        categories: evidence.categories,
        sample_count: evidence.model_labels.len(),
        passed: verdict.pass,
        gates: verdict
            .gates
            .into_iter()
            .map(|gate| JudgeCalibrationGate {
                name: gate.name.into(),
                value: gate.value,
                threshold: gate.threshold,
                pass: gate.pass,
            })
            .collect(),
        exact_agreement: verdict.exact_agreement,
        adjacent_agreement: verdict.adjacent_agreement,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evidence(categories: u32) -> JudgeCalibrationEvidence {
        JudgeCalibrationEvidence {
            schema_version: SCHEMA_VERSION,
            judgment_id: "a".repeat(64),
            categories,
            model_labels: vec![0, 1, 2, 0, 1, 2],
            human_labels: vec![0, 1, 2, 0, 1, 2],
            human_baseline_a: None,
            human_baseline_b: None,
            subgroup: Some(vec![10, 10, 20, 20, 10, 20]),
        }
    }

    #[test]
    fn validates_polytomous_labels_with_fast_mlsirm() {
        let result = validate(&evidence(3)).unwrap();
        assert_eq!(result.engine, ENGINE);
        assert_eq!(result.categories, 3);
        assert!(result.passed);
        assert_eq!(result.exact_agreement, 1.0);
    }

    #[test]
    fn validates_binary_true_false_labels() {
        let mut value = evidence(2);
        value.model_labels = vec![0, 1, 0, 1, 0, 1];
        value.human_labels = value.model_labels.clone();
        assert!(validate(&value).unwrap().passed);
    }

    #[test]
    fn rejects_mismatched_or_out_of_range_labels() {
        let mut value = evidence(2);
        value.judgment_id = "not-a-judgment-id".into();
        assert_eq!(
            validate(&value).unwrap_err(),
            "judge-calibration-judgment-id-invalid"
        );
        value.judgment_id = "a".repeat(64);
        value.model_labels = vec![0, 1, 0, 1, 0, 1];
        value.human_labels[0] = 2;
        assert_eq!(
            validate(&value).unwrap_err(),
            "judge-calibration-human-label-out-of-range"
        );
        value.human_labels = vec![0];
        assert_eq!(
            validate(&value).unwrap_err(),
            "judge-calibration-paired-length-mismatch"
        );
    }
}
