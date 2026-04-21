//! Positional CSV model: parses a buffer into cells with UTF-16 LSP ranges
//! so hover / completion / definition can locate the cell under a cursor and
//! find peers in the same column.

use std::collections::BTreeSet;

use tower_lsp::lsp_types::{Position, Range};

#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    pub row: usize,
    pub col: usize,
    pub raw: String,
    pub value: String,
    pub range: Range,
}

#[derive(Debug, Clone, Default)]
pub struct Model {
    pub cells: Vec<Vec<Cell>>,
}

#[derive(Copy, Clone, PartialEq)]
enum State {
    FieldStart,
    Unquoted,
    Quoted,
    AfterClosingQuote,
}

pub fn canonical(raw: &str) -> String {
    let trimmed = raw.trim_end_matches(' ');
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        trimmed[1..trimmed.len() - 1].replace("\"\"", "\"")
    } else {
        trimmed.to_string()
    }
}

fn push_cell(
    row_cells: &mut Vec<Cell>,
    row_idx: usize,
    col_idx: &mut usize,
    field: &mut String,
    field_start: Position,
    end: Position,
) {
    let raw = std::mem::take(field);
    let value = canonical(&raw);
    row_cells.push(Cell {
        row: row_idx,
        col: *col_idx,
        raw,
        value,
        range: Range {
            start: field_start,
            end,
        },
    });
    *col_idx += 1;
}

impl Model {
    pub fn parse(text: &str) -> Self {
        let mut cells: Vec<Vec<Cell>> = Vec::new();
        let mut row_cells: Vec<Cell> = Vec::new();
        let mut field = String::new();
        let mut field_start = Position::new(0, 0);
        let mut state = State::FieldStart;
        let mut line: u32 = 0;
        let mut col: u32 = 0;
        let mut row_idx: usize = 0;
        let mut col_idx: usize = 0;
        let mut chars = text.chars().peekable();

        while let Some(c) = chars.next() {
            let here = Position::new(line, col);
            match state {
                State::FieldStart => {
                    field_start = here;
                    match c {
                        '"' => {
                            field.push('"');
                            state = State::Quoted;
                            col += 1;
                        }
                        ',' => {
                            push_cell(
                                &mut row_cells,
                                row_idx,
                                &mut col_idx,
                                &mut field,
                                field_start,
                                here,
                            );
                            col += 1;
                        }
                        '\n' => {
                            push_cell(
                                &mut row_cells,
                                row_idx,
                                &mut col_idx,
                                &mut field,
                                field_start,
                                here,
                            );
                            cells.push(std::mem::take(&mut row_cells));
                            col_idx = 0;
                            row_idx += 1;
                            line += 1;
                            col = 0;
                        }
                        '\r' => {
                            if chars.peek() == Some(&'\n') {
                                chars.next();
                            }
                            push_cell(
                                &mut row_cells,
                                row_idx,
                                &mut col_idx,
                                &mut field,
                                field_start,
                                here,
                            );
                            cells.push(std::mem::take(&mut row_cells));
                            col_idx = 0;
                            row_idx += 1;
                            line += 1;
                            col = 0;
                        }
                        _ => {
                            field.push(c);
                            state = State::Unquoted;
                            col += c.len_utf16() as u32;
                        }
                    }
                }
                State::Unquoted | State::AfterClosingQuote => match c {
                    ',' => {
                        push_cell(
                            &mut row_cells,
                            row_idx,
                            &mut col_idx,
                            &mut field,
                            field_start,
                            here,
                        );
                        col += 1;
                        state = State::FieldStart;
                    }
                    '\n' => {
                        push_cell(
                            &mut row_cells,
                            row_idx,
                            &mut col_idx,
                            &mut field,
                            field_start,
                            here,
                        );
                        cells.push(std::mem::take(&mut row_cells));
                        col_idx = 0;
                        row_idx += 1;
                        line += 1;
                        col = 0;
                        state = State::FieldStart;
                    }
                    '\r' => {
                        if chars.peek() == Some(&'\n') {
                            chars.next();
                        }
                        push_cell(
                            &mut row_cells,
                            row_idx,
                            &mut col_idx,
                            &mut field,
                            field_start,
                            here,
                        );
                        cells.push(std::mem::take(&mut row_cells));
                        col_idx = 0;
                        row_idx += 1;
                        line += 1;
                        col = 0;
                        state = State::FieldStart;
                    }
                    _ => {
                        field.push(c);
                        col += c.len_utf16() as u32;
                    }
                },
                State::Quoted => match c {
                    '"' => {
                        field.push('"');
                        col += 1;
                        if chars.peek() == Some(&'"') {
                            chars.next();
                            field.push('"');
                            col += 1;
                        } else {
                            state = State::AfterClosingQuote;
                        }
                    }
                    '\n' => {
                        field.push('\n');
                        line += 1;
                        col = 0;
                    }
                    '\r' => {
                        if chars.peek() == Some(&'\n') {
                            chars.next();
                            field.push('\r');
                            field.push('\n');
                        } else {
                            field.push('\r');
                        }
                        line += 1;
                        col = 0;
                    }
                    _ => {
                        field.push(c);
                        col += c.len_utf16() as u32;
                    }
                },
            }
        }

        if !field.is_empty() || !row_cells.is_empty() {
            let end = Position::new(line, col);
            push_cell(
                &mut row_cells,
                row_idx,
                &mut col_idx,
                &mut field,
                field_start,
                end,
            );
            cells.push(row_cells);
        }

        Model { cells }
    }

