use anyhow::{Context, Result, bail};
use std::io::{Seek, Write};
use std::time::Duration;

pub(super) struct AudioPacket {
    pub start_frame: u64,
    pub samples: Vec<i16>,
}

pub(super) struct Timeline {
    sample_rate: u32,
    channels: usize,
    written_frames: u64,
    last_packet_start: Option<u64>,
}

pub(super) fn duration_frames(elapsed: Duration, sample_rate: u32) -> u64 {
    u64::try_from(elapsed.as_nanos().saturating_mul(u128::from(sample_rate)) / 1_000_000_000)
        .unwrap_or(u64::MAX)
}

impl Timeline {
    pub fn new(sample_rate: u32, channels: usize) -> Self {
        Self {
            sample_rate,
            channels,
            written_frames: 0,
            last_packet_start: None,
        }
    }

    pub fn write_packet_until<W: Write + Seek>(
        &mut self,
        writer: &mut hound::WavWriter<W>,
        packet: AudioPacket,
        final_frame: Option<u64>,
    ) -> Result<()> {
        if self.channels == 0 || !packet.samples.len().is_multiple_of(self.channels) {
            bail!("invalid system-output audio packet");
        }
        if final_frame.is_some_and(|end| packet.start_frame >= end) {
            return Ok(());
        }
        // A small backwards timestamp may be clock-mapping jitter, but a
        // larger regression is not safe to treat as an ordinary overlap.
        let tolerance = u64::from((self.sample_rate / 200).max(1));
        if self
            .last_packet_start
            .is_some_and(|last| packet.start_frame.saturating_add(tolerance) < last)
        {
            bail!("system-output capture timestamp moved backwards");
        }
        if packet.start_frame.saturating_add(tolerance) < self.written_frames {
            bail!("system-output capture packets overlap beyond clock tolerance");
        }
        self.last_packet_start = Some(packet.start_frame);
        let start = packet.start_frame;
        let frames = ((packet.samples.len() / self.channels) as u64)
            .min(final_frame.map_or(u64::MAX, |end| end.saturating_sub(start)));
        let gap = start.saturating_sub(self.written_frames);
        let skip = self.written_frames.saturating_sub(start).min(frames);
        self.ensure_wav_capacity(gap.saturating_add(frames.saturating_sub(skip)))?;
        self.write_silence(writer, gap)?;
        for sample in packet
            .samples
            .iter()
            .take(frames as usize * self.channels)
            .skip(skip as usize * self.channels)
        {
            writer
                .write_sample(*sample)
                .context("write system-output audio sample")?;
        }
        self.written_frames = self.written_frames.saturating_add(frames - skip);
        Ok(())
    }

    pub fn finish<W: Write + Seek>(
        &mut self,
        writer: &mut hound::WavWriter<W>,
        final_frame: u64,
    ) -> Result<()> {
        if self.written_frames > final_frame {
            bail!("system-output audio exceeded the recording stop time");
        }
        let silence = final_frame - self.written_frames;
        self.ensure_wav_capacity(silence)?;
        self.write_silence(writer, silence)
    }

    fn ensure_wav_capacity(&self, additional_frames: u64) -> Result<()> {
        let bytes = self
            .written_frames
            .checked_add(additional_frames)
            .and_then(|frames| frames.checked_mul(self.channels as u64))
            .and_then(|samples| samples.checked_mul(2))
            .context("system-output WAV length overflow")?;
        // RIFF's u32 chunk length also includes the format/header bytes.
        if bytes > u64::from(u32::MAX - 36) {
            bail!("system-output WAV exceeds its supported length");
        }
        Ok(())
    }

