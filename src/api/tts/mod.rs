//! Text-to-Speech using Gemini Live API
//!
//! This module provides persistent TTS capabilities using Gemini's native
//! audio model. The WebSocket connection is maintained at app startup
//! for instant speech synthesis with minimal latency.

pub mod types;
pub mod wsola;
pub mod utils;
pub mod websocket;
pub mod manager;
pub mod player;
pub mod worker;
pub mod instance;

// Re-export public API for backward compatibility
pub use types::TtsRequest;
pub use manager::TtsManager;
pub use instance::TTS_MANAGER;

/// Initialize the TTS system - call this at app startup
pub fn init_tts() {
    // Spawn 1 Player Thread
    let manager = TTS_MANAGER.clone();
    std::thread::spawn(move || {
        player::run_player_thread(manager);
    });

    // Spawn 2 Socket Worker Threads (Parallel Fetching)
    for _ in 0..2 {
        let manager = TTS_MANAGER.clone();
        std::thread::spawn(move || {
            worker::run_socket_worker(manager);
        });
    }
}