    pub fn cell_at(&self, pos: Position) -> Option<&Cell> {
        for row in &self.cells {
            for cell in row {
                if position_in_range(pos, cell.range) {
                    return Some(cell);
                }
            }
        }
        None
    }

    pub fn header(&self, col: usize) -> Option<&str> {
        self.cells.first()?.get(col).map(|c| c.value.as_str())
    }

    pub fn column_values_excluding(
        &self,
        col: usize,
        exclude_row: Option<usize>,
        skip_header: bool,
    ) -> BTreeSet<String> {
        let skip = if skip_header { 1 } else { 0 };
        let mut out = BTreeSet::new();
        for (i, row) in self.cells.iter().enumerate() {
            if i < skip {
                continue;
            }
            if Some(i) == exclude_row {
                continue;
            }
            if let Some(cell) = row.get(col) {
                if !cell.value.is_empty() {
                    out.insert(cell.value.clone());
                }
            }
        }
        out
    }

    pub fn find_in_column(&self, col: usize, value: &str, skip_header: bool) -> Vec<&Cell> {
        let skip = if skip_header { 1 } else { 0 };
        let mut out = Vec::new();
        for (i, row) in self.cells.iter().enumerate() {
            if i < skip {
                continue;
            }
            if let Some(cell) = row.get(col) {
                if cell.value == value {
                    out.push(cell);
                }
            }
        }
        out
    }
}

fn position_in_range(pos: Position, range: Range) -> bool {
    let after_start = pos.line > range.start.line
        || (pos.line == range.start.line && pos.character >= range.start.character);
    let before_end = pos.line < range.end.line
        || (pos.line == range.end.line && pos.character <= range.end.character);
    after_start && before_end
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(line: u32, ch: u32) -> Position {
        Position::new(line, ch)
    }

    #[test]
    fn parses_simple_rows_with_ranges() {
        let m = Model::parse("a,bb,ccc\nd,e,f\n");
        assert_eq!(m.cells.len(), 2);
        let r0 = &m.cells[0];
        assert_eq!(r0[0].value, "a");
        assert_eq!(r0[0].range, Range::new(pos(0, 0), pos(0, 1)));
        assert_eq!(r0[1].value, "bb");
        assert_eq!(r0[1].range, Range::new(pos(0, 2), pos(0, 4)));
        assert_eq!(r0[2].value, "ccc");
        assert_eq!(r0[2].range, Range::new(pos(0, 5), pos(0, 8)));
    }

    #[test]
    fn parses_empty_fields() {
        let m = Model::parse("a,,c\n");
        assert_eq!(m.cells[0].len(), 3);
        assert_eq!(m.cells[0][1].value, "");
        assert_eq!(m.cells[0][1].range, Range::new(pos(0, 2), pos(0, 2)));
    }

    #[test]
    fn parses_quoted_field_and_normalizes_value() {
        let m = Model::parse("\"a,b\",\"he said \"\"hi\"\"\"\n");
        assert_eq!(m.cells[0][0].value, "a,b");
        assert_eq!(m.cells[0][1].value, "he said \"hi\"");
    }

    #[test]
    fn parses_crlf() {
        let m = Model::parse("a,b\r\nc,d\r\n");
        assert_eq!(m.cells.len(), 2);
        assert_eq!(m.cells[1][0].value, "c");
        assert_eq!(m.cells[1][0].range.start, pos(1, 0));
    }

    #[test]
    fn parses_embedded_newline_in_quoted_advances_line() {
        let m = Model::parse("\"a\nb\",c\n");
        assert_eq!(m.cells.len(), 1);
        assert_eq!(m.cells[0][0].value, "a\nb");
        assert_eq!(m.cells[0][1].range.start, pos(1, 3));
    }

    #[test]
    fn trailing_padding_is_stripped_in_canonical_value() {
        let m = Model::parse("id,name \n1 ,Alice\n");
        assert_eq!(m.cells[0][1].value, "name");
        assert_eq!(m.cells[1][0].value, "1");
    }

    #[test]
    fn cell_at_finds_cell_under_cursor() {
        let m = Model::parse("a,bb,ccc\n");
        assert_eq!(m.cell_at(pos(0, 0)).unwrap().value, "a");
        assert_eq!(m.cell_at(pos(0, 3)).unwrap().value, "bb");
        assert_eq!(m.cell_at(pos(0, 7)).unwrap().value, "ccc");
    }

    #[test]
    fn cell_at_returns_none_outside_any_cell() {
        let m = Model::parse("a\n");
        assert!(m.cell_at(pos(1, 5)).is_none());
    }

    #[test]
    fn column_values_excluding_skips_header_and_current_row() {
        let m = Model::parse("name\nalice\nbob\nalice\n");
        let vals = m.column_values_excluding(0, Some(1), true);
        assert_eq!(
            vals.into_iter().collect::<Vec<_>>(),
            vec!["alice".to_string(), "bob".to_string()]
        );
    }

    #[test]
    fn find_in_column_returns_all_matches() {
        let m = Model::parse("name\nalice\nbob\nalice\n");
        let hits = m.find_in_column(0, "alice", true);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].row, 1);
        assert_eq!(hits[1].row, 3);
    }

    #[test]
    fn header_returns_first_row_value() {
        let m = Model::parse("id,name\n1,a\n");
        assert_eq!(m.header(0), Some("id"));
        assert_eq!(m.header(1), Some("name"));
        assert_eq!(m.header(2), None);
    }
}
