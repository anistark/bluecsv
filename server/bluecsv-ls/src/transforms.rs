//! Buffer-level CSV transforms exposed as `workspace/executeCommand` handlers.
//! Each function takes the full buffer text and returns the new text.

use crate::model;

pub fn add_column(input: &str, has_header: bool) -> String {
    let mut rows = bluecsv::parse(input);
    let trailing = input.ends_with('\n');
    let max_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let header_name = format!("column_{}", max_cols + 1);
    for (i, row) in rows.iter_mut().enumerate() {
        while row.len() < max_cols {
            row.push(String::new());
        }
        if has_header && i == 0 {
            row.push(header_name.clone());
        } else {
            row.push(String::new());
        }
    }
    serialize(&rows, trailing)
}

pub fn delete_column(input: &str, col: usize) -> String {
    let mut rows = bluecsv::parse(input);
    let trailing = input.ends_with('\n');
    for row in rows.iter_mut() {
        if col < row.len() {
            row.remove(col);
        }
    }
    serialize(&rows, trailing)
}

pub fn duplicate_row(input: &str, row: usize) -> String {
    let mut rows = bluecsv::parse(input);
    let trailing = input.ends_with('\n');
    if row < rows.len() {
        let clone = rows[row].clone();
        rows.insert(row + 1, clone);
    }
    serialize(&rows, trailing)
}

pub fn sort_by_column(input: &str, col: usize, ascending: bool, has_header: bool) -> String {
    let mut rows = bluecsv::parse(input);
    let trailing = input.ends_with('\n');
    let body_start = if has_header && !rows.is_empty() { 1 } else { 0 };
    if body_start < rows.len() {
        rows[body_start..].sort_by(|a, b| {
            let av = a.get(col).map(|s| model::canonical(s)).unwrap_or_default();
            let bv = b.get(col).map(|s| model::canonical(s)).unwrap_or_default();
            if ascending {
                av.cmp(&bv)
            } else {
                bv.cmp(&av)
            }
        });
    }
    serialize(&rows, trailing)
}

/// Re-emits a raw field value in properly quoted form: wraps in `"` and
/// doubles any embedded `"`. Used by the auto-quote-escape on-type formatter.
pub fn quote_field(raw: &str) -> String {
    format!("\"{}\"", raw.replace('"', "\"\""))
}

/// Renders the buffer as a GitHub-flavored pipe table. Fields are canonicalized
/// (quotes/padding stripped), then pipe and newline characters are escaped
/// (`|` → `\|`, `\n` → `<br>`). The first row becomes the header, followed by
/// a `| --- |` separator.
pub fn to_markdown_table(input: &str) -> String {
    let rows = bluecsv::parse(input);
    if rows.is_empty() {
        return String::new();
    }
    let max_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    if max_cols == 0 {
        return String::new();
    }
    let padded: Vec<Vec<String>> = rows
        .into_iter()
        .map(|mut r| {
            while r.len() < max_cols {
                r.push(String::new());
            }
            r.into_iter()
                .map(|f| md_cell_escape(&model::canonical(&f)))
                .collect()
        })
        .collect();

    let widths: Vec<usize> = (0..max_cols)
        .map(|i| {
            padded
                .iter()
                .map(|r| r[i].chars().count())
                .max()
                .unwrap_or(0)
                .max(3)
        })
        .collect();

    let mut out = String::new();
    for (idx, row) in padded.iter().enumerate() {
        out.push('|');
        for (c, cell) in row.iter().enumerate() {
            out.push(' ');
            out.push_str(cell);
            for _ in 0..widths[c].saturating_sub(cell.chars().count()) {
                out.push(' ');
            }
            out.push_str(" |");
        }
        out.push('\n');
        if idx == 0 {
            out.push('|');
            for w in &widths {
                out.push(' ');
                for _ in 0..*w {
                    out.push('-');
                }
                out.push_str(" |");
            }
            out.push('\n');
        }
    }
    out
}

