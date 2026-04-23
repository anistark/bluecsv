//! Per-column summary statistics.
//!
//! `summarize` is type-aware: min/max are computed numerically for Int/Float
//! columns, lexicographically for Date and String columns. `sum` and `mean`
//! are only populated for numeric columns.

use std::collections::BTreeSet;

use crate::types::CellType;

#[derive(Debug, Clone, PartialEq)]
pub struct ColumnStats {
    pub ty: CellType,
    pub count: usize,
    pub empty: usize,
    pub distinct: usize,
    pub min: Option<String>,
    pub max: Option<String>,
    pub sum: Option<f64>,
    pub mean: Option<f64>,
}

pub fn summarize<'a, I>(values: I, ty: CellType) -> ColumnStats
where
    I: IntoIterator<Item = &'a str>,
{
    let mut count = 0usize;
    let mut empty = 0usize;
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut numeric: Vec<f64> = Vec::new();
    let mut string_min: Option<String> = None;
    let mut string_max: Option<String> = None;

    for raw in values {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            empty += 1;
            continue;
        }
        count += 1;
        seen.insert(trimmed.to_string());

        match ty {
            CellType::Int | CellType::Float => {
                if let Ok(n) = trimmed.parse::<f64>() {
                    numeric.push(n);
                }
            }
            _ => {
                match &string_min {
                    Some(cur) if trimmed >= cur.as_str() => {}
                    _ => string_min = Some(trimmed.to_string()),
                }
                match &string_max {
                    Some(cur) if trimmed <= cur.as_str() => {}
                    _ => string_max = Some(trimmed.to_string()),
                }
            }
        }
    }

    let (min, max, sum, mean) = match ty {
        CellType::Int | CellType::Float if !numeric.is_empty() => {
            let min_n = numeric.iter().copied().fold(f64::INFINITY, f64::min);
            let max_n = numeric.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let s: f64 = numeric.iter().sum();
            let m = s / numeric.len() as f64;
            (
                Some(format_number(min_n, ty)),
                Some(format_number(max_n, ty)),
                Some(s),
                Some(m),
            )
        }
        _ => (string_min, string_max, None, None),
    };

    ColumnStats {
        ty,
        count,
        empty,
        distinct: seen.len(),
        min,
        max,
        sum,
        mean,
    }
}

fn format_number(n: f64, ty: CellType) -> String {
    if ty == CellType::Int && n.fract() == 0.0 && n.abs() < 1e16 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(vals: &[&str], ty: CellType) -> ColumnStats {
        summarize(vals.iter().copied(), ty)
    }

    #[test]
    fn int_column_stats() {
        let s = stats(&["1", "2", "3", "4"], CellType::Int);
        assert_eq!(s.count, 4);
        assert_eq!(s.distinct, 4);
        assert_eq!(s.min.as_deref(), Some("1"));
        assert_eq!(s.max.as_deref(), Some("4"));
        assert_eq!(s.sum, Some(10.0));
        assert_eq!(s.mean, Some(2.5));
    }

    #[test]
    fn float_column_stats() {
        let s = stats(&["1.5", "2.5", "3.0"], CellType::Float);
        assert_eq!(s.count, 3);
        assert_eq!(s.sum, Some(7.0));
        assert_eq!(s.mean, Some(7.0 / 3.0));
        assert_eq!(s.min.as_deref(), Some("1.5"));
        assert_eq!(s.max.as_deref(), Some("3"));
    }

    #[test]
    fn date_column_stats_lexical() {
        let s = stats(&["2024-03-01", "2024-01-15", "2024-02-10"], CellType::Date);
        assert_eq!(s.min.as_deref(), Some("2024-01-15"));
        assert_eq!(s.max.as_deref(), Some("2024-03-01"));
        assert_eq!(s.sum, None);
        assert_eq!(s.mean, None);
    }

    #[test]
    fn string_column_stats() {
        let s = stats(&["banana", "apple", "cherry"], CellType::String);
        assert_eq!(s.min.as_deref(), Some("apple"));
        assert_eq!(s.max.as_deref(), Some("cherry"));
        assert_eq!(s.distinct, 3);
    }

    #[test]
    fn empties_counted_separately() {
        let s = stats(&["1", "", "2", ""], CellType::Int);
        assert_eq!(s.count, 2);
        assert_eq!(s.empty, 2);
        assert_eq!(s.distinct, 2);
    }

    #[test]
    fn distinct_deduplicates() {
        let s = stats(&["a", "a", "b", "a"], CellType::String);
        assert_eq!(s.count, 4);
        assert_eq!(s.distinct, 2);
    }

    #[test]
    fn all_empty() {
        let s = stats(&["", "", ""], CellType::Empty);
        assert_eq!(s.count, 0);
        assert_eq!(s.empty, 3);
        assert_eq!(s.min, None);
        assert_eq!(s.max, None);
    }

    #[test]
    fn outlier_string_in_int_column_ignored_for_numeric_stats() {
        let s = stats(&["1", "2", "oops", "4"], CellType::Int);
        assert_eq!(s.sum, Some(7.0));
        assert_eq!(s.count, 4);
        assert_eq!(s.distinct, 4);
    }
}
