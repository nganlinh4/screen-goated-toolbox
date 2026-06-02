mod audio_player;

use std::sync::{Arc, atomic::Ordering};
use std::time::Duration;

use super::manager::TtsManager;
use super::types::*;
use super::utils::{clear_tts_loading_state, clear_tts_state};
use audio_player::AudioPlayer;

/// Main Player thread - consumes audio streams sequentially
pub fn run_player_thread(manager: Arc<TtsManager>) {
    // Create ONE persistent audio player
    // This avoids the overhead of opening the audio device for every request
    // We pass manager to AudioPlayer so it can check interrupts
    let audio_player = AudioPlayer::new(PLAYBACK_SAMPLE_RATE, manager.clone());

    loop {
        if manager.shutdown.load(Ordering::SeqCst) {
            break;
        }

        let playback_job = {
            let mut pq = manager.playback_queue.lock().unwrap();
            while pq.is_empty() && !manager.shutdown.load(Ordering::SeqCst) {
                let result = manager.playback_signal.wait(pq).unwrap();
                pq = result;
            }
            if manager.shutdown.load(Ordering::SeqCst) {
                return;
            }
            pq.pop_front()
        };

        if let Some((rx, hwnd, req_id, generation, is_realtime)) = playback_job {
            let mut loading_cleared = false;
            let mut chunks_received = 0u32;

            if !is_realtime {
                eprintln!(
                    "[TTS Player] Starting playback job: req_id={}, hwnd={}, gen={}, realtime={}",
                    req_id, hwnd, generation, is_realtime
                );
            }

            // Mark that we're now playing audio
            manager.is_playing.store(true, Ordering::SeqCst);

            // Loop reading chunks from this channel (with timeout to check interrupts)
            loop {
                match rx.recv_timeout(Duration::from_millis(500)) {
                    Ok(AudioEvent::Data(data)) => {
                        // Check interrupt before playing
                        if generation < manager.interrupt_generation.load(Ordering::SeqCst) {
                            if !is_realtime {
                                eprintln!(
                                    "[TTS Player] Interrupted during playback (after {} chunks)",
                                    chunks_received
                                );
                            }
                            audio_player.stop();
                            clear_tts_state(hwnd);
                            break;
                        }

                        chunks_received += 1;
                        if !loading_cleared {
                            loading_cleared = true;
                            if !is_realtime {
                                eprintln!(
                                    "[TTS Player] First audio chunk received, clearing loading state"
                                );
                            }
                            clear_tts_loading_state(hwnd);
                        }
                        audio_player.play(&data, is_realtime);
                    }
                    Ok(AudioEvent::Error(error)) => {
                        if !is_realtime {
                            eprintln!("[TTS Player] AudioEvent::Error: {error}");
                        }
                        audio_player.stop();
                        clear_tts_state(hwnd);
                        break;
                    }
                    Ok(AudioEvent::End) => {
                        if !is_realtime {
                            eprintln!(
                                "[TTS Player] AudioEvent::End received after {} chunks",
                                chunks_received
                            );
                        }
                        // Check if we were interrupted or finished normally
                        if generation < manager.interrupt_generation.load(Ordering::SeqCst) {
                            if !is_realtime {
                                eprintln!("[TTS Player] Was interrupted, stopping immediately");
                            }
                            audio_player.stop(); // Immediate cut-off
                        } else {
                            if !is_realtime {
                                eprintln!("[TTS Player] Normal finish, draining audio buffer");
                            }
                            audio_player.drain(); // Normal finish
                        }
                        clear_tts_state(hwnd);
                        break; // Job done
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        // Check interrupt while waiting for data (e.g. WebSocket stall)
                        if generation < manager.interrupt_generation.load(Ordering::SeqCst)
                            || manager.shutdown.load(Ordering::SeqCst)
                        {
                            if !is_realtime {
                                eprintln!("[TTS Player] Interrupted/shutdown during recv timeout");
                            }
                            audio_player.stop();
                            clear_tts_state(hwnd);
                            break;
                        }
                        continue;
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        // Sender disconnected
                        if !is_realtime {
                            eprintln!(
                                "[TTS Player] Channel disconnected after {} chunks",
                                chunks_received
                            );
                        }
                        if generation < manager.interrupt_generation.load(Ordering::SeqCst) {
                            audio_player.stop();
                        } else {
                            audio_player.drain();
                        }
                        clear_tts_state(hwnd);
                        break;
                    }
                }

                if manager.shutdown.load(Ordering::SeqCst) {
                    manager.is_playing.store(false, Ordering::SeqCst);
                    return;
                }

                // Check interrupt again
                if generation < manager.interrupt_generation.load(Ordering::SeqCst) {
                    audio_player.stop();
                    clear_tts_state(hwnd);
                    break;
                }
            }

            // Mark that we're done playing this job
            manager.is_playing.store(false, Ordering::SeqCst);
        }
    }
}
