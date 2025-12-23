//! Text-to-Speech using Gemini Live API
//!
//! This module provides persistent TTS capabilities using Gemini's native
//! audio model. The WebSocket connection is maintained at app startup
//! for instant speech synthesis with minimal latency.

use anyhow::Result;
use base64::{Engine as _, engine::general_purpose};
use std::sync::{Arc, atomic::{AtomicBool, AtomicU64, Ordering}, Mutex, Condvar};
use std::net::TcpStream;
use std::time::{Duration, Instant};
use std::collections::VecDeque;
use lazy_static::lazy_static;

use crate::APP;

/// Model for TTS (same native audio model, configured for output only)
const TTS_MODEL: &str = "gemini-2.5-flash-native-audio-preview-12-2025";

/// Output audio sample rate from Gemini (24kHz)
const SOURCE_SAMPLE_RATE: u32 = 24000;

/// Playback sample rate (48kHz - most devices support this)
const PLAYBACK_SAMPLE_RATE: u32 = 48000;

/// TTS request with unique ID for cancellation
#[derive(Clone)]
pub struct TtsRequest {
    pub id: u64,
    pub text: String,
    pub hwnd: isize, // Window handle to update state when audio starts
}

/// Global TTS manager - singleton pattern for persistent connection
lazy_static! {
    /// The global TTS connection manager
    pub static ref TTS_MANAGER: Arc<TtsManager> = Arc::new(TtsManager::new());
    
    /// Counter for generating unique request IDs
    static ref REQUEST_ID_COUNTER: AtomicU64 = AtomicU64::new(1);
}

/// Manages the persistent TTS WebSocket connection
pub struct TtsManager {
    /// Flag to indicate if the connection is ready
    is_ready: AtomicBool,
    /// Flag to stop the current speech
    stop_current: AtomicBool,
    /// Current active request ID (0 = no active request)
    current_request_id: AtomicU64,
    /// Queue of pending TTS requests
    request_queue: Mutex<VecDeque<TtsRequest>>,
    /// Condvar to signal new requests
    request_signal: Condvar,
    /// Flag to shutdown the manager
    shutdown: AtomicBool,
}

impl TtsManager {
    pub fn new() -> Self {
        Self {
            is_ready: AtomicBool::new(false),
            stop_current: AtomicBool::new(false),
            current_request_id: AtomicU64::new(0),
            request_queue: Mutex::new(VecDeque::new()),
            request_signal: Condvar::new(),
            shutdown: AtomicBool::new(false),
        }
    }
    
    /// Check if TTS is ready to accept requests
    pub fn is_ready(&self) -> bool {
        self.is_ready.load(Ordering::SeqCst)
    }
    
    /// Request TTS for the given text. Returns immediately.
    /// If TTS is already speaking, the current speech is stopped and new one starts.
    /// hwnd is used to update window state when audio starts playing.
    pub fn speak(&self, text: &str, hwnd: isize) -> u64 {
        let id = REQUEST_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        
        // Stop any current speech
        self.stop_current.store(true, Ordering::SeqCst);
        
        // Clear queue and add new request
        {
            let mut queue = self.request_queue.lock().unwrap();
            queue.clear();
            queue.push_back(TtsRequest {
                id,
                text: text.to_string(),
                hwnd,
            });
        }
        
        // Signal the worker thread
        self.request_signal.notify_one();
        
        id
    }
    
    /// Stop the current speech or cancel pending request
    pub fn stop(&self) {
        self.stop_current.store(true, Ordering::SeqCst);
        
        // Clear queue
        {
            let mut queue = self.request_queue.lock().unwrap();
            queue.clear();
        }
    }
    
    /// Stop speech for a specific request ID (only if it's the current one)
    pub fn stop_if_active(&self, request_id: u64) {
        if self.current_request_id.load(Ordering::SeqCst) == request_id {
            self.stop();
        }
    }
    
    /// Check if this request ID is currently active
    pub fn is_speaking(&self, request_id: u64) -> bool {
        self.current_request_id.load(Ordering::SeqCst) == request_id
    }
    
