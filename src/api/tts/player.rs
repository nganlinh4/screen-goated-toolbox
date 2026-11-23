pub(crate) mod audio_player;
mod device_failure;
mod render_state;

use std::sync::{Arc, atomic::Ordering};
use std::time::Duration;

use super::manager::TtsManager;
use super::types::*;
use super::utils::{clear_tts_loading_state, clear_tts_state};
use audio_player::AudioPlayer;

fn admit_chain_effect(
    token: Option<&crate::overlay::result::ChainCancelToken>,
    effect: impl FnOnce(),
) -> bool {
    match token {
        Some(token) => token.run_if_active(effect).is_some(),
        None => {
            effect();
            true
        }
    }
}

/// Main Player thread - consumes audio streams sequentially
pub fn run_player_thread(manager: Arc<TtsManager>) {
    // Create ONE persistent audio player
    // This avoids the overhead of opening the audio device for every request
    // We pass manager to AudioPlayer so it can check interrupts
    let audio_player = Arc::new(AudioPlayer::new_with_progress(
        PLAYBACK_SAMPLE_RATE,
        manager.playback_played_frames.clone(),
    ));
    *manager.playback_device.lock().unwrap() = Arc::downgrade(&audio_player);

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
            let job = pq.pop_front();
            if let Some((_, _, id, _, realtime, cancel, _)) = &job {
                *manager.active_playback.lock().unwrap() = Some((*id, *realtime, cancel.clone()));
            }
            job
        };

        if let Some((rx, hwnd, req_id, generation, is_realtime, cancel, chain_token)) = playback_job
        {
            let progress = manager.pending_progress.lock().unwrap().remove(&req_id);
            let device_failure_generation = audio_player.begin_request(progress.as_deref());
            let cancelled = || {
                cancel.load(Ordering::SeqCst)
                    || generation < manager.interrupt_generation.load(Ordering::SeqCst)
                    || manager.shutdown.load(Ordering::SeqCst)
                    || audio_player.failure_generation() != device_failure_generation
            };
            crate::log_info!(
                "[TTS Playback] started request={} generation={} realtime={}",
                req_id,
                generation,
                is_realtime
            );
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
                match rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(AudioEvent::Data(data)) => {
                        // Check interrupt before playing
                        if cancelled() {
                            if !is_realtime {
                                eprintln!(
                                    "[TTS Player] Interrupted during playback (after {} chunks)",
                                    chunks_received
                                );
                            }
                            audio_player.stop();
                            clear_tts_state(hwnd, req_id);
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
                            clear_tts_loading_state(hwnd, req_id);
                        }
                        let mut admitted = false;
                        if !manager.admit_playback(cancelled, || {
                            admitted = admit_chain_effect(chain_token.as_deref(), || {
                                audio_player.play(&data, is_realtime)
                            });
                        }) || !admitted
                        {
                            audio_player.stop();
                            clear_tts_state(hwnd, req_id);
                            break;
                        }
                    }
                    Ok(AudioEvent::Error(error)) => {
                        crate::log_info!(
                            "[TTS Playback] failed request={} error={}",
                            req_id,
                            error
                        );
                        if !is_realtime {
                            eprintln!("[TTS Player] AudioEvent::Error: {error}");
                        }
                        cancel.store(true, Ordering::SeqCst);
                        audio_player.stop();
                        clear_tts_state(hwnd, req_id);
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
                        if cancelled() {
                            if !is_realtime {
                                eprintln!("[TTS Player] Was interrupted, stopping immediately");
                            }
                            audio_player.stop(); // Immediate cut-off
                        } else {
                            if !is_realtime {
                                eprintln!("[TTS Player] Normal finish, draining audio buffer");
                            }
                            let mut flushed = false;
                            let mut admitted = false;
                            if !manager.admit_playback(cancelled, || {
                                admitted = admit_chain_effect(chain_token.as_deref(), || {
                                    flushed = audio_player.flush_stretcher(&cancelled);
                                });
                            }) || !admitted
                                || !flushed
                                || !audio_player.drain(cancelled)
                            {
                                audio_player.stop();
                                cancel.store(true, Ordering::SeqCst);
                            }
                        }
                        clear_tts_state(hwnd, req_id);
                        break; // Job done
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        // Check interrupt while waiting for data (e.g. WebSocket stall)
                        if cancelled() {
                            if !is_realtime {
                                eprintln!("[TTS Player] Interrupted/shutdown during recv timeout");
                            }
                            audio_player.stop();
                            clear_tts_state(hwnd, req_id);
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
                        if cancelled() {
                            audio_player.stop();
                        } else {
                            let mut flushed = false;
                            let mut admitted = false;
                            if !manager.admit_playback(cancelled, || {
                                admitted = admit_chain_effect(chain_token.as_deref(), || {
                                    flushed = audio_player.flush_stretcher(&cancelled);
                                });
                            }) || !admitted
                                || !flushed
                                || !audio_player.drain(cancelled)
                            {
                                audio_player.stop();
                                cancel.store(true, Ordering::SeqCst);
                            }
                        }
                        clear_tts_state(hwnd, req_id);
                        break;
                    }
                }

                if manager.shutdown.load(Ordering::SeqCst) {
                    manager.is_playing.store(false, Ordering::SeqCst);
                    return;
                }

                // Check interrupt again
                if cancelled() {
                    audio_player.stop();
                    clear_tts_state(hwnd, req_id);
                    break;
                }
            }

            // Mark that we're done playing this job
            crate::log_info!(
                "[TTS Playback] finished request={} chunks={} interrupted={}",
                req_id,
                chunks_received,
                generation < manager.interrupt_generation.load(Ordering::SeqCst)
            );
            manager.is_playing.store(false, Ordering::SeqCst);
            let mut active = manager.active_playback.lock().unwrap();
            if cancelled() {
                cancel.store(true, Ordering::SeqCst);
                audio_player.stop();
            }
            if let Some(progress) = &progress {
                progress.finish();
            }
            *active = None;
            manager.finish_realtime_text(req_id);
        }
    }
}

#[cfg(test)]
mod admission_tests {
    use super::admit_chain_effect;
    use crate::overlay::result::ChainCancelToken;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn cancelled_branch_cannot_admit_speech_but_its_sibling_can() {
        let root = ChainCancelToken::new();
        let left = ChainCancelToken::child(&root);
        let right = ChainCancelToken::child(&root);
        let effects = AtomicUsize::new(0);
        left.cancel();
        assert!(!admit_chain_effect(Some(&left), || {
            effects.fetch_add(1, Ordering::SeqCst);
        }));
        assert!(admit_chain_effect(Some(&right), || {
            effects.fetch_add(1, Ordering::SeqCst);
        }));
        root.cancel();
        assert!(!admit_chain_effect(Some(&right), || {
            effects.fetch_add(1, Ordering::SeqCst);
        }));
        assert_eq!(effects.load(Ordering::SeqCst), 1);
    }
}
