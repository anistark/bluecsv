//! Bridge between `bluecsv`'s type inference and the LSP's positional `Model`.

use bluecsv::{infer_column, ColumnType};

use crate::model::Model;

/// Infer a type for every column in the model. Skips row 0 when `has_header`.
pub fn infer_model(model: &Model, has_header: bool) -> Vec<ColumnType> {
    let max_cols = model.cells.iter().map(|r| r.len()).max().unwrap_or(0);
    let skip = if has_header { 1 } else { 0 };
    (0..max_cols)
        .map(|col| {
            let values = model
                .cells
                .iter()
                .enumerate()
                .skip(skip)
                .filter_map(|(i, row)| row.get(col).map(|c| (i, c.value.as_str())));
            infer_column(values)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bluecsv::CellType;

    #[test]
    fn skips_header_row() {
        let m = Model::parse("id,when\n1,2024-01-01\n2,2024-02-01\n3,2024-03-01\n");
        let cols = infer_model(&m, true);
        assert_eq!(cols[0].primary, CellType::Int);
        assert_eq!(cols[1].primary, CellType::Date);
    }

    #[test]
    fn includes_row_zero_when_no_header() {
        let m = Model::parse("1\n2\n3\n4\n");
        let cols = infer_model(&m, false);
        assert_eq!(cols[0].primary, CellType::Int);
    }
}
