use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::Value;

use super::super::client::McpTool;
use super::capacity::{self, CatalogCapacity, CatalogIssue};
use super::{ConnSnapshot, ConnectionStatus, Manager, ToolRoute, ToolSnapshot, connection_status};

const MAX_SNAPSHOT_ATTEMPTS: usize = 3;

struct ConnectionInput {
    id: String,
    status: ConnectionStatus,
    tools: Arc<Vec<McpTool>>,
}

struct CatalogInput {
    revision: u64,
    connections: Vec<ConnectionInput>,
}

impl CatalogInput {
    fn capture(manager: &Manager) -> Self {
        Self {
            revision: manager.revision,
            connections: manager
                .connected
                .iter()
                .map(|(id, connection)| ConnectionInput {
                    id: id.clone(),
                    status: connection_status(connection),
                    tools: connection.tools.clone(),
                })
                .collect(),
        }
    }

    fn materialize(&self) -> ConnSnapshot {
        let mut snapshot = self
            .connections
            .iter()
            .filter(|connection| connection.status.catalog_valid && connection.status.alive)
            .map(|connection| {
                (
                    connection.id.clone(),
                    connection.status.connection_token,
                    connection
                        .tools
                        .iter()
                        .map(|tool| ToolSnapshot {
                            name: tool.name.clone(),
                            description: tool.description.clone(),
                            input_schema: tool.input_schema.clone(),
                            annotations: tool.annotations,
                        })
                        .collect(),
                )
            })
            .collect::<ConnSnapshot>();
        snapshot.sort_by(|a, b| a.0.cmp(&b.0));
        snapshot
    }

    fn is_current(&self, manager: &Manager) -> bool {
        manager.revision == self.revision
            && manager.connected.len() == self.connections.len()
            && self.connections.iter().all(|input| {
                manager.connected.get(&input.id).is_some_and(|connection| {
                    connection_status(connection) == input.status
                        && Arc::ptr_eq(&connection.tools, &input.tools)
                })
            })
    }
}

#[derive(Debug)]
pub(super) struct CatalogChanged;
impl std::fmt::Display for CatalogChanged {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("MCP catalog changed while preparing its declarations")
    }
}
impl std::error::Error for CatalogChanged {}

pub(super) fn build(
    store: &Mutex<Manager>,
    mut normalize: impl FnMut(&ConnSnapshot) -> anyhow::Result<(Vec<Value>, HashMap<String, ToolRoute>)>,
) -> anyhow::Result<Vec<Value>> {
    let builder = store.lock().catalog_builder.clone();
    let _building = builder.lock();
    for _ in 0..MAX_SNAPSHOT_ATTEMPTS {
        let input = CatalogInput::capture(&store.lock());
        let snapshot = input.materialize();
        let outcome = capacity::check_snapshot(&snapshot).and_then(|()| {
            let (declarations, routes) = normalize(&snapshot)?;
            capacity::check_declarations(&declarations)?;
            Ok((declarations, routes))
        });
        let mut manager = store.lock();
        if input.is_current(&manager) {
            match outcome {
                Ok((declarations, routes)) => {
                    let previous = std::mem::replace(&mut manager.routes, routes);
                    manager.catalog_issue = None;
                    drop(manager);
                    drop(previous);
                    return Ok(declarations);
                }
                Err(error) => {
                    if let Some(capacity) = error.downcast_ref::<CatalogCapacity>() {
                        manager.catalog_issue =
                            Some(CatalogIssue::from_snapshot(&snapshot, capacity));
                        let previous = std::mem::take(&mut manager.routes);
                        drop(manager);
                        drop(previous);
                    }
                    return Err(error);
                }
            }
        }
        // A changed input requires the complete new snapshot, never a filtered
        // subset of the old declarations or a partial route publication.
    }
    Err(CatalogChanged.into())
}
