//! Streaming variants of `align` / `unalign` for files too large to hold in
//! memory.
//!
//! - [`stream_unalign`]: single-pass over any `Read`. Strips trailing spaces
//!   per field.
//! - [`stream_align`]: two-pass over `Read + Seek`. First pass computes
//!   column widths, second pass emits padded rows. Stdin and other
//!   non-seekable inputs must buffer.

use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};

const READ_CHUNK: usize = 64 * 1024;

#[derive(Copy, Clone, PartialEq)]
enum State {
    FieldStart,
    Unquoted,
    Quoted,
    AfterClosingQuote,
}

/// Events emitted by the streaming parser.
pub(crate) enum Event<'a> {
    Field(&'a str),
    /// `had_trailing_newline` is true when the row ended with `\n` / `\r\n` /
    /// `\r` in the source. False means we flushed a final row at EOF.
    RowEnd(bool),
}

/// Drive a state-machine reader over `r`, dispatching [`Event`]s to `sink`.
///
/// A single-callback shape keeps the caller's captured state behind one
/// mutable borrow — two-closure designs trip the borrow checker when both
/// closures touch the same counter.
fn drive<R, F>(mut r: R, mut sink: F) -> io::Result<()>
where
    R: Read,
    F: FnMut(Event<'_>) -> io::Result<()>,
{
    let mut buf = [0u8; READ_CHUNK];
    let mut state = State::FieldStart;
    let mut field = String::new();
    let mut pending_cr = false;
    let mut row_dirty = false;

    loop {
        let n = r.read(&mut buf)?;
        if n == 0 {
            break;
        }
        let mut s = std::str::from_utf8(&buf[..n]).map_err(|e| {
            io::Error::new(io::ErrorKind::InvalidData, format!("invalid utf-8: {e}"))
        })?;

        // Carry a pending lone `\r` into this chunk: collapse `\r\n`, else
        // treat it as a bare line break.
        if pending_cr {
            pending_cr = false;
            if let Some(rest) = s.strip_prefix('\n') {
                s = rest;
            }
            sink(Event::Field(&std::mem::take(&mut field)))?;
            sink(Event::RowEnd(true))?;
            state = State::FieldStart;
            row_dirty = false;
        }

        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            match state {
                State::FieldStart => match c {
                    '"' => {
                        field.push('"');
                        state = State::Quoted;
                        row_dirty = true;
                    }
                    ',' => {
                        sink(Event::Field(&std::mem::take(&mut field)))?;
                        row_dirty = true;
                    }
                    '\n' => {
                        sink(Event::Field(&std::mem::take(&mut field)))?;
                        sink(Event::RowEnd(true))?;
                        row_dirty = false;
                    }
                    '\r' => {
                        if chars.peek() == Some(&'\n') {
                            chars.next();
                            sink(Event::Field(&std::mem::take(&mut field)))?;
                            sink(Event::RowEnd(true))?;
                            row_dirty = false;
                        } else if chars.peek().is_none() {
                            pending_cr = true;
                        } else {
                            sink(Event::Field(&std::mem::take(&mut field)))?;
                            sink(Event::RowEnd(true))?;
                            row_dirty = false;
                        }
                    }
                    _ => {
                        field.push(c);
                        state = State::Unquoted;
                        row_dirty = true;
                    }
                },
                State::Unquoted | State::AfterClosingQuote => match c {
                    ',' => {
                        sink(Event::Field(&std::mem::take(&mut field)))?;
                        state = State::FieldStart;
                    }
                    '\n' => {
                        sink(Event::Field(&std::mem::take(&mut field)))?;
                        sink(Event::RowEnd(true))?;
                        state = State::FieldStart;
                        row_dirty = false;
                    }
                    '\r' => {
                        if chars.peek() == Some(&'\n') {
                            chars.next();
                            sink(Event::Field(&std::mem::take(&mut field)))?;
                            sink(Event::RowEnd(true))?;
                            state = State::FieldStart;
                            row_dirty = false;
                        } else if chars.peek().is_none() {
                            pending_cr = true;
                        } else {
                            sink(Event::Field(&std::mem::take(&mut field)))?;
                            sink(Event::RowEnd(true))?;
                            state = State::FieldStart;
                            row_dirty = false;
                        }
                    }
                    _ => field.push(c),
                },
                State::Quoted => {
                    if c == '"' {
                        field.push('"');
                        if chars.peek() == Some(&'"') {
                            field.push(chars.next().unwrap());
                        } else {
                            state = State::AfterClosingQuote;
                        }
                    } else {
                        field.push(c);
                    }
                }
            }
        }
    }

    if pending_cr {
        sink(Event::Field(&std::mem::take(&mut field)))?;
        sink(Event::RowEnd(true))?;
    } else if row_dirty || !field.is_empty() {
        sink(Event::Field(&std::mem::take(&mut field)))?;
        sink(Event::RowEnd(false))?;
    }
    Ok(())
}

