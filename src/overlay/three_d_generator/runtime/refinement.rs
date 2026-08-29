use serde::{Deserialize, Serialize};

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