/// Parses a pipe table back into CSV. Cells are trimmed (markdown ignores
/// surrounding whitespace), `\|` is unescaped, `<br>` is restored as a
/// newline, and fields containing a delimiter / quote / newline are
/// CSV-quoted.
pub fn from_markdown_table(input: &str) -> String {
    let mut rows: Vec<Vec<String>> = Vec::new();
    for line in input.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || !trimmed.contains('|') {
            continue;
        }
        let cells = parse_md_row(trimmed);
        if is_md_separator_row(&cells) {
            continue;
        }
        rows.push(cells);
    }
    let mut out = String::new();
    for row in &rows {
        for (j, cell) in row.iter().enumerate() {
            if j > 0 {
                out.push(',');
            }
            out.push_str(&csv_quote_if_needed(cell));
        }
        out.push('\n');
    }
    out
}

fn md_cell_escape(value: &str) -> String {
    value
        .replace('|', "\\|")
        .replace("\r\n", "<br>")
        .replace(['\n', '\r'], "<br>")
}

fn parse_md_row(line: &str) -> Vec<String> {
    let mut s = line.trim();
    if let Some(rest) = s.strip_prefix('|') {
        s = rest;
    }
    if let Some(rest) = s.strip_suffix('|') {
        s = rest;
    }
    let mut cells = Vec::new();
    let mut cur = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek() == Some(&'|') {
            chars.next();
            cur.push('|');
        } else if c == '|' {
            cells.push(unescape_md_cell(cur.trim()));
            cur = String::new();
        } else {
            cur.push(c);
        }
    }
    cells.push(unescape_md_cell(cur.trim()));
    cells
}

fn unescape_md_cell(s: &str) -> String {
    s.replace("<br>", "\n")
}

fn is_md_separator_row(cells: &[String]) -> bool {
    !cells.is_empty()
        && cells.iter().all(|c| {
            let t = c.trim_matches(|ch: char| ch == ':' || ch.is_whitespace());
            !t.is_empty() && t.chars().all(|ch| ch == '-')
        })
}

