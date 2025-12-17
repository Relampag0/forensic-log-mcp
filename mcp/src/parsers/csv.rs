use polars::prelude::*;
use std::path::Path;
use super::ParseError;

/// CSV/TSV parser using Polars native reader

pub fn parse(path: &Path) -> Result<LazyFrame, ParseError> {
    // Detect delimiter by examining first line
    let first_line = std::fs::read_to_string(path)
        .map(|s| s.lines().next().unwrap_or("").to_string())
        .unwrap_or_default();

    let separator = if first_line.contains('\t') {
        b'\t'
    } else {
        b','
    };

    let lf = LazyCsvReader::new(path)
        .with_has_header(true)
        .with_separator(separator)
        .with_infer_schema_length(Some(1000))
        .with_ignore_errors(true)
        .finish()
        .map_err(ParseError::from)?;

    Ok(lf)
}
