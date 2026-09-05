use serde::{Deserialize, Serialize};
use serde_json::Value;

pub(super) fn refinement_advertised(result: &Value, available_actions: &[String]) -> bool {
    result
        .get("canRefine")
        .and_then(Value::as_bool)
        .unwrap_or_else(|| {
            result.get("canSegment").and_then(Value::as_bool) == Some(true)
                || !available_actions.is_empty()
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn geometry_state_does_not_override_explicit_refinement_capability() {
        for segmented in [false, true] {
            for allowed in [false, true] {
                assert_eq!(
                    refinement_advertised(
                        &json!({"isSegmented": segmented, "canRefine": allowed}),
                        &["add_materials".to_string()],
                    ),
                    allowed,
                );
            }
        }
    }

    #[test]
    fn legacy_capability_fallback_does_not_invent_actions() {
        assert!(!refinement_advertised(&json!({"isSegmented": true}), &[]));
        assert!(refinement_advertised(&json!({"canSegment": true}), &[]));
        assert!(refinement_advertised(
            &json!({"isSegmented": true}),
            &["add_materials".to_string()],
        ));
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(in crate::overlay::three_d_generator) enum RefinementKind {
    SeparateParts,
    OptimizeMesh,
    AddMaterials,
    GeneratePbr,
    Rig,
    Animate,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::overlay::three_d_generator) struct RefineRequest {
    pub continuation_id: String,
    pub kind: RefinementKind,
    #[serde(default)]
    pub segmentation_level: Option<String>,
    #[serde(default)]
    pub topology: Option<String>,
    #[serde(default)]
    pub face_limit: Option<u32>,
    #[serde(default)]
    pub animation: Option<String>,
}

impl RefineRequest {
    pub(super) fn action(&self) -> &'static str {
        match (self.kind, self.topology.as_deref()) {
            (RefinementKind::SeparateParts, _) => "separate_parts",
            (RefinementKind::OptimizeMesh, Some("quad")) => "optimize_quad",
            (RefinementKind::OptimizeMesh, _) => "optimize_triangle",
            (RefinementKind::AddMaterials, _) => "add_materials",
            (RefinementKind::GeneratePbr, _) => "generate_pbr",
            (RefinementKind::Rig, _) => "rig",
            (RefinementKind::Animate, _) => "animate",
        }
    }

    pub(super) fn suffix(&self) -> &'static str {
        match self.kind {
            RefinementKind::SeparateParts => "parts",
            RefinementKind::OptimizeMesh => "optimized",
            RefinementKind::AddMaterials => "materials",
            RefinementKind::GeneratePbr => "pbr",
            RefinementKind::Rig => "rigged",
            RefinementKind::Animate => "animated",
        }
    }
}
