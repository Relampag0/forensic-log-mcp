use polars::prelude::*;
use std::path::Path;
use super::ParseError;

/// JSON Lines (NDJSON) parser
/// Each line is a separate JSON object

pub fn parse(path: &Path) -> Result<LazyFrame, ParseError> {
    // Use Polars native JSON reader for efficiency
    let lf = LazyJsonLineReader::new(path)
        .finish()
        .map_err(ParseError::from)?;

    Ok(lf)
}
