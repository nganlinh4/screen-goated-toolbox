use super::*;

#[test]
fn ordinary_unc_registration_preserves_native_paths_without_filesystem_access() {
    for path in [
        r"\\example.invalid\share\app & %!.exe",
        r"\\?\UNC\example.invalid\share\app & %!.exe",
    ] {
        let (mut provider, memory) = provider(Snapshot::default());
        provider.path = PathBuf::from(path);
        normal(&provider, || Ok(())).unwrap();
        let run = memory.snapshot().run.unwrap();
        assert_eq!(run_target(&run).unwrap(), Path::new(path));
        assert!(provider.is_enabled().unwrap());
        assert_eq!(u16::from_le_bytes(run.bytes[..2].try_into().unwrap()), 34);
    }
}

#[test]
fn unc_namespace_equivalence_allows_exact_prior_registration_rollback() {
    let original = Snapshot {
        run: Some(legacy(r"\\example.invalid\share\app.exe")),
        approved: Some(disabled()),
    };
    let (mut provider, memory) = provider(original.clone());
    provider.authorized = PathBuf::from(r"\\?\UNC\example.invalid\share\app.exe");
    assert!(elevated(&provider, || Err(anyhow!("Task preparation rejected"))).is_err());
    assert_eq!(memory.snapshot(), original);
}
