use super::super::instance_security::{ObjectSecurity, current_user_sid};
use super::*;
use std::sync::{Arc, Mutex};

fn unique(label: &str) -> String {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("sgt-{label}-test-{}-{nonce}", std::process::id())
}

#[test]
fn requests_round_trip_exact_paths_and_reject_malformed_input() {
    assert_eq!(decode_request(&encode_request(None).unwrap()), Some(None));
    let mut wide = r"C:\입력 & % ^ (x)\".encode_utf16().collect::<Vec<_>>();
    wide.push(0xd800);
    let path = PathBuf::from(std::ffi::OsString::from_wide(&wide));
    let encoded = encode_request(Some(&path)).unwrap();
    assert_eq!(decode_request(&encoded), Some(Some(path)));

    let mut odd = encoded.clone();
    odd.push(b'x');
    assert_eq!(decode_request(&odd), None);
    let mut embedded_nul = encoded;
    embedded_nul.extend([0, 0]);
    assert_eq!(decode_request(&embedded_nul), None);
    let mut restore_with_body = encode_request(None).unwrap();
    restore_with_body.push(b'x');
    assert_eq!(decode_request(&restore_with_body), None);
    let relative = encode_request(Some(Path::new("relative.txt"))).unwrap();
    assert_eq!(decode_request(&relative), None);
    assert_eq!(decode_request(b"SGT-ACTIVATION-0\x00"), None);
    assert_eq!(decode_request(&[]), None);
    assert!(encode_request(Some(Path::new("C:\\a\0b"))).is_err());
}

#[test]
fn the_owner_acknowledges_only_admitted_requests() {
    let sid = current_user_sid().unwrap();
    let security = ObjectSecurity::for_pipe(&sid).unwrap();
    let pipe = format!(r"\\.\pipe\{}", unique("activation"));
    let received = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&received);
    start_server(&pipe, &security, move |request| {
        sink.lock().unwrap().push(request);
        true
    })
    .unwrap();
    assert!(
        start_server(&pipe, &security, |_| true).is_err(),
        "a second server must not share the activation pipe"
    );

    let file = std::env::temp_dir().join(unique("activation-file"));
    std::fs::write(&file, b"input").unwrap();
    let missing = file.with_extension("missing");
    let timeout = Duration::from_secs(10);
    deliver(&pipe, encode_request(None).unwrap(), timeout).unwrap();
    deliver(&pipe, encode_request(Some(&file)).unwrap(), timeout).unwrap();
    assert!(deliver(&pipe, encode_request(Some(&missing)).unwrap(), timeout).is_err());
    assert!(deliver(&pipe, b"not an activation".to_vec(), timeout).is_err());
    assert_eq!(*received.lock().unwrap(), vec![None, Some(file.clone())]);
    std::fs::remove_file(&file).unwrap();

    let refusing = format!(r"\\.\pipe\{}", unique("refusing"));
    start_server(&refusing, &security, |_| false).unwrap();
    assert!(deliver(&refusing, encode_request(None).unwrap(), timeout).is_err());

    let started = Instant::now();
    let absent = format!(r"\\.\pipe\{}", unique("absent"));
    let error = deliver(
        &absent,
        encode_request(None).unwrap(),
        Duration::from_millis(300),
    );
    assert!(error.is_err());
    assert!(started.elapsed() < Duration::from_secs(5));
}
