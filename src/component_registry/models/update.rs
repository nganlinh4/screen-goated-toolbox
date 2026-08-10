use std::sync::{LazyLock, Mutex};

use super::DeliveryCatalog;

static CACHE: LazyLock<Mutex<Option<(u64, &'static DeliveryCatalog)>>> =
    LazyLock::new(|| Mutex::new(None));

pub(super) fn catalog() -> Option<&'static DeliveryCatalog> {
    let (sequence, value) = super::super::update_catalog::contract("windows-models-v1")?;
    let mut cache = CACHE.lock().unwrap_or_else(|value| value.into_inner());
    if let Some((cached_sequence, catalog)) = *cache
        && cached_sequence == sequence
    {
        return Some(catalog);
    }
    let Ok(catalog) = serde_json::from_value::<DeliveryCatalog>(value) else {
        return None;
    };
    if catalog.validate().is_err() {
        return None;
    }
    let catalog = Box::leak(Box::new(catalog));
    *cache = Some((sequence, catalog));
    Some(catalog)
}

#[test]
fn tracked_contract_parses() {
    let catalog: DeliveryCatalog =
        serde_json::from_str(include_str!("../../../model-delivery/windows-v1.json")).unwrap();
    catalog.validate().unwrap();
}
