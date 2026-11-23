//! Host tests normalize MCP catalogs with the engine's own source compiled
//! in-process, then apply the plan through the production host validation, so
//! only the process transport differs from the shipped path.

use super::normalization::{apply_normalized_catalog, catalog_request};
use super::{ConnSnapshot, ToolRoute};
use serde_json::Value;
use std::collections::HashMap;

// A path-attributed inline module keeps the engine's `mcp.rs` a non-mod-rs
// file, so its own `mod schema;` resolves to `src/mcp/schema.rs`.
#[path = "../../../../../native/computer_control_engine/src"]
mod engine_src {
    mod mcp;

    pub(super) fn normalize(
        request: sgt_computer_control_protocol::McpCatalogRequest,
    ) -> anyhow::Result<sgt_computer_control_protocol::McpCatalogPlan> {
        mcp::normalize(request)
    }
}

pub(super) fn direct_declarations(
    snapshot: &ConnSnapshot,
) -> (Vec<Value>, HashMap<String, ToolRoute>) {
    let plan = engine_src::normalize(catalog_request(snapshot))
        .expect("engine normalization accepts the snapshot");
    apply_normalized_catalog(snapshot, plan).expect("host accepts the engine plan")
}