/// Strip trailing spaces from every field. Single-pass; accepts any `Read`.
///
/// Mirrors [`crate::unalign`] trailing-newline behavior: if the source ended
/// without a newline the output won't have one either.
pub fn stream_unalign<R: Read, W: Write>(r: R, mut w: W) -> io::Result<()> {
    let r = BufReader::new(r);
    let mut first_in_row = true;
    let mut pending_newline = false;
    drive(r, |ev| -> io::Result<()> {
        match ev {
            Event::Field(field) => {
                if pending_newline {
                    w.write_all(b"\n")?;
                    pending_newline = false;
                }
                if !first_in_row {
                    w.write_all(b",")?;
                }
                w.write_all(field.trim_end_matches(' ').as_bytes())?;
                first_in_row = false;
            }
            Event::RowEnd(had_newline) => {
                if had_newline {
                    w.write_all(b"\n")?;
                    pending_newline = false;
                } else {
                    // Final row with no source newline: suppress the terminator.
                    pending_newline = false;
                }
                first_in_row = true;
            }
        }
        Ok(())
    })?;
    w.flush()
}

/// Two-pass align over a seekable source. Byte-identical to [`crate::align`]
/// for well-formed CSV.
pub fn stream_align<RS: Read + Seek, W: Write>(mut r: RS, mut w: W) -> io::Result<()> {
    // Pass 1: per-column max widths (in chars) + track trailing-newline.
    let mut widths: Vec<usize> = Vec::new();
    let mut col_idx: usize = 0;
    let mut had_trailing_newline = true;
    drive(BufReader::new(&mut r), |ev| -> io::Result<()> {
        match ev {
            Event::Field(field) => {
                let cw = field.chars().count();
                if col_idx >= widths.len() {
                    widths.resize(col_idx + 1, 0);
                }
                if cw > widths[col_idx] {
                    widths[col_idx] = cw;
                }
                col_idx += 1;
            }
            Event::RowEnd(had_newline) => {
                col_idx = 0;
                had_trailing_newline = had_newline;
            }
        }
        Ok(())
    })?;

    // Pass 2: re-read and emit, holding back the final row's newline so we
    // can honour `had_trailing_newline`.
    r.seek(SeekFrom::Start(0))?;
    let mut col_idx: usize = 0;
    let mut first_in_row = true;
    let mut pending_newline = false;
    drive(BufReader::new(&mut r), |ev| -> io::Result<()> {
        match ev {
            Event::Field(field) => {
                if pending_newline {
                    w.write_all(b"\n")?;
                    pending_newline = false;
                }
                if !first_in_row {
                    w.write_all(b",")?;
                }
                w.write_all(field.as_bytes())?;
                let want = *widths.get(col_idx).unwrap_or(&0);
                let have = field.chars().count();
                for _ in 0..want.saturating_sub(have) {
                    w.write_all(b" ")?;
                }
                first_in_row = false;
                col_idx += 1;
            }
            Event::RowEnd(_) => {
                pending_newline = true;
                first_in_row = true;
                col_idx = 0;
            }
        }
        Ok(())
    })?;
    if pending_newline && had_trailing_newline {
        w.write_all(b"\n")?;
    }
    w.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_unalign(input: &str) -> String {
        let mut out = Vec::new();
        stream_unalign(input.as_bytes(), &mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    fn run_align(input: &str) -> String {
        let mut out = Vec::new();
        let cursor = std::io::Cursor::new(input.as_bytes().to_vec());
        stream_align(cursor, &mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn unalign_simple() {
        let aligned = "id,name \n1 ,Alice\n22,Bob  \n";
        assert_eq!(run_unalign(aligned), crate::unalign(aligned));
    }

    #[test]
    fn unalign_no_trailing_newline() {
        let aligned = "id,name \n1 ,Alice\n22,Bob  ";
        assert_eq!(run_unalign(aligned), crate::unalign(aligned));
    }

    #[test]
    fn align_simple() {
        let input = "id,name\n1,Alice\n22,Bob\n";
        assert_eq!(run_align(input), crate::align(input));
    }

    #[test]
    fn align_no_trailing_newline() {
        let input = "id,name\n1,Alice\n22,Bob";
        assert_eq!(run_align(input), crate::align(input));
    }

    #[test]
    fn align_quoted_with_comma() {
        let input = "a,\"b,c\"\nxx,y\n";
        assert_eq!(run_align(input), crate::align(input));
    }

    #[test]
    fn align_embedded_newline_in_quoted() {
        let input = "\"a\nb\",c\nxx,yy\n";
        assert_eq!(run_align(input), crate::align(input));
    }

    #[test]
    fn align_crlf() {
        let input = "a,b\r\nc,d\r\n";
        assert_eq!(run_align(input), crate::align(input));
    }

    #[test]
    fn unalign_ragged_rows() {
        let input = "a,b,c\nd,e\nf\n";
        assert_eq!(run_unalign(input), crate::unalign(input));
    }

    #[test]
    fn align_empty_fields() {
        let input = "a,,c\n,,\nx,y,z\n";
        assert_eq!(run_align(input), crate::align(input));
    }

    #[test]
    fn align_escaped_quotes() {
        let input = "\"a\"\"b\",c\nx,y\n";
        assert_eq!(run_align(input), crate::align(input));
    }
}