    /// Shutdown the TTS manager
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
        self.stop_current.store(true, Ordering::SeqCst);
        self.request_signal.notify_one();
    }
}

/// Initialize the TTS system - call this at app startup
pub fn init_tts() {
    std::thread::spawn(|| {
        run_tts_worker();
    });
}

/// Clear the TTS loading state for a window and trigger repaint
fn clear_tts_loading_state(hwnd: isize) {
    use crate::overlay::result::state::WINDOW_STATES;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::InvalidateRect;
    
    {
        let mut states = WINDOW_STATES.lock().unwrap();
        if let Some(state) = states.get_mut(&hwnd) {
            state.tts_loading = false;
        }
    }
    
    // Trigger repaint to update button appearance
    unsafe {
        InvalidateRect(HWND(hwnd), None, false);
    }
}

/// Clear TTS state completely when speech ends
fn clear_tts_state(hwnd: isize) {
    use crate::overlay::result::state::WINDOW_STATES;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::InvalidateRect;
    
    {
        let mut states = WINDOW_STATES.lock().unwrap();
        if let Some(state) = states.get_mut(&hwnd) {
            state.tts_loading = false;
            state.tts_request_id = 0;
        }
    }
    
    // Trigger repaint to update button appearance
    unsafe {
        InvalidateRect(HWND(hwnd), None, false);
    }
}

/// Create TLS WebSocket connection to Gemini Live API for TTS
fn connect_tts_websocket(api_key: &str) -> Result<tungstenite::WebSocket<native_tls::TlsStream<TcpStream>>> {
    let ws_url = format!(
        "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent?key={}",
        api_key
    );
    
    let url = url::Url::parse(&ws_url)?;
    let host = url.host_str().ok_or_else(|| anyhow::anyhow!("No host in URL"))?;
    let port = 443;
    
    eprintln!("TTS: Resolving {}...", host);
    use std::net::ToSocketAddrs;
    let addr = format!("{}:{}", host, port)
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| anyhow::anyhow!("Failed to resolve hostname: {}", host))?;
    eprintln!("TTS: Resolved to {}", addr);
    
    eprintln!("TTS: Opening TCP connection...");
    let tcp_stream = TcpStream::connect_timeout(&addr, Duration::from_secs(10))?;
    tcp_stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    tcp_stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    tcp_stream.set_nodelay(true)?;
    eprintln!("TTS: TCP connected");
    
    eprintln!("TTS: TLS handshake...");
    let connector = native_tls::TlsConnector::new()?;
    let tls_stream = connector.connect(host, tcp_stream)?;
    eprintln!("TTS: TLS connected");
    
    eprintln!("TTS: WebSocket handshake...");
    let (socket, _response) = tungstenite::client::client(&ws_url, tls_stream)?;
    eprintln!("TTS: WebSocket connected!");
    
    Ok(socket)
}

/// Send TTS setup message - configures for audio output only, no input transcription
fn send_tts_setup(socket: &mut tungstenite::WebSocket<native_tls::TlsStream<TcpStream>>) -> Result<()> {
    let setup = serde_json::json!({
        "setup": {
            "model": format!("models/{}", TTS_MODEL),
            "generationConfig": {
                "responseModalities": ["AUDIO"],
                "speechConfig": {
                    "voiceConfig": {
                        "prebuiltVoiceConfig": {
                            "voiceName": "Aoede"
                        }
                    }
                },
                "thinkingConfig": {
                    "thinkingBudget": 0
                }
            },
            "systemInstruction": {
                "parts": [{
                    "text": "You are a text-to-speech reader. Your ONLY job is to read the user's text out loud, exactly as written, word for word. Do NOT respond conversationally. Do NOT add commentary. Do NOT ask questions. Simply read the provided text aloud naturally and clearly. Start reading immediately."
                }]
            }
        }
    });
    
    let msg_str = setup.to_string();
    eprintln!("TTS: Sending setup: {}", msg_str);
    socket.write(tungstenite::Message::Text(msg_str))?;
    socket.flush()?;
    
    Ok(())
}

