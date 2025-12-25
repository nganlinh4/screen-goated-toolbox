use std::sync::{Arc, atomic::Ordering};
use std::time::{Duration, Instant};
use tungstenite::Message;

use crate::APP;
use super::types::AudioEvent;
use super::manager::TtsManager;
use super::websocket::{connect_tts_websocket, send_tts_setup, send_tts_text, parse_audio_data, is_turn_complete};
use super::utils::{clear_tts_state, clear_tts_loading_state, get_language_instruction_for_text};

/// Socket Worker thread - fetches audio data and pipes it to the player
pub fn run_socket_worker(manager: Arc<TtsManager>) {
    // Delay start slightly to stagger connections if multiple workers start at once
    std::thread::sleep(Duration::from_millis(100));
    
    loop {
        if manager.shutdown.load(Ordering::SeqCst) {
            break;
        }
        
        // Wait for a request
        let (request, tx) = {
            let mut queue = manager.work_queue.lock().unwrap();
            while queue.is_empty() && !manager.shutdown.load(Ordering::SeqCst) {
                let result = manager.work_signal.wait(queue).unwrap();
                queue = result;
            }
            if manager.shutdown.load(Ordering::SeqCst) {
                return;
            }
            queue.pop_front().unwrap()
        };
        
        // Check if this request is stale
        if request.generation < manager.interrupt_generation.load(Ordering::SeqCst) {
            let _ = tx.send(AudioEvent::End);
            continue;
        }
        
        // Get API key
        let api_key = {
            match APP.lock() {
                Ok(app) => app.config.gemini_api_key.clone(),
                Err(_) => {
                    let _ = tx.send(AudioEvent::End);
                    std::thread::sleep(Duration::from_secs(1));
                    continue;
                }
            }
        };
        
        if api_key.trim().is_empty() {
            eprintln!("TTS: No Gemini API key configured");
            let _ = tx.send(AudioEvent::End);
            clear_tts_loading_state(request.req.hwnd); 
            clear_tts_state(request.req.hwnd);
            std::thread::sleep(Duration::from_secs(5));
            continue;
        }
        
        // Attempt to connect
        let socket_result = connect_tts_websocket(&api_key);
        let mut socket = match socket_result {
            Ok(s) => s,
            Err(e) => {
                eprintln!("TTS: Failed to connect: {}", e);
                let _ = tx.send(AudioEvent::End);
                clear_tts_loading_state(request.req.hwnd);
                clear_tts_state(request.req.hwnd);
                std::thread::sleep(Duration::from_secs(3));
                continue;
            }
        };
        
        // Read config for setup
        let (current_voice, current_speed, language_instruction) = {
            let app = APP.lock().unwrap();
            let voice = app.config.tts_voice.clone();
            let conditions = app.config.tts_language_conditions.clone();
            
            let instruction = get_language_instruction_for_text(&request.req.text, &conditions);
            
            if request.req.is_realtime {
                (voice, "Normal".to_string(), instruction)
            } else {
                (voice, app.config.tts_speed.clone(), instruction)
            }
        };

        // Send setup
        if let Err(e) = send_tts_setup(&mut socket, &current_voice, &current_speed, language_instruction.as_deref()) {
            eprintln!("TTS: Failed to send setup: {}", e);
            let _ = socket.close(None);
            let _ = tx.send(AudioEvent::End);
            std::thread::sleep(Duration::from_secs(2));
            continue;
        }
        
        // Wait for setup acknowledgment
        let setup_start = Instant::now();
        let mut setup_complete = false;
        loop {
            if request.generation < manager.interrupt_generation.load(Ordering::SeqCst) || manager.shutdown.load(Ordering::SeqCst) {
                 let _ = socket.close(None);
                 let _ = tx.send(AudioEvent::End);
                 break;
            }

            match socket.read() {
                Ok(Message::Text(msg)) => {
                    if msg.contains("setupComplete") {
                        setup_complete = true;
                        break;
                    }
                    if msg.contains("error") || msg.contains("Error") {
                        eprintln!("TTS: Setup error: {}", msg);
                        break;
                    }
                }
                Ok(Message::Close(_)) => { break; }
                Ok(Message::Binary(data)) => {
                    if let Ok(text) = String::from_utf8(data) {
                        if text.contains("setupComplete") { setup_complete = true; break; }
                    }
                }
                Ok(_) => {}
                Err(tungstenite::Error::Io(ref e)) if e.kind() == std::io::ErrorKind::WouldBlock => {
                     if setup_start.elapsed() > Duration::from_secs(10) { break; }
                     std::thread::sleep(Duration::from_millis(50));
                }
                Err(_) => { break; }
            }
        }
        
        if manager.shutdown.load(Ordering::SeqCst) { return; }
        
        if !setup_complete {
            let _ = socket.close(None);
            let _ = tx.send(AudioEvent::End); 
            continue;
        }
        
        // Send request text
        if let Err(e) = send_tts_text(&mut socket, &request.req.text) {
             eprintln!("TTS: Failed to send text: {}", e);
             let _ = tx.send(AudioEvent::End);
             let _ = socket.close(None);
             continue;
        }
        
        // Read loop
        loop {
            if request.generation < manager.interrupt_generation.load(Ordering::SeqCst) || manager.shutdown.load(Ordering::SeqCst) {
                let _ = socket.close(None);
                let _ = tx.send(AudioEvent::End);
                break;
            }
            
            match socket.read() {
                Ok(Message::Text(msg)) => {
                    if let Some(audio_data) = parse_audio_data(&msg) {
                        let _ = tx.send(AudioEvent::Data(audio_data));
                    }
                    if is_turn_complete(&msg) {
                        let _ = tx.send(AudioEvent::End);
                        break;
                    }
                }
                Ok(Message::Binary(data)) => {
                     if let Ok(text) = String::from_utf8(data) {
                        if let Some(audio_data) = parse_audio_data(&text) {
                             let _ = tx.send(AudioEvent::Data(audio_data));
                        }
                        if is_turn_complete(&text) {
                            let _ = tx.send(AudioEvent::End);
                            break;
                        }
                     }
                }
                Ok(Message::Close(_)) => {
                    let _ = tx.send(AudioEvent::End);
                    break;
                }
                Ok(_) => {}
                Err(tungstenite::Error::Io(ref e)) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(e) => {
                    eprintln!("TTS: Read error: {}", e);
                    let _ = tx.send(AudioEvent::End);
                    break;
                }
            }
        }
        
        let _ = socket.close(None);
    }
}
