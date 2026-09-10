use std::time::Duration;
use ureq::tls::TlsConfig;

pub(super) fn build(
    tls: TlsConfig,
    user_agent: &str,
    response_start_timeout: Duration,
    progress_idle_timeout: Duration,
) -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent(user_agent)
        .timeout_connect(Some(Duration::from_secs(30)))
        // ureq carries a completed phase's deadline into the next phase. Bound
        // header receipt with the completed request deadline, not RecvResponse:
        // that deadline would also cap the entire otherwise-progressing body.
        .timeout_send_request(Some(response_start_timeout))
        .timeout_recv_response(None)
        .timeout_recv_body(Some(progress_idle_timeout))
        .tls_config(tls)
        .build()
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read as _, Write as _};
    use std::net::TcpListener;
    use std::time::Instant;

    fn server(
        response: impl FnOnce(&mut std::net::TcpStream) + Send + 'static,
    ) -> (String, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/model", listener.local_addr().unwrap());
        let thread = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut header = Vec::new();
            while !header.ends_with(b"\r\n\r\n") {
                let mut byte = [0_u8];
                stream.read_exact(&mut byte).unwrap();
                header.push(byte[0]);
                assert!(header.len() < 4096);
            }
            response(&mut stream);
        });
        (url, thread)
    }

    fn agent(response_start: Duration, idle: Duration) -> ureq::Agent {
        build(
            super::super::platform_tls_config(),
            "download-test",
            response_start,
            idle,
        )
    }

    #[test]
    fn active_body_outlives_response_start_budget() {
        let (url, server) = server(|stream| {
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\n")
                .unwrap();
            for _ in 0..8 {
                std::thread::sleep(Duration::from_millis(70));
                if stream.write_all(b"x").is_err() {
                    break;
                }
            }
        });
        let agent = agent(Duration::from_millis(250), Duration::from_millis(250));
        let started = Instant::now();
        let response = agent.get(url).call().unwrap();
        let mut text = String::new();
        response
            .into_body()
            .into_reader()
            .read_to_string(&mut text)
            .unwrap();
        server.join().unwrap();
        assert_eq!(text, "xxxxxxxx");
        assert!(started.elapsed() >= Duration::from_millis(500));
    }

    #[test]
    fn absent_response_headers_remain_bounded() {
        let (url, server) = server(|_| std::thread::sleep(Duration::from_millis(300)));
        let error = agent(Duration::from_millis(80), Duration::from_millis(80))
            .get(url)
            .call()
            .unwrap_err();
        server.join().unwrap();
        assert!(matches!(error, ureq::Error::Timeout(_)), "{error:?}");
    }

    #[test]
    fn stalled_body_remains_idle_bounded() {
        let (url, server) = server(|stream| {
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\n")
                .unwrap();
            std::thread::sleep(Duration::from_millis(300));
        });
        let response = agent(Duration::from_secs(2), Duration::from_millis(80))
            .get(url)
            .call()
            .unwrap();
        let error = response
            .into_body()
            .into_reader()
            .read(&mut [0_u8; 1])
            .unwrap_err();
        server.join().unwrap();
        assert!(error.to_string().contains("timeout"), "{error:?}");
    }
}
