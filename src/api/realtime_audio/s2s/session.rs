use super::*;

pub(super) fn session_worker(
    session_index: usize,
    segment_rx: mpsc::Receiver<Segment>,
    event_tx: mpsc::Sender<S2sEvent>,
    stop_signal: Arc<AtomicBool>,
    settings: S2sSettings,
    context_memory: Arc<Mutex<S2sContextMemory>>,
    adaptive_vad: Arc<Mutex<AdaptiveS2sVadState>>,
) {
    let resources = S2sSessionResources {
        event_tx: event_tx.clone(),
        stop_signal: stop_signal.clone(),
        settings,
        context_memory,
        adaptive_vad,
    };
    let mut generation = 0u64;
    while !stop_signal.load(Ordering::SeqCst) {
        match segment_rx.recv_timeout(Duration::from_millis(50)) {
            Ok(segment) => {
                let segment_id = segment.id;
                generation += 1;
                if let Err(err) =
                    run_single_segment_session(session_index, generation, segment, &resources)
                {
                    eprintln!(
                        "[RealtimeS2S] session={session_index} segment={segment_id} error: {err}"
                    );
                    let _ = event_tx.send(S2sEvent::Error {
                        id: segment_id,
                        message: err.to_string(),
                    });
                    std::thread::sleep(Duration::from_millis(250));
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

pub(super) fn run_single_segment_session(
    session_index: usize,
    generation: u64,
    segment: Segment,
    resources: &S2sSessionResources,
) -> Result<()> {
    let context = resources
        .context_memory
        .lock()
        .map(|memory| memory.snapshot())
        .unwrap_or_else(|_| S2sContextSnapshot {
            text: String::new(),
        });
    let outcome = run_hedged_segment_session(
        HedgedSegmentRequest {
            session_index,
            generation,
            segment: segment.clone(),
            context: context.clone(),
            final_attempt: false,
        },
        resources,
    )?;
    observe_adaptive_vad(&resources.adaptive_vad, outcome, &segment);
    if outcome == SegmentOutcome::EmptyNoInput {
        let _ = resources.event_tx.send(S2sEvent::Done { id: segment.id });
    } else if outcome == SegmentOutcome::RetryFresh && !resources.stop_signal.load(Ordering::SeqCst)
    {
        let retry_generation = generation + 1_000_000;
        eprintln!(
            "[RealtimeS2S] retry segment={} session={} gen={} fresh_gen={}",
            segment.id, session_index, generation, retry_generation
        );
        let retry_outcome = run_hedged_segment_session(
            HedgedSegmentRequest {
                session_index,
                generation: retry_generation,
                segment: segment.clone(),
                context,
                final_attempt: true,
            },
            resources,
        )?;
        observe_adaptive_vad(&resources.adaptive_vad, retry_outcome, &segment);
        if matches!(
            retry_outcome,
            SegmentOutcome::RetryFresh | SegmentOutcome::EmptyNoInput
        ) {
            let _ = resources.event_tx.send(S2sEvent::Done { id: segment.id });
        }
    }
    Ok(())
}

fn run_hedged_segment_session(
    request: HedgedSegmentRequest,
    resources: &S2sSessionResources,
) -> Result<SegmentOutcome> {
    const HEDGE_ATTEMPTS: usize = 2;
    let HedgedSegmentRequest {
        session_index,
        generation,
        segment,
        context,
        final_attempt,
    } = request;

    let (race_tx, race_rx) = mpsc::channel::<S2sRaceEvent>();
    let mut cancel_flags = Vec::with_capacity(HEDGE_ATTEMPTS);
    let mut saw_audio = [false; HEDGE_ATTEMPTS];
    let mut saw_input_text = [false; HEDGE_ATTEMPTS];
    let mut saw_output_text = [false; HEDGE_ATTEMPTS];
    let mut finished = [false; HEDGE_ATTEMPTS];
    let mut buffered_events = [Vec::<S2sEvent>::new(), Vec::<S2sEvent>::new()];
    let mut winner: Option<usize> = None;
    let started = Instant::now();
    let source_audio_ms = samples_to_ms(segment.samples.len()) as u128;
    let hard_timeout_ms = grouped_hard_timeout_ms(source_audio_ms, final_attempt);

    for attempt in 0..HEDGE_ATTEMPTS {
        let attempt_generation = generation + (attempt as u64 * 100_000);
        let attempt_cancel = Arc::new(AtomicBool::new(false));
        cancel_flags.push(attempt_cancel.clone());
        spawn_hedged_attempt(
            HedgedAttemptRequest {
                session_index,
                attempt,
                generation: attempt_generation,
                segment: segment.clone(),
                context: context.clone(),
                final_attempt,
            },
            HedgedAttemptResources {
                settings: resources.settings.clone(),
                stop_signal: resources.stop_signal.clone(),
                cancel_signal: attempt_cancel,
                race_tx: race_tx.clone(),
            },
        );
    }
    drop(race_tx);

    while !resources.stop_signal.load(Ordering::SeqCst) {
        if started.elapsed().as_millis() >= hard_timeout_ms {
            for cancel in &cancel_flags {
                cancel.store(true, Ordering::SeqCst);
            }
            eprintln!(
                "[RealtimeS2S][Segment] timeout id={} session={} gen={} elapsed_ms={} timeout_ms={} audio_ms={} speech_ratio={:.2} peak_rms={:.4} peak_sample={:.4} winner={:?} saw_audio={:?} finished={:?} events={} final_attempt={}",
                segment.id,
                session_index,
                generation,
                started.elapsed().as_millis(),
                hard_timeout_ms,
                segment_audio_ms(&segment),
                segment_speech_ratio(&segment),
                segment.peak_rms,
                segment_peak_sample(&segment),
                winner,
                saw_audio,
                finished,
                format_s2s_attempt_counts(&buffered_events),
                final_attempt
            );
            if let Some(attempt) = winner
                && saw_audio[attempt]
            {
                let _ = resources.event_tx.send(S2sEvent::Done { id: segment.id });
                return Ok(SegmentOutcome::Healthy);
            }
            let (input_text_events, output_text_events, audio_events) =
                s2s_attempt_counts(&buffered_events);
            if input_text_events == 0 && output_text_events == 0 && audio_events == 0 {
                if final_attempt {
                    let _ = resources.event_tx.send(S2sEvent::Done { id: segment.id });
                }
                return Ok(SegmentOutcome::EmptyNoInput);
            }
            if final_attempt {
                let _ = resources.event_tx.send(S2sEvent::Done { id: segment.id });
            }
            return Ok(SegmentOutcome::RetryFresh);
        }
        match race_rx.recv_timeout(Duration::from_millis(50)) {
            Ok(S2sRaceEvent::Event { attempt, event }) => {
                if matches!(event, S2sEvent::Audio { .. }) {
                    saw_audio[attempt] = true;
                } else if matches!(event, S2sEvent::InputText { .. }) {
                    saw_input_text[attempt] = true;
                } else if matches!(event, S2sEvent::OutputText { .. }) {
                    saw_output_text[attempt] = true;
                }

                if matches!(event, S2sEvent::Audio { .. }) && winner.is_none() {
                    winner = Some(attempt);
                    for (index, cancel) in cancel_flags.iter().enumerate() {
                        if index != attempt {
                            cancel.store(true, Ordering::SeqCst);
                        }
                    }
                    for buffered in buffered_events[attempt].drain(..) {
                        let _ = resources.event_tx.send(buffered);
                    }
                }

                if winner == Some(attempt) {
                    let done = matches!(event, S2sEvent::Done { .. } | S2sEvent::Error { .. });
                    let _ = resources.event_tx.send(event);
                    if done {
                        return Ok(if saw_audio[attempt] {
                            SegmentOutcome::Healthy
                        } else if !saw_input_text[attempt] && !saw_output_text[attempt] {
                            SegmentOutcome::EmptyNoInput
                        } else {
                            SegmentOutcome::RetryFresh
                        });
                    }
                } else if winner.is_none() {
                    buffered_events[attempt].push(event);
                }
            }
            Ok(S2sRaceEvent::Finished { attempt, outcome }) => {
                finished[attempt] = true;
                if winner == Some(attempt) {
                    continue;
                }
                if winner.is_none() && outcome == SegmentOutcome::Healthy && saw_audio[attempt] {
                    winner = Some(attempt);
                    for (index, cancel) in cancel_flags.iter().enumerate() {
                        if index != attempt {
                            cancel.store(true, Ordering::SeqCst);
                        }
                    }
                    for buffered in buffered_events[attempt].drain(..) {
                        let _ = resources.event_tx.send(buffered);
                    }
                    continue;
                }
                if finished.iter().all(|done| *done) && winner.is_none() {
                    let (input_text_events, output_text_events, audio_events) =
                        s2s_attempt_counts(&buffered_events);
                    eprintln!(
                        "[RealtimeS2S][Segment] empty id={} session={} gen={} elapsed_ms={} attempts={} audio_ms={} speech_ratio={:.2} peak_rms={:.4} peak_sample={:.4} events={} final_attempt={}",
                        segment.id,
                        session_index,
                        generation,
                        started.elapsed().as_millis(),
                        HEDGE_ATTEMPTS,
                        segment_audio_ms(&segment),
                        segment_speech_ratio(&segment),
                        segment.peak_rms,
                        segment_peak_sample(&segment),
                        format_s2s_attempt_counts(&buffered_events),
                        final_attempt
                    );
                    if input_text_events == 0 && output_text_events == 0 && audio_events == 0 {
                        return Ok(SegmentOutcome::EmptyNoInput);
                    }
                    return Ok(SegmentOutcome::RetryFresh);
                }
            }
            Ok(S2sRaceEvent::Error { attempt, message }) => {
                finished[attempt] = true;
                eprintln!(
                    "[RealtimeS2S] hedge-attempt-error segment={} session={} gen={} attempt={} error={}",
                    segment.id, session_index, generation, attempt, message
                );
                if winner == Some(attempt) {
                    let _ = resources.event_tx.send(S2sEvent::Error {
                        id: segment.id,
                        message,
                    });
                    return Ok(SegmentOutcome::RetryFresh);
                }
                if finished.iter().all(|done| *done) && winner.is_none() {
                    return Ok(SegmentOutcome::RetryFresh);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    Ok(SegmentOutcome::RetryFresh)
}

fn spawn_hedged_attempt(request: HedgedAttemptRequest, resources: HedgedAttemptResources) {
    let HedgedAttemptRequest {
        session_index,
        attempt,
        generation,
        segment,
        context,
        final_attempt,
    } = request;
    let HedgedAttemptResources {
        settings,
        stop_signal,
        cancel_signal,
        race_tx,
    } = resources;
    std::thread::spawn(move || {
        let (attempt_tx, attempt_rx) = mpsc::channel::<S2sEvent>();
        let forward_tx = race_tx.clone();
        let forwarder = std::thread::spawn(move || {
            while let Ok(event) = attempt_rx.recv() {
                if forward_tx
                    .send(S2sRaceEvent::Event { attempt, event })
                    .is_err()
                {
                    break;
                }
            }
        });

        let outcome = match open_fresh_socket_session(
            session_index,
            generation,
            &settings,
            &context,
            &stop_signal,
        ) {
            Ok(mut socket) => {
                let result = process_segment(
                    &mut socket,
                    &segment,
                    ProcessSegmentParams {
                        session_index,
                        generation,
                        event_tx: &attempt_tx,
                        stop_signal: &stop_signal,
                        cancel_signal: Some(&cancel_signal),
                        final_attempt,
                    },
                );
                let _ = socket.close(None);
                result
            }
            Err(err) => Err(err),
        };
        drop(attempt_tx);
        let _ = forwarder.join();

        match outcome {
            Ok(outcome) => {
                let _ = race_tx.send(S2sRaceEvent::Finished { attempt, outcome });
            }
            Err(err) => {
                let _ = race_tx.send(S2sRaceEvent::Error {
                    attempt,
                    message: err.to_string(),
                });
            }
        }
    });
}
