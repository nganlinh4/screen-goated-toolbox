//! One `IMFSourceReader::ReadSample` contract for every decoder: a null sample
//! without end-of-stream (a stream tick or gap) is skipped, never mistaken for
//! the end of the file, and a reported output media type change is handed to
//! the decoder before any sample of the new type is returned.

use windows::Win32::Media::MediaFoundation::*;

/// Consecutive empty reads tolerated before the stream is treated as stalled.
const MAX_EMPTY_READS: u32 = 4096;

/// One raw `ReadSample` result: stream flags, the sample if any, and its time.
type RawRead<S> = (u32, Option<S>, i64);

fn has_flag(stream_flags: u32, flag: MF_SOURCE_READER_FLAG) -> bool {
    (stream_flags & flag.0 as u32) != 0
}

/// The read loop behind [`read_stream_sample`], over any sample source.
///
/// `on_media_type_changed` runs whenever the reader reports
/// `MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED`, before the sample that carries
/// the flag is returned, so the caller re-reads the output type instead of
/// misinterpreting the new samples with the old one.
fn read_next<S>(
    mut read: impl FnMut() -> Result<RawRead<S>, String>,
    mut on_media_type_changed: impl FnMut() -> Result<(), String>,
) -> Result<Option<(S, i64)>, String> {
    for _ in 0..MAX_EMPTY_READS {
        let (stream_flags, sample, timestamp) = read()?;
        if has_flag(stream_flags, MF_SOURCE_READERF_ENDOFSTREAM) {
            return Ok(None);
        }
        if has_flag(stream_flags, MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED) {
            on_media_type_changed()?;
        }
        if let Some(sample) = sample {
            return Ok(Some((sample, timestamp)));
        }
    }
    Err(format!(
        "Media stream produced no sample after {MAX_EMPTY_READS} consecutive reads"
    ))
}

/// Returns the next sample and its timestamp, or `None` at end of stream.
pub(crate) fn read_stream_sample(
    reader: &IMFSourceReader,
    stream_index: u32,
    on_media_type_changed: impl FnMut() -> Result<(), String>,
) -> Result<Option<(IMFSample, i64)>, String> {
    read_next(
        || {
            let mut stream_flags: u32 = 0;
            let mut timestamp: i64 = 0;
            let mut sample: Option<IMFSample> = None;
            unsafe {
                reader
                    .ReadSample(
                        stream_index,
                        0,
                        None,
                        Some(&mut stream_flags),
                        Some(&mut timestamp),
                        Some(&mut sample),
                    )
                    .map_err(|e| format!("ReadSample: {e}"))?;
            }
            Ok((stream_flags, sample, timestamp))
        },
        on_media_type_changed,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const TICK: u32 = MF_SOURCE_READERF_STREAMTICK.0 as u32;
    const EOS: u32 = MF_SOURCE_READERF_ENDOFSTREAM.0 as u32;
    const TYPE_CHANGED: u32 = MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0 as u32;

    fn scripted(reads: Vec<RawRead<u8>>) -> impl FnMut() -> Result<RawRead<u8>, String> {
        let mut reads = reads.into_iter();
        move || Ok(reads.next().unwrap_or((EOS, None, 0)))
    }

    #[test]
    fn stream_tick_without_sample_is_a_gap_not_end_of_file() {
        let read = scripted(vec![(TICK, None, 0), (0, None, 0), (TICK, Some(7), 40)]);
        assert_eq!(read_next(read, || Ok(())), Ok(Some((7, 40))));

        let read = scripted(vec![(EOS | TICK, Some(7), 0)]);
        assert_eq!(read_next(read, || Ok(())), Ok(None));
    }

    #[test]
    fn media_type_change_is_handled_before_the_new_sample_is_returned() {
        let mut changes = 0;
        let mut read = scripted(vec![(0, Some(1), 0), (TYPE_CHANGED, Some(2), 10)]);
        assert_eq!(read_next(&mut read, || Ok(())), Ok(Some((1, 0))));
        let result = read_next(&mut read, || {
            changes += 1;
            Ok(())
        });
        assert_eq!(result, Ok(Some((2, 10))));
        assert_eq!(changes, 1);

        // A type the decoder cannot follow stops decoding with that reason
        // instead of handing out samples it would misread.
        let read = scripted(vec![(TYPE_CHANGED, Some(3), 0)]);
        let result = read_next(read, || Err("audio format changed".to_string()));
        assert_eq!(result, Err("audio format changed".to_string()));
    }
}
