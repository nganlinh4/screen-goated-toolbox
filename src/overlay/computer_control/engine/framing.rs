use std::io::BufRead;

use anyhow::{Result, bail};

pub(super) fn read_bounded_line(
    reader: &mut impl BufRead,
    maximum: usize,
) -> Result<Option<Vec<u8>>> {
    let mut line = Vec::new();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return if line.is_empty() {
                Ok(None)
            } else {
                bail!("Computer Control engine response ended before newline")
            };
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let take = newline.map_or(available.len(), |index| index + 1);
        if line.len().saturating_add(take) > maximum {
            bail!("Computer Control engine response exceeds protocol limit");
        }
        line.extend_from_slice(&available[..take]);
        reader.consume(take);
        if newline.is_some() {
            while matches!(line.last(), Some(b'\n' | b'\r')) {
                line.pop();
            }
            return Ok(Some(line));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_oversized_and_incomplete_frames() {
        let mut oversized = vec![b'x'; 32];
        oversized.push(b'\n');
        assert!(read_bounded_line(&mut std::io::Cursor::new(oversized), 32).is_err());
        assert!(read_bounded_line(&mut std::io::Cursor::new(b"partial"), 32).is_err());
    }
}
