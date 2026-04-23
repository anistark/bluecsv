//! Per-cell and per-column type inference.
//!
//! Deliberately dependency-free: date recognition is hand-rolled against a
//! small whitelist of ISO-ish shapes so the crate stays standalone.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CellType {
    Empty,
    Int,
    Float,
    Date,
    String,
}

impl CellType {
    pub fn label(self) -> &'static str {
        match self {
            CellType::Empty => "empty",
            CellType::Int => "int",
            CellType::Float => "float",
            CellType::Date => "date",
            CellType::String => "string",
        }
    }
}

pub fn classify_cell(raw: &str) -> CellType {
    let s = raw.trim();
    if s.is_empty() {
        return CellType::Empty;
    }
    if is_int(s) {
        return CellType::Int;
    }
    if is_float(s) {
        return CellType::Float;
    }
    if is_date(s) {
        return CellType::Date;
    }
    CellType::String
}

fn is_int(s: &str) -> bool {
    let bytes = s.as_bytes();
    let start = match bytes.first() {
        Some(b'-') | Some(b'+') => 1,
        _ => 0,
    };
    if start >= bytes.len() {
        return false;
    }
    bytes[start..].iter().all(|b| b.is_ascii_digit())
}

fn is_float(s: &str) -> bool {
    if is_int(s) {
        return false;
    }
    // Disallow hex/inf/nan/underscore shapes that `f64::from_str` would accept.
    if !s
        .bytes()
        .all(|b| matches!(b, b'0'..=b'9' | b'.' | b'-' | b'+' | b'e' | b'E'))
    {
        return false;
    }
    s.parse::<f64>().is_ok()
}

/// Accepts:
/// - `YYYY-MM-DD`
/// - `YYYY/MM/DD`
/// - `YYYY-MM-DDTHH:MM:SS`
/// - `YYYY-MM-DDTHH:MM:SSZ`
/// - `YYYY-MM-DDTHH:MM:SS±HH:MM`
fn is_date(s: &str) -> bool {
    let (date_part, rest) = match s.split_once('T') {
        Some((d, r)) => (d, Some(r)),
        None => (s, None),
    };
    if !is_calendar_date(date_part) {
        return false;
    }
    match rest {
        None => true,
        Some(time) => is_time_with_offset(time),
    }
}

fn is_calendar_date(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() != 10 {
        return false;
    }
    let sep = bytes[4];
    if sep != b'-' && sep != b'/' {
        return false;
    }
    if bytes[7] != sep {
        return false;
    }
    bytes[0..4].iter().all(|b| b.is_ascii_digit())
        && bytes[5..7].iter().all(|b| b.is_ascii_digit())
        && bytes[8..10].iter().all(|b| b.is_ascii_digit())
}

fn is_time_with_offset(s: &str) -> bool {
    let (time, offset) = split_offset(s);
    if !is_time(time) {
        return false;
    }
    match offset {
        None => true,
        Some("Z") => true,
        Some(o) => is_numeric_offset(o),
    }
}

fn split_offset(s: &str) -> (&str, Option<&str>) {
    if let Some(stripped) = s.strip_suffix('Z') {
        return (stripped, Some("Z"));
    }
    if s.len() >= 6 {
        let cut = s.len() - 6;
        let rest = &s[cut..];
        let bytes = rest.as_bytes();
        if (bytes[0] == b'+' || bytes[0] == b'-') && bytes[3] == b':' {
            return (&s[..cut], Some(rest));
        }
    }
    (s, None)
}

fn is_time(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() != 8 || bytes[2] != b':' || bytes[5] != b':' {
        return false;
    }
    bytes[0..2].iter().all(|b| b.is_ascii_digit())
        && bytes[3..5].iter().all(|b| b.is_ascii_digit())
        && bytes[6..8].iter().all(|b| b.is_ascii_digit())
}

fn is_numeric_offset(s: &str) -> bool {
    let bytes = s.as_bytes();
    bytes.len() == 6
        && (bytes[0] == b'+' || bytes[0] == b'-')
        && bytes[3] == b':'
        && bytes[1..3].iter().all(|b| b.is_ascii_digit())
        && bytes[4..6].iter().all(|b| b.is_ascii_digit())
}

/// Tuning knobs for `infer_column`. Kept as module constants so they can be
/// moved behind a setting later without changing the public API.
pub const MIN_SAMPLE_FOR_TYPED_COLUMN: usize = 3;
pub const MIN_CONFIDENCE: f32 = 0.9;

