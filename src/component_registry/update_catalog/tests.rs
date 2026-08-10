use super::signature::verify_test_key;

const PUBLIC_KEY: &str = "0456b573c6eb8cd3996eb3ca132be504746107b782e511a6b5884d5e3e4d1ca80468b8764005a2957f8f9e1f2798ea703630ee2aaf01051d0cba3d9e63ebca20b0";
const SIGNATURE: &str = "5003f14e2f0fb17a4cf134b4d40d2367ac891787345d7bf7614d7c5062770596a10720f99c936ba5a8d22e0dfb351055418138fc80517b061168a292d51907c6";
const PAYLOAD: &[u8] = b"signed catalog fixture";

#[test]
fn p256_signature_accepts_exact_bytes_and_rejects_tampering() {
    let public_key = decode(PUBLIC_KEY);
    let signature = decode(SIGNATURE);
    verify_test_key(&public_key, PAYLOAD, &signature).unwrap();
    assert!(
        verify_test_key(&public_key, b"signed catalog fixturf", &signature).is_err(),
        "a one-byte payload mutation must invalidate the catalog"
    );
    let mut changed = signature;
    changed[17] ^= 1;
    assert!(verify_test_key(&public_key, PAYLOAD, &changed).is_err());
}

#[test]
fn incompatible_or_malformed_catalogs_fail_closed() {
    let value = serde_json::json!({
        "schemaVersion": 1,
        "sequence": 1,
        "channel": "stable",
        "minHostVersion": "99.0.0",
        "maxHostVersionExclusive": "100.0.0",
        "contracts": [{"name":"fixture-v1","platform":"windows-x64","delivery":{}}],
        "policies": []
    });
    let catalog: super::UpdateCatalog = serde_json::from_value(value).unwrap();
    assert!(super::validate(&catalog).is_err());
}

fn decode(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