    fn write_silence<W: Write + Seek>(
        &mut self,
        writer: &mut hound::WavWriter<W>,
        frames: u64,
    ) -> Result<()> {
        for _ in 0..frames.saturating_mul(self.channels as u64) {
            writer
                .write_sample(0i16)
                .context("write system-output audio silence")?;
        }
        self.written_frames = self.written_frames.saturating_add(frames);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn gaps_initial_delay_overlap_and_tail_follow_the_recording_clock() {
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut output = Cursor::new(Vec::new());
        let mut writer = hound::WavWriter::new(&mut output, spec).unwrap();
        let mut timeline = Timeline::new(100, 2);
        timeline
            .write_packet_until(
                &mut writer,
                AudioPacket {
                    start_frame: 300,
                    samples: vec![1, 2, 3, 4],
                },
                None,
            )
            .unwrap();
        timeline
            .write_packet_until(
                &mut writer,
                AudioPacket {
                    start_frame: 500,
                    samples: vec![5, 6, 7, 8],
                },
                None,
            )
            .unwrap();
        // A packet beginning one frame behind the written cursor only adds its
        // unseen second frame, never compressing or duplicating time.
        timeline
            .write_packet_until(
                &mut writer,
                AudioPacket {
                    start_frame: 501,
                    samples: vec![7, 8, 9, 10],
                },
                None,
            )
            .unwrap();
        timeline.finish(&mut writer, 600).unwrap();
        writer.finalize().unwrap();
        output.set_position(0);
        let samples = hound::WavReader::new(output)
            .unwrap()
            .samples::<i16>()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(samples.len(), 1200);
        assert!(samples[..600].iter().all(|sample| *sample == 0));
        assert_eq!(&samples[600..604], &[1, 2, 3, 4]);
        assert!(samples[604..1000].iter().all(|sample| *sample == 0));
        assert_eq!(&samples[1000..1006], &[5, 6, 7, 8, 9, 10]);
        assert!(samples[1006..].iter().all(|sample| *sample == 0));
    }

    #[test]
    fn rejects_wav_overflow_without_writing_or_allocating_silence() {
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::new(Cursor::new(Vec::new()), spec).unwrap();
        let mut timeline = Timeline::new(48_000, 2);
        assert!(timeline.finish(&mut writer, u64::MAX).is_err());
        assert_eq!(timeline.written_frames, 0);
    }

    #[test]
    fn delayed_stop_drains_only_frames_before_exact_stop() {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut output = Cursor::new(Vec::new());
        let mut writer = hound::WavWriter::new(&mut output, spec).unwrap();
        let mut timeline = Timeline::new(100, 1);
        timeline
            .write_packet_until(
                &mut writer,
                AudioPacket {
                    start_frame: 2,
                    samples: vec![1, 2, 3, 4],
                },
                Some(4),
            )
            .unwrap();
        timeline
            .write_packet_until(
                &mut writer,
                AudioPacket {
                    start_frame: 4,
                    samples: vec![9, 9],
                },
                Some(4),
            )
            .unwrap();
        timeline.finish(&mut writer, 4).unwrap();
        writer.finalize().unwrap();
        output.set_position(0);
        let samples = hound::WavReader::new(output)
            .unwrap()
            .samples::<i16>()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(samples, vec![0, 0, 1, 2]);
    }

    #[test]
    fn large_backward_timestamp_fails_without_discarding_as_overlap() {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::new(Cursor::new(Vec::new()), spec).unwrap();
        let mut timeline = Timeline::new(100, 1);
        timeline
            .write_packet_until(
                &mut writer,
                AudioPacket {
                    start_frame: 30,
                    samples: vec![1, 2],
                },
                None,
            )
            .unwrap();
        assert!(
            timeline
                .write_packet_until(
                    &mut writer,
                    AudioPacket {
                        start_frame: 1,
                        samples: vec![3, 4]
                    },
                    None
                )
                .is_err()
        );
        assert_eq!(timeline.written_frames, 32);
    }

    #[test]
    fn frozen_or_slow_clock_does_not_discard_a_packet_as_overlap() {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::new(Cursor::new(Vec::new()), spec).unwrap();
        let mut timeline = Timeline::new(48_000, 1);
        timeline
            .write_packet_until(
                &mut writer,
                AudioPacket {
                    start_frame: 0,
                    samples: vec![1; 480],
                },
                None,
            )
            .unwrap();
        assert!(
            timeline
                .write_packet_until(
                    &mut writer,
                    AudioPacket {
                        start_frame: 1,
                        samples: vec![2; 480]
                    },
                    None
                )
                .is_err()
        );
        assert_eq!(timeline.written_frames, 480);
    }

    #[test]
    fn wav_write_failure_is_returned_instead_of_reporting_complete_audio() {
        use std::cell::Cell;
        use std::io::{self, SeekFrom};
        use std::rc::Rc;

        struct FailingWriter {
            data: Cursor<Vec<u8>>,
            fail: Rc<Cell<bool>>,
        }
        impl Write for FailingWriter {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                if self.fail.get() {
                    return Err(io::Error::other("simulated disk failure"));
                }
                self.data.write(bytes)
            }
            fn flush(&mut self) -> io::Result<()> {
                self.data.flush()
            }
        }
        impl Seek for FailingWriter {
            fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
                self.data.seek(position)
            }
        }
        let fail = Rc::new(Cell::new(false));
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::new(
            FailingWriter {
                data: Cursor::new(Vec::new()),
                fail: fail.clone(),
            },
            spec,
        )
        .unwrap();
        let mut timeline = Timeline::new(100, 1);
        fail.set(true);
        assert!(
            timeline
                .write_packet_until(
                    &mut writer,
                    AudioPacket {
                        start_frame: 0,
                        samples: vec![1, 2]
                    },
                    None
                )
                .is_err()
        );
        assert_eq!(timeline.written_frames, 0);
        fail.set(false);
    }
}
