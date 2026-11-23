use super::*;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

fn http(port: u16, request: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let mut response = Vec::new();
    let _ = stream.read_to_end(&mut response);
    String::from_utf8_lossy(&response).into_owned()
}

#[test]
fn reopen_reuses_the_server_and_playback_is_served_during_a_stalled_upload() {
    let port = start_global_media_server().unwrap();
    assert_eq!(
        start_global_media_server().unwrap(),
        port,
        "a second editor window must reuse the running server"
    );
    let token = MEDIA_SERVER_TOKEN.get().unwrap().clone();

    let mut random = [0u8; 8];
    getrandom::fill(&mut random).unwrap();
    let media = std::env::temp_dir().join(format!(
        "sgt-media-server-{}-{}.mp4",
        std::process::id(),
        u64::from_ne_bytes(random)
    ));
    std::fs::write(&media, vec![7u8; 1000]).unwrap();

    // An upload whose body never finishes arriving.
    let mut upload = TcpStream::connect(("127.0.0.1", port)).unwrap();
    upload
        .write_all(
            format!(
                "POST /write-temp HTTP/1.1\r\nHost: 127.0.0.1\r\nX-SGT-Token: {token}\r\nContent-Length: 1000000\r\n\r\npartial"
            )
            .as_bytes(),
        )
        .unwrap();
    std::thread::sleep(Duration::from_millis(200));

    let started = std::time::Instant::now();
    let response = http(
        port,
        &format!(
            "GET /?path={}&token={token} HTTP/1.1\r\nHost: 127.0.0.1\r\nRange: bytes=0-9\r\nConnection: close\r\n\r\n",
            urlencoding::encode(&media.to_string_lossy())
        ),
    );
    assert!(response.starts_with("HTTP/1.1 206"), "{response}");
    assert!(started.elapsed() < Duration::from_secs(5));

    let past_end = http(
        port,
        &format!(
            "GET /?path={}&token={token} HTTP/1.1\r\nHost: 127.0.0.1\r\nRange: bytes=5000-\r\nConnection: close\r\n\r\n",
            urlencoding::encode(&media.to_string_lossy())
        ),
    );
    assert!(past_end.starts_with("HTTP/1.1 416"), "{past_end}");

    drop(upload);
    std::fs::remove_file(media).unwrap();
}
