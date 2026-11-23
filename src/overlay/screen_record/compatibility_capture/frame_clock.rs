use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Maps the capture filter's absolute microsecond epoch onto our monotonic clock.
pub(super) struct FrameClock {
    monotonic: Instant,
    unix_us: u128,
}

impl FrameClock {
    pub(super) fn now() -> anyhow::Result<Self> {
        let before = Instant::now();
        let unix_us = SystemTime::now().duration_since(UNIX_EPOCH)?.as_micros();
        let after = Instant::now();
        anyhow::ensure!(
            after.duration_since(before) < Duration::from_millis(5),
            "capture epoch calibration interrupted"
        );
        Ok(Self {
            monotonic: before + after.duration_since(before) / 2,
            unix_us,
        })
    }

    pub(super) fn first_frame(&self, line: &str) -> Option<Instant> {
        if !line.contains("Parsed_showinfo_") || number(line, " n:")? != 0 {
            return None;
        }
        let pts_us = number(line, " pts:")?;
        // Reject foreign clock domains and wall-clock jumps during setup.
        let elapsed = pts_us.checked_sub(self.unix_us)?;
        if elapsed > Duration::from_secs(30).as_micros() {
            return None;
        }
        self.monotonic
            .checked_add(Duration::from_micros(elapsed.try_into().ok()?))
    }
}

fn number(line: &str, field: &str) -> Option<u128> {
    line.split_once(field)?
        .1
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_captured_frame_excludes_process_startup_and_encoder_delivery_latency() {
        let start = Instant::now();
        let clock = FrameClock {
            monotonic: start,
            unix_us: 1_000_000,
        };
        let origin = clock
            .first_frame("[Parsed_showinfo_1 @ x] n:   0 pts:1350000 pts_time:1.350000")
            .unwrap();
        assert_eq!(origin.duration_since(start), Duration::from_millis(350));
        assert!(
            clock
                .first_frame("[Parsed_showinfo_1 @ x] n: 1 pts:1400000")
                .is_none()
        );
    }

    #[test]
    fn rejects_zero_relative_epochs_future_clock_jumps_and_other_diagnostics() {
        let clock = FrameClock {
            monotonic: Instant::now(),
            unix_us: 1_000_000,
        };
        for line in [
            "[Parsed_showinfo_1 @ x] n: 0 pts:0",
            "[Parsed_showinfo_1 @ x] n: 0 pts:32000000",
            "n: 0 pts:1350000",
            "[Parsed_showinfo_1 @ x] config in time_base:1/1000000",
        ] {
            assert!(clock.first_frame(line).is_none(), "{line}");
        }
    }
}
