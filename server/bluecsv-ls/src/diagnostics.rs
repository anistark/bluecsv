use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};

#[derive(Copy, Clone, PartialEq)]
enum State {
    FieldStart,
    Unquoted,
    Quoted,
    AfterClosingQuote,
}

struct Row {
    start_line: u32,
    end_line: u32,
    fields: u32,
    has_content: bool,
}

pub fn scan(input: &str) -> Vec<Diagnostic> {
    let mut state = State::FieldStart;
    let mut line: u32 = 0;
    let mut col: u32 = 0;
    let mut row_start_line: u32 = 0;
    let mut fields: u32 = 1;
    let mut row_has_content = false;
    let mut rows: Vec<Row> = Vec::new();
    let mut open_quote: Option<(u32, u32)> = None;
    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    let mut chars = input.chars().peekable();
    let mut saw_any = false;

    let push_row = |rows: &mut Vec<Row>,
                    row_start_line: u32,
                    end_line: u32,
                    fields: u32,
                    has_content: bool| {
        rows.push(Row {
            start_line: row_start_line,
            end_line,
            fields,
            has_content,
        });
    };

    while let Some(c) = chars.next() {
        saw_any = true;
        match state {
            State::FieldStart => match c {
                '"' => {
                    open_quote = Some((line, col));
                    state = State::Quoted;
                    row_has_content = true;
                    col += 1;
                }
                ',' => {
                    fields += 1;
                    row_has_content = true;
                    col += 1;
                }
                '\r' => {
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    push_row(&mut rows, row_start_line, line, fields, row_has_content);
                    line += 1;
                    col = 0;
                    fields = 1;
                    row_has_content = false;
                    row_start_line = line;
                }
                '\n' => {
                    push_row(&mut rows, row_start_line, line, fields, row_has_content);
                    line += 1;
                    col = 0;
                    fields = 1;
                    row_has_content = false;
                    row_start_line = line;
                }
                _ => {
                    state = State::Unquoted;
                    row_has_content = true;
                    col += c.len_utf16() as u32;
                }
            },
            State::Unquoted | State::AfterClosingQuote => match c {
                ',' => {
                    fields += 1;
                    state = State::FieldStart;
                    col += 1;
                }
                '\r' => {
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    push_row(&mut rows, row_start_line, line, fields, row_has_content);
                    line += 1;
                    col = 0;
                    fields = 1;
                    row_has_content = false;
                    row_start_line = line;
                    state = State::FieldStart;
                }
                '\n' => {
                    push_row(&mut rows, row_start_line, line, fields, row_has_content);
                    line += 1;
                    col = 0;
                    fields = 1;
                    row_has_content = false;
                    row_start_line = line;
                    state = State::FieldStart;
                }
                _ => col += c.len_utf16() as u32,
            },
            State::Quoted => match c {
                '"' => {
                    col += 1;
                    if chars.peek() == Some(&'"') {
                        chars.next();
                        col += 1;
                    } else {
                        state = State::AfterClosingQuote;
                        open_quote = None;
                    }
                }
                '\n' => {
                    line += 1;
                    col = 0;
                }
                '\r' => {
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    line += 1;
                    col = 0;
                }
                _ => col += c.len_utf16() as u32,
            },
        }
    }

    if state == State::Quoted {
        if let Some((ql, qc)) = open_quote {
            diagnostics.push(Diagnostic {
                range: Range {
                    start: Position::new(ql, qc),
                    end: Position::new(line, col),
                },
                severity: Some(DiagnosticSeverity::ERROR),
                message: "Unterminated quoted field.".into(),
                source: Some("bluecsv".into()),
                ..Default::default()
            });
        }
        return diagnostics;
    }

    if saw_any && row_has_content {
        push_row(&mut rows, row_start_line, line, fields, row_has_content);
    }

    let baseline = rows.iter().find(|r| r.has_content).map(|r| r.fields);
    if let Some(expected) = baseline {
        for row in &rows {
            if !row.has_content || row.fields == expected {
                continue;
            }
            diagnostics.push(Diagnostic {
                range: Range {
                    start: Position::new(row.start_line, 0),
                    end: Position::new(row.end_line.saturating_add(1), 0),
                },
                severity: Some(DiagnosticSeverity::WARNING),
                message: format!("Row has {} fields; expected {}.", row.fields, expected),
                source: Some("bluecsv".into()),
                ..Default::default()
            });
        }
    }

    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_input_no_diagnostics() {
        assert!(scan("a,b,c\nd,e,f\n").is_empty());
    }

    #[test]
    fn row_width_mismatch_flagged() {
        let diags = scan("a,b,c\nd,e\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, Some(DiagnosticSeverity::WARNING));
        assert_eq!(diags[0].range.start.line, 1);
    }

    #[test]
    fn unterminated_quote_flagged() {
        let diags = scan("a,\"oops\nmore\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(diags[0].range.start, Position::new(0, 2));
    }

    #[test]
    fn blank_trailing_line_is_ignored() {
        assert!(scan("a,b\nc,d\n\n").is_empty());
    }

    #[test]
    fn quoted_embedded_newline_does_not_break_rows() {
        assert!(scan("\"a\nb\",c\nd,e\n").is_empty());
    }

    #[test]
    fn no_trailing_newline_still_counts_last_row() {
        let diags = scan("a,b\nc");
        assert_eq!(diags.len(), 1);
    }
}
