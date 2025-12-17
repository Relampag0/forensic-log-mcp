pub mod apache;
pub mod apache_simd;
pub mod csv;
pub mod json;
pub mod syslog;
pub mod syslog_simd;

use polars::prelude::*;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("Failed to read file: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Failed to parse log: {0}")]
    ParseFailed(String),
    #[error("Polars error: {0}")]
    PolarsError(#[from] PolarsError),
    #[error("Unknown format")]
    UnknownFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    Apache,
    Nginx,
    Syslog,
    Json,
    Csv,
    Auto,
}

impl LogFormat {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "apache" => LogFormat::Apache,
            "nginx" => LogFormat::Nginx,
            "syslog" => LogFormat::Syslog,
            "json" | "jsonl" | "ndjson" => LogFormat::Json,
            "csv" | "tsv" => LogFormat::Csv,
            _ => LogFormat::Auto,
        }
    }
}

/// Detect log format by examining file extension and content
pub fn detect_format(path: &Path) -> LogFormat {
    // First check extension
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        match ext.to_lowercase().as_str() {
            "json" | "jsonl" | "ndjson" => return LogFormat::Json,
            "csv" => return LogFormat::Csv,
            "tsv" => return LogFormat::Csv,
            _ => {}
        }
    }

    // Try to detect from content
    if let Ok(content) = std::fs::read_to_string(path).map(|s| s.lines().next().unwrap_or("").to_string()) {
        if content.starts_with('{') {
            return LogFormat::Json;
        }
        if content.contains(',') && !content.contains(" - - [") {
            return LogFormat::Csv;
        }
        if content.contains(" - - [") || content.contains("\" ") {
            return LogFormat::Apache;
        }
        if content.starts_with('<') || content.contains("]: ") {
            return LogFormat::Syslog;
        }
    }

    LogFormat::Auto
}

/// Parse logs from a file into a LazyFrame
pub fn parse_logs(path: &Path, format: LogFormat) -> Result<LazyFrame, ParseError> {
    let format = if format == LogFormat::Auto {
        detect_format(path)
    } else {
        format
    };

    match format {
        LogFormat::Apache | LogFormat::Nginx => apache::parse(path),
        LogFormat::Syslog => syslog::parse(path),
        LogFormat::Json => json::parse(path),
        LogFormat::Csv => csv::parse(path),
        LogFormat::Auto => {
            // Try each parser in order
            json::parse(path)
                .or_else(|_| csv::parse(path))
                .or_else(|_| apache::parse(path))
                .or_else(|_| syslog::parse(path))
        }
    }
}

/// Expand glob pattern and return matching file paths
pub fn expand_glob(pattern: &str) -> Result<Vec<std::path::PathBuf>, ParseError> {
    let paths: Vec<_> = glob::glob(pattern)
        .map_err(|e| ParseError::ParseFailed(e.to_string()))?
        .filter_map(Result::ok)
        .collect();

    if paths.is_empty() {
        // Maybe it's a single file path
        let path = Path::new(pattern);
        if path.exists() {
            return Ok(vec![path.to_path_buf()]);
        }
        return Err(ParseError::ParseFailed(format!("No files found matching: {}", pattern)));
    }

    Ok(paths)
}

/// Parse multiple files and concatenate into a single LazyFrame
pub fn parse_multiple(pattern: &str, format: LogFormat) -> Result<LazyFrame, ParseError> {
    let paths = expand_glob(pattern)?;

    let mut frames: Vec<LazyFrame> = Vec::new();

    for path in &paths {
        let mut lf = parse_logs(path, format)?;
        // Add source file column
        let file_name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        lf = lf.with_column(lit(file_name).alias("_source_file"));
        frames.push(lf);
    }

    if frames.is_empty() {
        return Err(ParseError::ParseFailed("No valid log files found".to_string()));
    }

    // Concatenate all frames
    let args = UnionArgs::default();
    concat(&frames, args).map_err(ParseError::from)
}
