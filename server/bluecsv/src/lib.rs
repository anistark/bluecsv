//! CSV transforms used by the Blue CSV Zed extension.

pub mod stats;
pub mod stream;
pub mod types;

pub use stats::{summarize, ColumnStats};
pub use types::{classify_cell, infer_column, infer_table, CellType, ColumnType};

enum State {
    FieldStart,
    Unquoted,
    Quoted,
    AfterClosingQuote,
}

/// Parses `input` into rows of raw field text. Quoted fields keep their
/// surrounding quotes and any escaped inner quotes; whitespace between a
/// closing quote and the next delimiter is preserved so `unalign` can
/// round-trip a previously-aligned buffer.
pub fn parse(input: &str) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut state = State::FieldStart;
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        match state {
            State::FieldStart => match c {
                '"' => {
                    field.push('"');
                    state = State::Quoted;
                }
                ',' => {
                    row.push(std::mem::take(&mut field));
                }
                '\n' => {
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                }
                '\r' => {
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                }
                _ => {
                    field.push(c);
                    state = State::Unquoted;
                }
            },
            State::Unquoted => match c {
                ',' => {
                    row.push(std::mem::take(&mut field));
                    state = State::FieldStart;
                }
                '\n' => {
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                    state = State::FieldStart;
                }
                '\r' => {
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                    state = State::FieldStart;
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
            State::AfterClosingQuote => match c {
                ',' => {
                    row.push(std::mem::take(&mut field));
                    state = State::FieldStart;
                }
                '\n' => {
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                    state = State::FieldStart;
                }
                '\r' => {
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                    state = State::FieldStart;
                }
                _ => field.push(c),
            },
        }
    }

    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }

    rows
}

fn column_widths(rows: &[Vec<String>]) -> Vec<usize> {
    let max_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    (0..max_cols)
        .map(|col| {
            rows.iter()
                .filter_map(|r| r.get(col))
                .map(|f| f.chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect()
}

/// Pads every field with trailing spaces so columns line up.
pub fn align(input: &str) -> String {
    let rows = parse(input);
    let widths = column_widths(&rows);
    let trailing_newline = input.ends_with('\n');

    let mut out = String::new();
    for row in &rows {
        for (j, field) in row.iter().enumerate() {
            if j > 0 {
                out.push(',');
            }
            out.push_str(field);
            let width = widths.get(j).copied().unwrap_or(0);
            let pad = width.saturating_sub(field.chars().count());
            for _ in 0..pad {
                out.push(' ');
            }
        }
        out.push('\n');
    }
    if !trailing_newline {
        out.pop();
    }
    out
}

/// Strips trailing spaces added by `align`. For an input that was never
/// aligned (no trailing padding) this is a no-op.
pub fn unalign(input: &str) -> String {
    let rows = parse(input);
    let trailing_newline = input.ends_with('\n');

    let mut out = String::new();
    for row in &rows {
        for (j, field) in row.iter().enumerate() {
            if j > 0 {
                out.push(',');
            }
            out.push_str(field.trim_end_matches(' '));
        }
        out.push('\n');
    }
    if !trailing_newline {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_rows() {
        let rows = parse("a,b,c\nd,e,f\n");
        assert_eq!(rows, vec![vec!["a", "b", "c"], vec!["d", "e", "f"]]);
    }

    #[test]
    fn parses_quoted_field_with_comma() {
        let rows = parse("a,\"b,c\",d\n");
        assert_eq!(rows, vec![vec!["a", "\"b,c\"", "d"]]);
    }

    #[test]
    fn parses_escaped_quote() {
        let rows = parse("\"he said \"\"hi\"\"\",x\n");
        assert_eq!(rows, vec![vec!["\"he said \"\"hi\"\"\"", "x"]]);
    }

    #[test]
    fn parses_embedded_newline_in_quoted() {
        let rows = parse("\"line1\nline2\",x\n");
        assert_eq!(rows, vec![vec!["\"line1\nline2\"", "x"]]);
    }

    #[test]
    fn parses_crlf() {
        let rows = parse("a,b\r\nc,d\r\n");
        assert_eq!(rows, vec![vec!["a", "b"], vec!["c", "d"]]);
    }

    #[test]
    fn parses_trailing_empty_field() {
        let rows = parse("a,b,\n");
        assert_eq!(rows, vec![vec!["a", "b", ""]]);
    }

    #[test]
    fn parses_no_trailing_newline() {
        let rows = parse("a,b");
        assert_eq!(rows, vec![vec!["a", "b"]]);
    }

    #[test]
    fn align_pads_to_column_width() {
        let input = "id,name\n1,Alice\n22,Bob\n";
        let expected = "id,name \n1 ,Alice\n22,Bob  \n";
        assert_eq!(align(input), expected);
    }

    #[test]
    fn align_preserves_quoted_fields() {
        let input = "a,\"b,c\"\nxx,y\n";
        let expected = "a ,\"b,c\"\nxx,y    \n";
        assert_eq!(align(input), expected);
    }

    #[test]
    fn align_preserves_trailing_newline_absence() {
        let out = align("a,b\nc,d");
        assert!(!out.ends_with('\n'));
    }

    #[test]
    fn unalign_strips_trailing_spaces() {
        let aligned = "id,name \n1 ,Alice\n22,Bob  \n";
        let expected = "id,name\n1,Alice\n22,Bob\n";
        assert_eq!(unalign(aligned), expected);
    }

    #[test]
    fn round_trip_basic() {
        let canonical = "id,name,email\n1,Alice,a@x\n22,Bob,b@x\n";
        assert_eq!(unalign(&align(canonical)), canonical);
    }

    #[test]
    fn round_trip_with_quoted_and_embedded_comma() {
        let canonical = "a,\"b,c\",d\nxx,\"y\",zz\n";
        assert_eq!(unalign(&align(canonical)), canonical);
    }

    #[test]
    fn round_trip_with_ragged_rows() {
        let canonical = "a,b,c\nd,e\nf\n";
        assert_eq!(unalign(&align(canonical)), canonical);
    }

    #[test]
    fn round_trip_empty_fields() {
        let canonical = "a,,c\n,,\nx,y,z\n";
        assert_eq!(unalign(&align(canonical)), canonical);
    }

    #[test]
    fn round_trip_escaped_quotes() {
        let canonical = "\"a\"\"b\",c\nx,y\n";
        assert_eq!(unalign(&align(canonical)), canonical);
    }

    #[test]
    fn round_trip_no_trailing_newline() {
        let canonical = "a,b\nc,d";
        assert_eq!(unalign(&align(canonical)), canonical);
    }
}
