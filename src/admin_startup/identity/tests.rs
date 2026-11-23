use super::*;

#[test]
fn ordinary_unc_identity_is_distinct_from_local_execution_admission() {
    for path in [
        r"\\example.invalid\share\app.exe",
        r"\\?\UNC\example.invalid\share\app.exe",
    ] {
        assert!(absolute_startup_path(Path::new(path)));
        assert!(!local_absolute(Path::new(path)));
    }
    assert!(same_path(
        Path::new(r"\\example.invalid\share\app.exe"),
        Path::new(r"\\?\UNC\example.invalid\share\APP.exe")
    ));
    assert!(same_path(
        Path::new(r"C:\Folder\app.exe"),
        Path::new(r"\\?\C:\Folder\APP.exe")
    ));
}

#[test]
fn prefix_equivalence_does_not_rewrite_ambiguous_or_different_paths() {
    for (ordinary, verbatim) in [
        (r"C:\dir.\app.exe", r"\\?\C:\dir.\app.exe"),
        (r"C:\dir \app.exe", r"\\?\C:\dir \app.exe"),
        (r"C:\dir\..\app.exe", r"\\?\C:\dir\..\app.exe"),
        (r"C:\dir\.\app.exe", r"\\?\C:\dir\.\app.exe"),
        (r"C:/dir/app.exe", r"\\?\C:/dir/app.exe"),
        (
            r"\\example.invalid\share\dir.\app.exe",
            r"\\?\UNC\example.invalid\share\dir.\app.exe",
        ),
        (
            r"\\example.invalid\share\app.exe",
            r"\\?\UNC\alias.invalid\share\app.exe",
        ),
    ] {
        assert!(
            !same_path(Path::new(ordinary), Path::new(verbatim)),
            "{ordinary} and {verbatim}"
        );
    }
    let exact = Path::new(r"\\?\UNC\example.invalid\share\dir.\app.exe");
    assert!(same_path(exact, exact));
}

#[test]
fn device_and_incomplete_namespaces_are_never_startup_file_identities() {
    for path in [
        r"\\.\pipe\endpoint",
        r"\\?\GLOBALROOT\Device\disk\app.exe",
        r"C:app.exe",
        r"\app.exe",
        "app.exe",
    ] {
        assert!(!absolute_startup_path(Path::new(path)), "{path}");
    }
}