#[derive(Debug, Clone, PartialEq)]
pub struct ColumnType {
    pub primary: CellType,
    pub confidence: f32,
    pub empty_count: usize,
    /// Row indices (absolute, same as the iterator keys passed in) whose
    /// classified type differed from `primary`. Empty cells are never listed.
    pub mismatch_rows: Vec<usize>,
}

/// Classify a column given an iterator of `(row_index, raw_value)` pairs.
/// The row index is passed through unchanged into `mismatch_rows`.
pub fn infer_column<'a, I>(values: I) -> ColumnType
where
    I: IntoIterator<Item = (usize, &'a str)>,
{
    let mut empty_count = 0usize;
    let mut non_empty: Vec<(usize, CellType)> = Vec::new();
    for (row, raw) in values {
        let t = classify_cell(raw);
        if t == CellType::Empty {
            empty_count += 1;
        } else {
            non_empty.push((row, t));
        }
    }

    if non_empty.is_empty() {
        return ColumnType {
            primary: CellType::Empty,
            confidence: 1.0,
            empty_count,
            mismatch_rows: Vec::new(),
        };
    }

    let total = non_empty.len();
    let candidates = [
        CellType::Int,
        CellType::Float,
        CellType::Date,
        CellType::String,
    ];
    let mut best = CellType::String;
    let mut best_count = 0usize;
    for candidate in candidates {
        let count = non_empty
            .iter()
            .filter(|(_, t)| matches_as(*t, candidate))
            .count();
        if count > best_count {
            best_count = count;
            best = candidate;
        }
    }

    let confidence = best_count as f32 / total as f32;
    let typed = total >= MIN_SAMPLE_FOR_TYPED_COLUMN
        && confidence >= MIN_CONFIDENCE
        && best != CellType::String;

    let (primary, mismatch_rows) = if typed {
        let mismatches = non_empty
            .iter()
            .filter(|(_, t)| !matches_as(*t, best))
            .map(|(row, _)| *row)
            .collect();
        (best, mismatches)
    } else {
        (CellType::String, Vec::new())
    };

    ColumnType {
        primary,
        confidence,
        empty_count,
        mismatch_rows,
    }
}

/// An `Int` cell satisfies a `Float` column (numeric promotion). All other
/// type matches are strict.
fn matches_as(cell: CellType, column: CellType) -> bool {
    match (cell, column) {
        (a, b) if a == b => true,
        (CellType::Int, CellType::Float) => true,
        _ => false,
    }
}

/// Infer types for every column in a parsed table. Skips row 0 when
/// `has_header` is true.
pub fn infer_table(rows: &[Vec<String>], has_header: bool) -> Vec<ColumnType> {
    let max_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let skip = if has_header { 1 } else { 0 };
    (0..max_cols)
        .map(|col| {
            let values = rows
                .iter()
                .enumerate()
                .skip(skip)
                .filter_map(|(i, r)| r.get(col).map(|v| (i, v.as_str())));
            infer_column(values)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_whitespace() {
        assert_eq!(classify_cell(""), CellType::Empty);
        assert_eq!(classify_cell("   "), CellType::Empty);
        assert_eq!(classify_cell("\t"), CellType::Empty);
    }

    #[test]
    fn ints() {
        assert_eq!(classify_cell("0"), CellType::Int);
        assert_eq!(classify_cell("42"), CellType::Int);
        assert_eq!(classify_cell("-17"), CellType::Int);
        assert_eq!(classify_cell("+5"), CellType::Int);
        assert_eq!(classify_cell("  99  "), CellType::Int);
    }

    #[test]
    fn floats() {
        assert_eq!(classify_cell("1.5"), CellType::Float);
        assert_eq!(classify_cell("-0.1"), CellType::Float);
        assert_eq!(classify_cell("1e9"), CellType::Float);
        assert_eq!(classify_cell("1.2E-3"), CellType::Float);
        assert_eq!(classify_cell(".5"), CellType::Float);
    }

    #[test]
    fn float_rejects_nan_inf_hex() {
        assert_eq!(classify_cell("NaN"), CellType::String);
        assert_eq!(classify_cell("inf"), CellType::String);
        assert_eq!(classify_cell("0x10"), CellType::String);
    }

    #[test]
    fn int_not_classified_as_float() {
        assert_eq!(classify_cell("42"), CellType::Int);
    }

    #[test]
    fn iso_dates() {
        assert_eq!(classify_cell("2024-01-15"), CellType::Date);
        assert_eq!(classify_cell("2024/01/15"), CellType::Date);
        assert_eq!(classify_cell("2024-01-15T09:30:00"), CellType::Date);
        assert_eq!(classify_cell("2024-01-15T09:30:00Z"), CellType::Date);
        assert_eq!(classify_cell("2024-01-15T09:30:00+05:30"), CellType::Date);
        assert_eq!(classify_cell("2024-01-15T09:30:00-08:00"), CellType::Date);
    }

    #[test]
    fn not_dates() {
        assert_eq!(classify_cell("01/15/2024"), CellType::String);
        assert_eq!(classify_cell("15-01-2024"), CellType::String);
        assert_eq!(classify_cell("2024-1-15"), CellType::String);
        assert_eq!(classify_cell("2024-01-15T09:30"), CellType::String);
        assert_eq!(classify_cell("2024-01-15 09:30:00"), CellType::String);
    }

    #[test]
    fn mixed_separators_rejected() {
        assert_eq!(classify_cell("2024-01/15"), CellType::String);
    }

    #[test]
    fn arbitrary_strings() {
        assert_eq!(classify_cell("alice"), CellType::String);
        assert_eq!(classify_cell("a1b2"), CellType::String);
        assert_eq!(classify_cell("true"), CellType::String);
    }

    fn column(vals: &[&str]) -> ColumnType {
        infer_column(vals.iter().enumerate().map(|(i, v)| (i + 1, *v)))
    }

    #[test]
    fn all_ints_is_int_column() {
        let c = column(&["1", "2", "3", "4"]);
        assert_eq!(c.primary, CellType::Int);
        assert_eq!(c.empty_count, 0);
        assert!(c.mismatch_rows.is_empty());
    }

    #[test]
    fn int_plus_float_promotes_to_float() {
        let c = column(&["1", "2.5", "3", "4.25"]);
        assert_eq!(c.primary, CellType::Float);
        assert!(c.mismatch_rows.is_empty());
    }

    #[test]
    fn empty_cells_do_not_count_against_type() {
        let c = column(&["1", "", "2", "", "3"]);
        assert_eq!(c.primary, CellType::Int);
        assert_eq!(c.empty_count, 2);
        assert!(c.mismatch_rows.is_empty());
    }

    #[test]
    fn outlier_is_flagged_when_column_typed() {
        // 9 ints, 1 string = 90% confidence, meets threshold.
        let mut vals: Vec<&str> = (0..9).map(|_| "1").collect();
        vals.push("oops");
        let c = column(&vals);
        assert_eq!(c.primary, CellType::Int);
        assert_eq!(c.mismatch_rows, vec![10]);
    }

    #[test]
    fn below_threshold_becomes_string_no_warnings() {
        // 2 ints, 1 string = 66%, below 90%.
        let c = column(&["1", "2", "oops"]);
        assert_eq!(c.primary, CellType::String);
        assert!(c.mismatch_rows.is_empty());
    }

    #[test]
    fn fewer_than_three_rows_stays_string() {
        let c = column(&["1", "2"]);
        assert_eq!(c.primary, CellType::String);
        assert!(c.mismatch_rows.is_empty());
    }

    #[test]
    fn all_empty_column() {
        let c = column(&["", "", ""]);
        assert_eq!(c.primary, CellType::Empty);
        assert_eq!(c.empty_count, 3);
    }

    #[test]
    fn infer_table_skips_header() {
        let rows: Vec<Vec<String>> = vec![
            vec!["id".into(), "when".into()],
            vec!["1".into(), "2024-01-01".into()],
            vec!["2".into(), "2024-02-01".into()],
            vec!["3".into(), "2024-03-01".into()],
        ];
        let cols = infer_table(&rows, true);
        assert_eq!(cols.len(), 2);
        assert_eq!(cols[0].primary, CellType::Int);
        assert_eq!(cols[1].primary, CellType::Date);
    }

    #[test]
    fn infer_table_without_header_uses_row_zero() {
        let rows: Vec<Vec<String>> = vec![
            vec!["1".into()],
            vec!["2".into()],
            vec!["3".into()],
            vec!["4".into()],
        ];
        let cols = infer_table(&rows, false);
        assert_eq!(cols[0].primary, CellType::Int);
    }

    #[test]
    fn infer_table_handles_ragged_rows() {
        let rows: Vec<Vec<String>> = vec![
            vec!["a".into(), "b".into(), "c".into()],
            vec!["1".into()],
            vec!["2".into(), "x".into()],
            vec!["3".into(), "y".into(), "z".into()],
        ];
        let cols = infer_table(&rows, true);
        assert_eq!(cols.len(), 3);
    }
}
