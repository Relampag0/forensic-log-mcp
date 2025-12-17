use polars::prelude::*;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum QueryError {
    #[error("Polars error: {0}")]
    PolarsError(#[from] PolarsError),
    #[error("Invalid query: {0}")]
    InvalidQuery(String),
}

/// Query builder for constructing Polars queries
pub struct QueryBuilder {
    lf: LazyFrame,
}

impl QueryBuilder {
    pub fn new(lf: LazyFrame) -> Self {
        Self { lf }
    }

    /// Filter by text pattern in a column (case-insensitive by default)
    pub fn filter_text(mut self, column: &str, pattern: &str, case_sensitive: bool) -> Self {
        let expr = if case_sensitive {
            col(column).str().contains(lit(pattern), true)
        } else {
            col(column).str().to_lowercase().str().contains(lit(pattern.to_lowercase()), true)
        };
        self.lf = self.lf.filter(expr);
        self
    }

    /// Filter by regex pattern
    pub fn filter_regex(mut self, column: &str, pattern: &str) -> Self {
        self.lf = self.lf.filter(col(column).str().contains(lit(pattern), false));
        self
    }

    /// Filter by status code (supports ranges like ">=400", "4xx", "500")
    pub fn filter_status(mut self, status_filter: &str) -> Self {
        let expr = parse_status_filter(status_filter);
        if let Some(e) = expr {
            self.lf = self.lf.filter(e);
        }
        self
    }

    /// Filter by time range
    pub fn filter_time_range(mut self, column: &str, start: Option<&str>, end: Option<&str>) -> Self {
        if let Some(start_time) = start {
            self.lf = self.lf.filter(col(column).gt_eq(lit(start_time)));
        }
        if let Some(end_time) = end {
            self.lf = self.lf.filter(col(column).lt_eq(lit(end_time)));
        }
        self
    }

    /// Group by a column
    pub fn group_by(self, column: &str) -> GroupByBuilder {
        GroupByBuilder {
            lf: self.lf,
            group_col: column.to_string(),
        }
    }

    /// Sort by a column
    pub fn sort(mut self, column: &str, descending: bool) -> Self {
        self.lf = self.lf.sort([column], SortMultipleOptions::default().with_order_descending(descending));
        self
    }

    /// Limit results
    pub fn limit(mut self, n: u32) -> Self {
        self.lf = self.lf.limit(n);
        self
    }

    /// Select specific columns
    pub fn select(mut self, columns: &[&str]) -> Self {
        let exprs: Vec<Expr> = columns.iter().map(|c| col(*c)).collect();
        self.lf = self.lf.select(exprs);
        self
    }

    /// Execute the query and return results
    pub fn collect(self) -> Result<DataFrame, QueryError> {
        self.lf.collect().map_err(QueryError::from)
    }

    /// Get the LazyFrame for further manipulation
    pub fn into_lazy(self) -> LazyFrame {
        self.lf
    }
}

pub struct GroupByBuilder {
    lf: LazyFrame,
    group_col: String,
}

impl GroupByBuilder {
    /// Count occurrences per group
    pub fn count(self) -> QueryBuilder {
        let lf = self.lf
            .group_by([col(&self.group_col)])
            .agg([col(&self.group_col).count().alias("count")])
            .sort(["count"], SortMultipleOptions::default().with_order_descending(true));
        QueryBuilder { lf }
    }

    /// Sum a column per group
    pub fn sum(self, column: &str) -> QueryBuilder {
        let lf = self.lf
            .group_by([col(&self.group_col)])
            .agg([col(column).sum().alias("sum")])
            .sort(["sum"], SortMultipleOptions::default().with_order_descending(true));
        QueryBuilder { lf }
    }

    /// Average a column per group
    pub fn avg(self, column: &str) -> QueryBuilder {
        let lf = self.lf
            .group_by([col(&self.group_col)])
            .agg([col(column).mean().alias("avg")])
            .sort(["avg"], SortMultipleOptions::default().with_order_descending(true));
        QueryBuilder { lf }
    }

    /// Min value per group
    pub fn min(self, column: &str) -> QueryBuilder {
        let lf = self.lf
            .group_by([col(&self.group_col)])
            .agg([col(column).min().alias("min")]);
        QueryBuilder { lf }
    }

    /// Max value per group
    pub fn max(self, column: &str) -> QueryBuilder {
        let lf = self.lf
            .group_by([col(&self.group_col)])
            .agg([col(column).max().alias("max")]);
        QueryBuilder { lf }
    }

    /// Count unique values per group
    pub fn unique_count(self, column: &str) -> QueryBuilder {
        let lf = self.lf
            .group_by([col(&self.group_col)])
            .agg([col(column).n_unique().alias("unique_count")])
            .sort(["unique_count"], SortMultipleOptions::default().with_order_descending(true));
        QueryBuilder { lf }
    }
}

/// Parse status filter expressions like ">=400", "4xx", "500"
fn parse_status_filter(filter: &str) -> Option<Expr> {
    let filter = filter.trim();

    // Handle range patterns like "4xx", "5xx"
    if filter.ends_with("xx") && filter.len() == 3 {
        if let Some(prefix) = filter.chars().next().and_then(|c| c.to_digit(10)) {
            let min = (prefix * 100) as i32;
            let max = min + 99;
            return Some(col("status").gt_eq(lit(min)).and(col("status").lt_eq(lit(max))));
        }
    }

    // Handle comparison operators
    if filter.starts_with(">=") {
        if let Ok(val) = filter[2..].trim().parse::<i32>() {
            return Some(col("status").gt_eq(lit(val)));
        }
    } else if filter.starts_with("<=") {
        if let Ok(val) = filter[2..].trim().parse::<i32>() {
            return Some(col("status").lt_eq(lit(val)));
        }
    } else if filter.starts_with('>') {
        if let Ok(val) = filter[1..].trim().parse::<i32>() {
            return Some(col("status").gt(lit(val)));
        }
    } else if filter.starts_with('<') {
        if let Ok(val) = filter[1..].trim().parse::<i32>() {
            return Some(col("status").lt(lit(val)));
        }
    } else if filter.starts_with("!=") {
        if let Ok(val) = filter[2..].trim().parse::<i32>() {
            return Some(col("status").neq(lit(val)));
        }
    } else if let Ok(val) = filter.parse::<i32>() {
        // Exact match
        return Some(col("status").eq(lit(val)));
    }

    None
}

/// Convert DataFrame to JSON string
pub fn dataframe_to_json(df: &DataFrame) -> Result<String, QueryError> {
    let mut buf = Vec::new();
    JsonWriter::new(&mut buf)
        .with_json_format(JsonFormat::Json)
        .finish(&mut df.clone())
        .map_err(|e| QueryError::PolarsError(e))?;
    String::from_utf8(buf).map_err(|e| QueryError::InvalidQuery(e.to_string()))
}

/// Get schema information from a DataFrame
pub fn get_schema_info(df: &DataFrame) -> Vec<(String, String)> {
    df.schema()
        .iter()
        .map(|(name, dtype)| (name.to_string(), dtype.to_string()))
        .collect()
}