/// Send text to be spoken
fn send_tts_text(socket: &mut tungstenite::WebSocket<native_tls::TlsStream<TcpStream>>, text: &str) -> Result<()> {
    // Format with explicit instruction to read verbatim
    let prompt = format!("[READ ALOUD VERBATIM - START NOW]\n\n{}", text);
    
    let msg = serde_json::json!({
        "clientContent": {
            "turns": [{
                "role": "user",
                "parts": [{
                    "text": prompt
                }]
            }],
            "turnComplete": true
        }
    });
    
    socket.write(tungstenite::Message::Text(msg.to_string()))?;
    socket.flush()?;
    
    Ok(())
}

/// Parse audio data from WebSocket message
fn parse_audio_data(msg: &str) -> Option<Vec<u8>> {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(msg) {
        // Check for serverContent -> modelTurn -> parts -> inlineData
        if let Some(server_content) = json.get("serverContent") {
            if let Some(model_turn) = server_content.get("modelTurn") {
                if let Some(parts) = model_turn.get("parts").and_then(|p| p.as_array()) {
                    for part in parts {
                        if let Some(inline_data) = part.get("inlineData") {
                            if let Some(data_b64) = inline_data.get("data").and_then(|d| d.as_str()) {
                                if let Ok(audio_bytes) = general_purpose::STANDARD.decode(data_b64) {
                                    return Some(audio_bytes);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Check if the response indicates turn is complete
fn is_turn_complete(msg: &str) -> bool {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(msg) {
        if let Some(server_content) = json.get("serverContent") {
            // Check for turnComplete
            if let Some(turn_complete) = server_content.get("turnComplete") {
                if turn_complete.as_bool().unwrap_or(false) {
                    return true;
                }
            }
            // Also check for generationComplete (seen in TTS responses)
            if let Some(gen_complete) = server_content.get("generationComplete") {
                if gen_complete.as_bool().unwrap_or(false) {
                    return true;
                }
            }
        }
    }
    false
}

/// Main TTS worker thread - maintains persistent connection and processes requests
fn run_tts_worker() {
    let manager = &*TTS_MANAGER;
    
    // Delay startup to let main app initialize
    std::thread::sleep(Duration::from_secs(2));
    
    loop {
        if manager.shutdown.load(Ordering::SeqCst) {
            break;
        }
        
        // Wait for a request first (lazy connection)
        // This avoids connection timeouts during app startup
        {
            let mut queue = manager.request_queue.lock().unwrap();
            while queue.is_empty() && !manager.shutdown.load(Ordering::SeqCst) {
                let result = manager.request_signal.wait_timeout(queue, Duration::from_secs(30)).unwrap();
                queue = result.0;
            }
        }
        
        if manager.shutdown.load(Ordering::SeqCst) {
            break;
        }
        
        // Get API key
        let api_key = {
            match APP.lock() {
                Ok(app) => app.config.gemini_api_key.clone(),
                Err(_) => {
                    std::thread::sleep(Duration::from_secs(1));
                    continue;
                }
            }
        };
        
        if api_key.trim().is_empty() {
            // No API key configured, wait and retry
            eprintln!("TTS: No Gemini API key configured");
            std::thread::sleep(Duration::from_secs(5));
            continue;
        }
        
        // Attempt to connect
        eprintln!("TTS: Connecting...");
        let socket_result = connect_tts_websocket(&api_key);
        let mut socket = match socket_result {
            Ok(s) => s,
            Err(e) => {
                eprintln!("TTS: Failed to connect: {}", e);
                std::thread::sleep(Duration::from_secs(3));
                continue;
            }
        };
        
        // Send setup
        if let Err(e) = send_tts_setup(&mut socket) {
            eprintln!("TTS: Failed to send setup: {}", e);
            let _ = socket.close(None);
            std::thread::sleep(Duration::from_secs(2));
            continue;
        }
        
        // Wait for setup acknowledgment (blocking mode with 30s timeout)
        let setup_start = Instant::now();
        let mut setup_complete = false;
        loop {
            match socket.read() {
                Ok(tungstenite::Message::Text(msg)) => {
                    eprintln!("TTS: Received: {}", &msg[..msg.len().min(200)]);
                    if msg.contains("setupComplete") {
                        setup_complete = true;
                        eprintln!("TTS: Setup complete, ready for requests");
                        break;
                    }
                    if msg.contains("error") || msg.contains("Error") {
                        eprintln!("TTS: Setup error: {}", msg);
                        break;
                    }
                }
                Ok(tungstenite::Message::Close(frame)) => {
                    let close_info = frame.map(|f| format!("code={}, reason={}", f.code, f.reason)).unwrap_or("no frame".to_string());
                    eprintln!("TTS: Connection closed by server: {}", close_info);
                    break;
                }
                Ok(tungstenite::Message::Binary(data)) => {
                    if let Ok(text) = String::from_utf8(data.clone()) {
                        eprintln!("TTS: Received binary as text: {}", &text[..text.len().min(200)]);
                        if text.contains("setupComplete") {
                            setup_complete = true;
                            eprintln!("TTS: Setup complete (from binary)");
                            break;
                        }
                    }
                }
                Ok(_) => {}
                Err(tungstenite::Error::Io(ref e)) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => {
                    // No data yet, check timeout and continue
                    if setup_start.elapsed() > Duration::from_secs(30) {
                        eprintln!("TTS: Setup timeout - no response from server");
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(e) => {
                    eprintln!("TTS: Error waiting for setup: {}", e);
                    break;
                }
            }
            
            if manager.shutdown.load(Ordering::SeqCst) {
                let _ = socket.close(None);
                return;
            }
        }
        
        if !setup_complete {
            let _ = socket.close(None);
            std::thread::sleep(Duration::from_secs(2));
            continue;
        }
        
        // Connection is ready
        manager.is_ready.store(true, Ordering::SeqCst);
        
        // Set to non-blocking for the main loop
        {
            let stream = socket.get_mut();
            let tcp_stream = stream.get_mut();
            let _ = tcp_stream.set_read_timeout(Some(Duration::from_millis(100)));
        }
        
        // Main processing loop
        'connection_loop: loop {
            if manager.shutdown.load(Ordering::SeqCst) {
                break;
            }
            
            // Wait for a request
            let request: Option<TtsRequest> = {
                let mut queue = manager.request_queue.lock().unwrap();
                if queue.is_empty() {
                    eprintln!("TTS: Waiting for request...");
                    // Wait with timeout
                    let result = manager.request_signal.wait_timeout(queue, Duration::from_secs(30)).unwrap();
                    queue = result.0;
                }
                queue.pop_front()
            };
            
            if manager.shutdown.load(Ordering::SeqCst) {
                break;
            }
            
            if let Some(req) = request {
                eprintln!("TTS: Got request id={}, text length={}", req.id, req.text.len());
                manager.stop_current.store(false, Ordering::SeqCst);
                manager.current_request_id.store(req.id, Ordering::SeqCst);
                
                // Send the text to be spoken
                eprintln!("TTS: Sending text: {}...", &req.text[..req.text.len().min(100)]);
                if let Err(e) = send_tts_text(&mut socket, &req.text) {
                    eprintln!("TTS: Failed to send text: {}", e);
                    manager.current_request_id.store(0, Ordering::SeqCst);
                    // Clear loading state on error
                    clear_tts_loading_state(req.hwnd);
                    break 'connection_loop; // Reconnect
                }
                eprintln!("TTS: Text sent, initializing audio player...");
                
                // Initialize audio playback (Windows Audio API)
                let audio_player = AudioPlayer::new(PLAYBACK_SAMPLE_RATE);
                
                // Receive and play audio chunks
                eprintln!("TTS: Waiting for audio response...");
                let mut audio_chunks_received = 0;
                let mut loading_cleared = false;
                loop {
                    if manager.stop_current.load(Ordering::SeqCst) {
                        // Stop requested - drain any pending audio
                        drop(audio_player);
                        eprintln!("TTS: Stopped by user");
                        clear_tts_loading_state(req.hwnd);
                        break;
                    }
                    
                    if manager.shutdown.load(Ordering::SeqCst) {
                        break 'connection_loop;
                    }
                    
                    match socket.read() {
                        Ok(tungstenite::Message::Text(msg)) => {
                            eprintln!("TTS: Received text message: {}...", &msg[..msg.len().min(150)]);
                            // Parse and play audio data
                            if let Some(audio_data) = parse_audio_data(&msg) {
                                audio_chunks_received += 1;
                                
                                // On first audio chunk, clear loading state (button turns blue)
                                if !loading_cleared {
                                    loading_cleared = true;
                                    clear_tts_loading_state(req.hwnd);
                                    eprintln!("TTS: First audio received, button now blue");
                                }
                                
                                eprintln!("TTS: Got audio chunk #{}, {} bytes", audio_chunks_received, audio_data.len());
                                audio_player.play(&audio_data);
                            }
                            
                            // Check if turn is complete
                            if is_turn_complete(&msg) {
                                eprintln!("TTS: Turn complete, draining audio ({} chunks received)", audio_chunks_received);
                                // Wait for audio to finish playing
                                audio_player.drain();
                                break;
                            }
                        }
                        Ok(tungstenite::Message::Binary(data)) => {
                            // Try to parse as JSON text
                            if let Ok(text) = String::from_utf8(data.clone()) {
                                eprintln!("TTS: Received binary as text: {}...", &text[..text.len().min(150)]);
                                if let Some(audio_data) = parse_audio_data(&text) {
                                    audio_chunks_received += 1;
                                    
                                    // On first audio chunk, clear loading state
                                    if !loading_cleared {
                                        loading_cleared = true;
                                        clear_tts_loading_state(req.hwnd);
                                        eprintln!("TTS: First audio received (binary), button now blue");
                                    }
                                    
                                    eprintln!("TTS: Got audio chunk #{}, {} bytes", audio_chunks_received, audio_data.len());
                                    audio_player.play(&audio_data);
                                }
                                if is_turn_complete(&text) {
                                    eprintln!("TTS: Turn complete (from binary), draining audio ({} chunks received)", audio_chunks_received);
                                    audio_player.drain();
                                    break;
                                }
                            }
                        }
                        Ok(tungstenite::Message::Close(_)) => {
                            eprintln!("TTS: Connection closed by server");
                            break 'connection_loop; // Reconnect
                        }
                        Ok(_) => {}
                        Err(tungstenite::Error::Io(ref e)) 
                            if e.kind() == std::io::ErrorKind::WouldBlock 
                            || e.kind() == std::io::ErrorKind::TimedOut => {
                            // No data available, continue
                        }
                        Err(e) => {
                            eprintln!("TTS: Read error: {}", e);
                            break 'connection_loop; // Reconnect
                        }
                    }
                    
                    std::thread::sleep(Duration::from_millis(5));
                }
                
                manager.current_request_id.store(0, Ordering::SeqCst);
                
                // Clear button state when speech completes
                clear_tts_state(req.hwnd);
                
                // Break connection after each request to get fresh context
                // This prevents conversation history from accumulating
                eprintln!("TTS: Request complete, reconnecting for fresh context...");
                break 'connection_loop;
            }
            
            // No request, check if we should stay connected (timeout after ping)
        }
        
        // Connection lost or error - mark as not ready and reconnect
        manager.is_ready.store(false, Ordering::SeqCst);
        let _ = socket.close(None);
        
        if !manager.shutdown.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_secs(2));
        }
    }
}

/// Simple audio player using Windows Audio API
struct AudioPlayer {
    #[allow(dead_code)]
    sample_rate: u32,
    // Audio buffer for accumulating samples
    buffer: Vec<u8>,
    // Handle to Windows audio stream (cpal)
    stream: Option<cpal::Stream>,
    // Shared buffer for audio data
    shared_buffer: Arc<Mutex<VecDeque<i16>>>,
}

impl AudioPlayer {
    fn new(sample_rate: u32) -> Self {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
        
        eprintln!("TTS: Creating audio player for source at {}Hz", sample_rate);
        
        let shared_buffer: Arc<Mutex<VecDeque<i16>>> = Arc::new(Mutex::new(VecDeque::new()));
        let buffer_clone = shared_buffer.clone();
        
        // Use WASAPI explicitly on Windows for better compatibility
        #[cfg(target_os = "windows")]
        let host = cpal::host_from_id(cpal::HostId::Wasapi).unwrap_or(cpal::default_host());
        #[cfg(not(target_os = "windows"))]
        let host = cpal::default_host();
        
        let device = host.default_output_device();
        
        if device.is_none() {
            eprintln!("TTS: No audio output device found!");
        }
        
        let stream = device.and_then(|device| {
            eprintln!("TTS: Using audio device: {:?}", device.name());
            
            // Try to get supported configs for debugging
            if let Ok(configs) = device.supported_output_configs() {
                for cfg in configs {
                    eprintln!("TTS: Supported config: {:?}", cfg);
                }
            }
            
            // Try f32 format first (more commonly supported)
            // Use stereo (2 channels) since many devices don't support mono
            let config = cpal::StreamConfig {
                channels: 2,
                sample_rate: cpal::SampleRate(sample_rate),
                buffer_size: cpal::BufferSize::Default,
            };
            
            // Clone for the f32 closure
            let buffer_clone_f32 = buffer_clone.clone();
            
            // Try building with f32 format
            match device.build_output_stream(
                &config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let mut buf = buffer_clone_f32.lock().unwrap();
                    // For stereo, output same sample to both channels
                    for frame in data.chunks_mut(2) {
                        let i16_sample = buf.pop_front().unwrap_or(0);
                        let sample = i16_sample as f32 / 32768.0;
                        frame[0] = sample; // Left
                        frame[1] = sample; // Right (same as left for mono source)
                    }
                },
                |err| eprintln!("TTS Audio error: {}", err),
                None,
            ) {
                Ok(stream) => {
                    eprintln!("TTS: Created f32 stream at {}Hz", sample_rate);
                    Some(stream)
                }
                Err(e) => {
                    eprintln!("TTS: Failed to create f32 stream: {}", e);
                    // Try i16 format as fallback
                    match device.build_output_stream(
                        &config,
                        move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                            let mut buf = buffer_clone.lock().unwrap();
                            // For stereo, output same sample to both channels
                            for frame in data.chunks_mut(2) {
                                let sample = buf.pop_front().unwrap_or(0);
                                frame[0] = sample; // Left
                                frame[1] = sample; // Right
                            }
                        },
                        |err| eprintln!("TTS Audio error: {}", err),
                        None,
                    ) {
                        Ok(stream) => {
                            eprintln!("TTS: Created i16 stream at {}Hz", sample_rate);
                            Some(stream)
                        }
                        Err(e2) => {
                            eprintln!("TTS: Failed to create i16 stream: {}", e2);
                            None
                        }
                    }
                }
            }
        });
        
        if stream.is_none() {
            eprintln!("TTS: Failed to create audio stream!");
        } else {
            eprintln!("TTS: Audio stream created successfully");
        }
        
        if let Some(ref s) = stream {
            match s.play() {
                Ok(_) => eprintln!("TTS: Audio stream started"),
                Err(e) => eprintln!("TTS: Failed to start stream: {}", e),
            }
        }
        
        Self {
            sample_rate,
            buffer: Vec::new(),
            stream,
            shared_buffer,
        }
    }
    
    fn play(&self, audio_data: &[u8]) {
        // Convert raw PCM bytes to i16 samples (little-endian)
        // Also upsample from 24kHz to 48kHz by duplicating each sample
        let mut samples = Vec::with_capacity(audio_data.len()); // 2x because of upsampling
        for chunk in audio_data.chunks_exact(2) {
            let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
            // Duplicate each sample for 2x upsampling (24kHz -> 48kHz)
            samples.push(sample);
            samples.push(sample);
        }
        
        // Add to shared buffer
        if let Ok(mut buf) = self.shared_buffer.lock() {
            buf.extend(samples);
        }
    }
    
    fn drain(&self) {
        // Wait for buffer to drain
        loop {
            let len = self.shared_buffer.lock().map(|b| b.len()).unwrap_or(0);
            if len == 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        // Extra grace period for audio hardware
        std::thread::sleep(Duration::from_millis(100));
    }
}

impl Drop for AudioPlayer {
    fn drop(&mut self) {
        // Stream will be stopped when dropped
    }
}
