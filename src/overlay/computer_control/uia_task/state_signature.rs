use super::{UiElement, render::is_clickable};
use sha2::{Digest, Sha256};

fn field(hash: &mut Sha256, text: &str) {
    hash.update((text.len() as u64).to_le_bytes());
    hash.update(text.as_bytes());
}

fn optional(hash: &mut Sha256, text: Option<&str>) {
    hash.update([u8::from(text.is_some())]);
    if let Some(text) = text {
        field(hash, text);
    }
}

pub(super) fn scoped_signature(
    surface: &super::super::controller::world::SurfaceIdentity,
    text: &str,
    observed: bool,
) -> Option<String> {
    if !observed || text.trim().is_empty() {
        return None;
    }
    let mut hash = Sha256::new();
    field(&mut hash, &format!("{surface:?}"));
    field(&mut hash, text);
    Some(format!("{:x}", hash.finalize()))
}

pub(super) fn state_signature(elements: &[UiElement]) -> String {
    let mut nodes = Vec::<[u8; 32]>::new();
    for element in elements.iter().filter(|element| {
        !element.name.trim().is_empty()
            && (element.control_type == "Text" || is_clickable(element.control_type))
    }) {
        let mut hash = Sha256::new();
        field(&mut hash, element.control_type);
        field(&mut hash, &element.name);
        field(&mut hash, &element.automation_id);
        hash.update((element.runtime_id.len() as u64).to_le_bytes());
        for id in &element.runtime_id {
            hash.update(id.to_le_bytes());
        }
        optional(&mut hash, element.value.text.as_deref());
        hash.update([
            u8::from(element.value.incomplete),
            u8::from(element.value.supported),
            u8::from(element.value.writable),
        ]);
        optional(&mut hash, element.state.as_deref());
        hash.update([
            u8::from(element.enabled),
            u8::from(element.required),
            u8::from(element.protected),
        ]);
        for coordinate in [element.left, element.top, element.right, element.bottom] {
            hash.update(coordinate.to_le_bytes());
        }
        nodes.push(hash.finalize().into());
    }
    if nodes.is_empty() {
        return String::new();
    }
    nodes.sort_unstable();
    nodes.dedup();
    let mut hash = Sha256::new();
    for node in nodes {
        hash.update(node);
    }
    format!("{:x}", hash.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn element(name: &str) -> UiElement {
        UiElement {
            name: name.into(),
            automation_id: "id".into(),
            runtime_id: vec![1],
            control_type: "Edit",
            left: 0,
            top: 0,
            right: 10,
            bottom: 10,
            enabled: true,
            protected: false,
            state: None,
            value: Some(String::new()).into(),
            required: false,
        }
    }
    #[test]
    fn large_values_have_fixed_size_signatures_and_tail_changes_are_observed() {
        let mut item = element("editor");
        item.value = Some("x".repeat(20 * 1024 * 1024)).into();
        let first = state_signature(&[item.clone()]);
        item.value.text.as_mut().unwrap().push('y');
        let second = state_signature(&[item]);
        assert_eq!(first.len(), 64);
        assert_eq!(second.len(), 64);
        assert_ne!(first, second);
    }
    #[test]
    fn signatures_are_order_independent_and_distinguish_unknown_from_empty_values() {
        let a = element("a");
        let mut b = element("b");
        assert_eq!(
            state_signature(&[a.clone(), b.clone()]),
            state_signature(&[b.clone(), a.clone(), a.clone()])
        );
        let before = state_signature(&[b.clone()]);
        b.value = None.into();
        assert_ne!(before, state_signature(&[b]));
        assert!(state_signature(&[]).is_empty());
    }
    #[test]
    fn field_boundaries_and_control_identity_do_not_alias() {
        let mut a = element("a|b");
        let mut b = element("a");
        a.value = Some("c".into()).into();
        b.value = Some("b|c".into()).into();
        assert_ne!(state_signature(&[a.clone()]), state_signature(&[b]));
        let before = state_signature(&[a.clone()]);
        a.runtime_id.push(2);
        assert_ne!(before, state_signature(&[a]));
    }
}
