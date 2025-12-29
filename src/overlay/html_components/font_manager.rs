//! Font Manager - Bundles Google Sans Flex variable font and serves it via local HTTP
//!
//! Spins up a tiny ephemeral HTTP server to serve the bundled font.
//! This bypasses WebView2 file:// restrictions and base64 size limits.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Mutex, Once};
use wry::WebViewBuilder;

/// Google Sans Flex variable font - bundled at compile time (~5MB)
static GOOGLE_SANS_FLEX_TTF: &[u8] =
    include_bytes!("../../../assets/GoogleSansFlex-VariableFont_GRAD,ROND,opsz,slnt,wdth,wght.ttf");

static INIT_FONTS: Once = Once::new();
lazy_static::lazy_static! {
    static ref FONT_SERVER_URL: Mutex<Option<String>> = Mutex::new(None);
}

pub fn warmup_fonts() {
    start_font_server();
}

/// Helper to configure WebViewBuilder (legacy pass-through)
pub fn configure_webview(builder: WebViewBuilder) -> WebViewBuilder {
    builder
}

fn start_font_server() {
    INIT_FONTS.call_once(|| {
        std::thread::spawn(|| {
            // Bind to ephemeral port
            let listener = match TcpListener::bind("127.0.0.1:0") {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("Failed to bind font server: {}", e);
                    return;
                }
            };

            let addr = listener.local_addr().unwrap();
            let url = format!("http://{}:{}/GoogleSansFlex.ttf", addr.ip(), addr.port());
            println!("Font server running at {}", url);

            {
                let mut url_guard = FONT_SERVER_URL.lock().unwrap();
                *url_guard = Some(url);
            }

            for stream in listener.incoming() {
                match stream {
                    Ok(mut stream) => {
                        let _ = std::thread::spawn(move || {
                            let mut buffer = [0; 1024];
                            // Read request (just consume it)
                            let _ = stream.read(&mut buffer);

                            // Simple check if it looks like a GET (optional, we only serve one thing)
                            // Serve font
                            let response_header = format!(
                                "HTTP/1.1 200 OK\r\n\
                                Content-Type: font/ttf\r\n\
                                Access-Control-Allow-Origin: *\r\n\
                                Content-Length: {}\r\n\
                                Connection: close\r\n\r\n",
                                GOOGLE_SANS_FLEX_TTF.len()
                            );

                            if let Err(e) = stream.write_all(response_header.as_bytes()) {
                                eprintln!("Font server write error: {}", e);
                                return;
                            }
                            if let Err(e) = stream.write_all(GOOGLE_SANS_FLEX_TTF) {
                                eprintln!("Font server body error: {}", e);
                                return;
                            }
                            let _ = stream.flush();
                        });
                    }
                    Err(e) => eprintln!("Font server request error: {}", e),
                }
            }
        });

        // Give the thread a moment to bind and set the URL
        std::thread::sleep(std::time::Duration::from_millis(50));
    });
}

pub fn get_font_css() -> String {
    // Ensure server is started
    start_font_server();

    // Get URL with retry logic
    let mut font_url = String::new();
    for _ in 0..10 {
        if let Ok(guard) = FONT_SERVER_URL.lock() {
            if let Some(url) = guard.as_ref() {
                font_url = url.clone();
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    if font_url.is_empty() {
        eprintln!("ERROR: Could not get font server URL");
    }

    format!(
        r#"
        @font-face {{
            font-family: 'Google Sans Flex';
            font-style: normal;
            font-weight: 100 1000;
            font-stretch: 25% 151%;
            font-display: block;
            src: url('{}') format('truetype');
        }}
    "#,
        font_url
    )
}
