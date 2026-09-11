//! Opt-in local replay of public provider captures through the production reducer.
use super::*;

#[test]
#[ignore = "requires explicitly supplied public provider capture"]
fn public_transcription_delivery_replay() -> anyhow::Result<()> {
    let input = std::fs::read_to_string(std::env::var("SGT_PUBLIC_TRANSCRIPT_LOG")?)?;
    let mut state = TranscriptState::default();
    let mut reference = String::new();
    let (mut total, mut maximum, mut frames, mut withheld) = (0, 0, 0, 0);
    for line in input.lines() {
        let Some((_, json)) = line.split_once("PROBE_FRAME ") else {
            continue;
        };
        let event: serde_json::Value = serde_json::from_str(json)?;
        let interim = event["interim"].as_str();
        let final_text = event["final"].as_str();
        if interim.is_none() && final_text.is_none() {
            continue;
        }
        let old = state.display();
        state.apply_update(interim, final_text);
        if let Some(text) = final_text {
            append_segment(&mut reference, text);
        }
        if interim.is_some() && final_text.is_none() && state.interim().is_empty() {
            withheld += 1;
        }
        let new = state.display();
        let prefix = old
            .chars()
            .zip(new.chars())
            .take_while(|(a, b)| a == b)
            .count();
        let erased = old.chars().count() - prefix;
        anyhow::ensure!(
            erased <= stabilization::REVISION_SCALARS,
            "erase limit exceeded at frame {frames}: {erased}"
        );
        total += erased;
        maximum = maximum.max(erased);
        frames += 1;
    }
    let before = state.display();
    state.finish_pending();
    anyhow::ensure!(state.display() == before, "conclude changed delivered text");
    println!(
        "DELIVERY_REPLAY {}",
        serde_json::json!({
            "frames": frames, "withheld_frames": withheld, "total_erased": total,
            "maximum_erased": maximum, "delivered": state.committed(), "provider_final": reference
        })
    );
    Ok(())
}
