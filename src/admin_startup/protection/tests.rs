use super::*;
use std::io::Write as _;

mod paths;
mod reference;
mod reparse;
mod task_descriptor;

#[test]
fn unavailable_local_volume_is_rejected_before_opening_a_component() {
    let path = Path::new(r"\\?\Volume{00000000-0000-0000-0000-000000000000}\candidate.exe");
    let error = open(path)
        .err()
        .expect("Unavailable volume must be rejected");
    assert!(
        error.to_string().contains("fixed local volume"),
        "{error:#}"
    );
}

struct Fixture {
    root: PathBuf,
    file: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let mut nonce = [0_u8; 16];
        getrandom::fill(&mut nonce).unwrap();
        let nonce = nonce
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let temp =
            crate::verbatim_path::display_path(&std::env::temp_dir().canonicalize().unwrap());
        let root = temp.join(format!("sgt-startup-protection-{nonce}"));
        std::fs::create_dir(&root).unwrap();
        let file = root.join("candidate.exe");
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&file)
            .unwrap();
        output.write_all(b"fixture executable bytes").unwrap();
        drop(output);
        Self { root, file }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(&self.file) {
            eprintln!(
                "Startup fixture retained at {}: {error}",
                self.file.display()
            );
        }
        for name in ["junction", "target"] {
            let path = self.root.join(name);
            if let Err(error) = std::fs::remove_dir(&path)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                eprintln!("Startup fixture retained at {}: {error}", path.display());
            }
        }
        if let Err(error) = std::fs::remove_dir(&self.root) {
            eprintln!(
                "Startup fixture retained at {}: {error}",
                self.root.display()
            );
        }
    }
}

#[test]
fn an_ordinary_owned_file_is_not_an_elevated_startup_target() {
    let fixture = Fixture::new();
    let file = OpenOptions::new().read(true).open(&fixture.file).unwrap();
    let descriptor = acl::check(&file, acl::Kind::File);
    assert!(
        descriptor.is_err(),
        "Ordinary-user file unexpectedly has trusted ownership and ACLs"
    );
    assert!(open(&fixture.file).is_err());
    assert_eq!(
        std::fs::read(&fixture.file).unwrap(),
        b"fixture executable bytes"
    );
}

#[test]
fn a_readonly_attribute_does_not_make_user_owned_bytes_trusted() {
    let fixture = Fixture::new();
    let original = std::fs::metadata(&fixture.file).unwrap().permissions();
    let mut readonly = original.clone();
    readonly.set_readonly(true);
    std::fs::set_permissions(&fixture.file, readonly).unwrap();
    let result = open(&fixture.file);
    std::fs::set_permissions(&fixture.file, original).unwrap();
    assert!(result.is_err());
    assert_eq!(
        std::fs::read(&fixture.file).unwrap(),
        b"fixture executable bytes"
    );
}

#[test]
fn native_component_guard_blocks_write_and_rename_without_changing_permissions() {
    let fixture = Fixture::new();
    let guard = open_raw(&fixture.file).unwrap();
    assert!(OpenOptions::new().write(true).open(&fixture.file).is_err());
    assert!(std::fs::rename(&fixture.file, fixture.root.join("moved.exe")).is_err());
    drop(guard);
    assert!(OpenOptions::new().write(true).open(&fixture.file).is_ok());
}

#[test]
fn protected_system_file_round_trips_through_its_exact_volume_path() {
    use windows::Win32::System::SystemInformation::GetSystemDirectoryW;
    let mut buffer = vec![0_u16; 32_768];
    let length = unsafe { GetSystemDirectoryW(Some(&mut buffer)) } as usize;
    assert!(length > 0 && length < buffer.len());
    let path = PathBuf::from(String::from_utf16(&buffer[..length]).unwrap()).join("kernel32.dll");
    let target = open(&path).unwrap_or_else(|error| {
        panic!(
            "System fixture failed target protection for {}: {error:#}",
            path.display()
        )
    });
    assert!(matches!(
        Input::parse(target.path()).unwrap().namespace,
        Namespace::Volume
    ));
    let reopened = open(target.path()).unwrap();
    assert!(same_path(target.path(), reopened.path()));
    assert!(!target._ancestors.is_empty());
    assert_eq!(
        final_path(&target._file, VOLUME_NAME_GUID).unwrap(),
        target.path(),
    );
}