fn csv_quote_if_needed(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn serialize(rows: &[Vec<String>], trailing_newline: bool) -> String {
    let mut out = String::new();
    for row in rows {
        for (j, field) in row.iter().enumerate() {
            if j > 0 {
                out.push(',');
            }
            out.push_str(field);
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
    fn add_column_appends_empty_field_and_generated_header() {
        let out = add_column("id,name\n1,alice\n2,bob\n", true);
        assert_eq!(out, "id,name,column_3\n1,alice,\n2,bob,\n");
    }

    #[test]
    fn add_column_without_header_appends_empty_everywhere() {
        let out = add_column("1,alice\n2,bob\n", false);
        assert_eq!(out, "1,alice,\n2,bob,\n");
    }

    #[test]
    fn add_column_pads_ragged_rows_before_appending() {
        let out = add_column("a,b,c\nd,e\n", false);
        assert_eq!(out, "a,b,c,\nd,e,,\n");
    }

    #[test]
    fn delete_column_removes_field_from_every_row() {
        let out = delete_column("id,name,email\n1,alice,a@x\n2,bob,b@x\n", 1);
        assert_eq!(out, "id,email\n1,a@x\n2,b@x\n");
    }

    #[test]
    fn delete_column_out_of_range_is_noop() {
        let input = "a,b\nc,d\n";
        assert_eq!(delete_column(input, 5), input);
    }

    #[test]
    fn duplicate_row_inserts_copy_after_target() {
        let out = duplicate_row("id,name\n1,alice\n2,bob\n", 1);
        assert_eq!(out, "id,name\n1,alice\n1,alice\n2,bob\n");
    }

    #[test]
    fn duplicate_row_out_of_range_is_noop() {
        let input = "a\nb\n";
        assert_eq!(duplicate_row(input, 9), input);
    }

    #[test]
    fn sort_by_column_preserves_header_and_sorts_ascending() {
        let out = sort_by_column("name\ncarol\nalice\nbob\n", 0, true, true);
        assert_eq!(out, "name\nalice\nbob\ncarol\n");
    }

    #[test]
    fn sort_by_column_descending() {
        let out = sort_by_column("name\ncarol\nalice\nbob\n", 0, false, true);
        assert_eq!(out, "name\ncarol\nbob\nalice\n");
    }

    #[test]
    fn sort_by_column_uses_canonical_value_through_quotes_and_padding() {
        let out = sort_by_column("name\n\"carol\"\nalice \nbob\n", 0, true, true);
        assert_eq!(out, "name\nalice \nbob\n\"carol\"\n");
    }

    #[test]
    fn sort_by_column_without_header_sorts_every_row() {
        let out = sort_by_column("b\na\nc\n", 0, true, false);
        assert_eq!(out, "a\nb\nc\n");
    }

    #[test]
    fn preserves_trailing_newline_absence() {
        let out = add_column("a,b", false);
        assert!(!out.ends_with('\n'));
    }

    #[test]
    fn quote_field_wraps_and_doubles_embedded_quotes() {
        assert_eq!(quote_field("abc"), "\"abc\"");
        assert_eq!(quote_field("a\"b"), "\"a\"\"b\"");
        assert_eq!(quote_field("a,b"), "\"a,b\"");
    }

    #[test]
    fn to_markdown_table_basic() {
        let out = to_markdown_table("id,name\n1,alice\n22,bob\n");
        let expected = "\
| id  | name  |
| --- | ----- |
| 1   | alice |
| 22  | bob   |
";
        assert_eq!(out, expected);
    }

    #[test]
    fn to_markdown_table_pads_ragged_rows() {
        let out = to_markdown_table("a,b,c\nd,e\n");
        let expected = "\
| a   | b   | c   |
| --- | --- | --- |
| d   | e   |     |
";
        assert_eq!(out, expected);
    }

    #[test]
    fn to_markdown_table_escapes_pipes_and_newlines() {
        let out = to_markdown_table("x\n\"a|b\"\n\"a\nb\"\n");
        let expected = "\
| x      |
| ------ |
| a\\|b   |
| a<br>b |
";
        assert_eq!(out, expected);
    }

    #[test]
    fn from_markdown_table_basic() {
        let md = "\
| id  | name  |
| --- | ----- |
| 1   | alice |
| 22  | bob   |
";
        assert_eq!(from_markdown_table(md), "id,name\n1,alice\n22,bob\n");
    }

    #[test]
    fn from_markdown_table_tolerates_alignment_colons() {
        let md = "| a | b |\n| :-- | ---: |\n| 1 | 2 |\n";
        assert_eq!(from_markdown_table(md), "a,b\n1,2\n");
    }

    #[test]
    fn from_markdown_table_tolerates_missing_outer_pipes() {
        let md = "a | b\n--- | ---\n1 | 2\n";
        assert_eq!(from_markdown_table(md), "a,b\n1,2\n");
    }

    #[test]
    fn from_markdown_table_csv_quotes_when_needed() {
        let md = "| a | b |\n| --- | --- |\n| 1 | has, comma |\n| 2 | has \"quote\" |\n";
        assert_eq!(
            from_markdown_table(md),
            "a,b\n1,\"has, comma\"\n2,\"has \"\"quote\"\"\"\n"
        );
    }

    #[test]
    fn from_markdown_table_restores_pipes_and_newlines() {
        // `|` isn't a CSV metacharacter, so the restored field stays unquoted;
        // embedded newlines force CSV quoting.
        let md = "| a    |\n| ---- |\n| a\\|b |\n| a<br>b |\n";
        assert_eq!(from_markdown_table(md), "a\na|b\n\"a\nb\"\n");
    }

    #[test]
    fn round_trip_csv_md_csv_simple() {
        let csv = "id,name,email\n1,alice,a@x\n22,bob,b@x\n";
        let back = from_markdown_table(&to_markdown_table(csv));
        assert_eq!(back, csv);
    }

    #[test]
    fn round_trip_csv_md_csv_canonicalizes_unnecessary_quotes() {
        // `|` doesn't need CSV quoting, so the round trip strips the outer
        // quotes around `"x|y"`. Embedded newlines still require quotes.
        let csv = "a,b\n\"x|y\",\"m\nn\"\n";
        let back = from_markdown_table(&to_markdown_table(csv));
        assert_eq!(back, "a,b\nx|y,\"m\nn\"\n");
    }

    #[test]
    fn round_trip_lossy_trims_field_whitespace() {
        // Documented lossy case: leading/trailing spaces inside a CSV field
        // are dropped because markdown table cells are whitespace-trimmed.
        let csv = "a,b\n\"  x  \",y\n";
        let back = from_markdown_table(&to_markdown_table(csv));
        assert_eq!(back, "a,b\nx,y\n");
    }
}
