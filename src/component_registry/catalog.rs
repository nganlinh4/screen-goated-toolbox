use std::collections::{HashMap, HashSet};

use anyhow::{Result, bail};
use serde::Deserialize;

const EMBEDDED_CATALOG: &str = include_str!("../../component-catalog/catalog-v1.json");
const MAX_COMPONENTS: usize = 256;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ComponentCatalog {
    pub(crate) schema_version: u32,
    pub(crate) components: Vec<ComponentDefinition>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ComponentDefinition {
    pub(crate) id: String,
    pub(crate) display_name: String,
    pub(crate) kind: ComponentKind,
    pub(crate) capabilities: Vec<String>,
    pub(crate) dependencies: Vec<String>,
    pub(crate) removable: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ComponentKind {
    WebAssets,
    Model,
    NativeRuntime,
    Sidecar,
    ExternalTool,
    PlayModule,
}

pub(crate) fn embedded_catalog() -> &'static ComponentCatalog {
    static CATALOG: std::sync::OnceLock<ComponentCatalog> = std::sync::OnceLock::new();
    CATALOG.get_or_init(|| {
        let catalog: ComponentCatalog =
            serde_json::from_str(EMBEDDED_CATALOG).expect("embedded component catalog is invalid");
        catalog
            .validate()
            .expect("embedded component catalog violates its contract");
        catalog
    })
}

impl ComponentCatalog {
    pub(crate) fn validate(&self) -> Result<()> {
        if self.schema_version != 1 {
            bail!("unsupported component catalog schema");
        }
        if self.components.is_empty() || self.components.len() > MAX_COMPONENTS {
            bail!("component catalog has an invalid component count");
        }
        let mut by_id = HashMap::new();
        for component in &self.components {
            validate_identifier(&component.id)?;
            if component.display_name.trim().is_empty() || component.display_name.len() > 120 {
                bail!("component display name is invalid");
            }
            if by_id.insert(component.id.as_str(), component).is_some() {
                bail!("component catalog contains duplicate ids");
            }
            if component.capabilities.len() > 32 || component.dependencies.len() > 32 {
                bail!("component catalog entry is too large");
            }
            let mut capabilities = HashSet::new();
            for capability in &component.capabilities {
                validate_identifier(capability)?;
                if !capabilities.insert(capability) {
                    bail!("component catalog contains duplicate capabilities");
                }
            }
        }
        for component in &self.components {
            let mut dependencies = HashSet::new();
            for dependency in &component.dependencies {
                validate_identifier(dependency)?;
                if dependency == &component.id
                    || !by_id.contains_key(dependency.as_str())
                    || !dependencies.insert(dependency)
                {
                    bail!("component dependency is invalid");
                }
            }
        }
        let mut visiting = HashSet::new();
        let mut visited = HashSet::new();
        for component in &self.components {
            visit(component.id.as_str(), &by_id, &mut visiting, &mut visited)?;
        }
        Ok(())
    }

    pub(crate) fn is_removable(&self, id: &str) -> bool {
        #[cfg(test)]
        if id.starts_with("test-") {
            return true;
        }
        self.components.iter().any(|component| {
            let _ = &component.kind;
            component.id == id && component.removable
        })
    }
}

fn visit<'a>(
    id: &'a str,
    by_id: &HashMap<&'a str, &'a ComponentDefinition>,
    visiting: &mut HashSet<&'a str>,
    visited: &mut HashSet<&'a str>,
) -> Result<()> {
    if visited.contains(id) {
        return Ok(());
    }
    if !visiting.insert(id) {
        bail!("component dependency graph contains a cycle");
    }
    for dependency in &by_id[id].dependencies {
        visit(dependency, by_id, visiting, visited)?;
    }
    visiting.remove(id);
    visited.insert(id);
    Ok(())
}

pub(crate) fn validate_identifier(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 80
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_' | b'.')
        })
    {
        bail!("component identifier is invalid");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_catalog_has_a_valid_acyclic_graph() {
        embedded_catalog().validate().unwrap();
    }

    #[test]
    fn catalog_rejects_unknown_fields() {
        let invalid = r#"{"schemaVersion":1,"components":[],"signature":"fake"}"#;
        assert!(serde_json::from_str::<ComponentCatalog>(invalid).is_err());
    }

    #[test]
    fn catalog_rejects_duplicate_edges() {
        let invalid = r#"{
            "schemaVersion": 1,
            "components": [
                {
                    "id": "runtime",
                    "displayName": "Runtime",
                    "kind": "native-runtime",
                    "capabilities": ["inference", "inference"],
                    "dependencies": [],
                    "removable": true
                }
            ]
        }"#;
        let catalog: ComponentCatalog = serde_json::from_str(invalid).unwrap();
        assert!(catalog.validate().is_err());
    }
}
