use polars::prelude::*;
use rayon::prelude::*;
use regex::Regex;
use std::fs::File;
use std::path::Path;
use memmap2::Mmap;
use super::ParseError;

/// Apache/Nginx Combined Log Format parser - Optimized version
/// Uses memory-mapped I/O + parallel processing with rayon
///
/// Format: %h %l %u %t "%r" %>s %b "%{Referer}i" "%{User-agent}i"
/// Example: 192.168.1.1 - - [10/Oct/2024:13:55:36 +0000] "GET /index.html HTTP/1.1" 200 2326 "-" "Mozilla/5.0"

// Parsed log entry - using Option for optional fields
struct ApacheLogEntry {
    ip: String,
    timestamp: String,
    request: String,
    method: Option<String>,
    path: Option<String>,
    status: Option<i32>,
    size: Option<i64>,
    referer: Option<String>,
    user_agent: Option<String>,
    raw: String,
}

pub fn parse(path: &Path) -> Result<LazyFrame, ParseError> {
    // Memory-map the file for efficient reading
    let file = File::open(path)?;
    let mmap = unsafe { Mmap::map(&file)? };
    let content = std::str::from_utf8(&mmap)
        .map_err(|e| ParseError::ParseFailed(format!("Invalid UTF-8: {}", e)))?;

    // Compile regex once
    let re = Regex::new(
        r#"^(\S+)\s+(\S+)\s+(\S+)\s+\[([^\]]+)\]\s+"([^"]+)"\s+(\d+)\s+(\S+)(?:\s+"([^"]*)")?(?:\s+"([^"]*)")?$"#
    ).unwrap();

    // Collect lines (avoids multiple iterations)
    let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    let line_count = lines.len();

    // Pre-allocate with capacity
    let entries: Vec<ApacheLogEntry> = lines
        .par_iter()  // Parallel iteration with rayon
        .map(|line| {
            let line = line.trim();

            if let Some(caps) = re.captures(line) {
                let request = caps.get(5).map(|m| m.as_str()).unwrap_or("");
                let parts: Vec<&str> = request.split_whitespace().collect();

                ApacheLogEntry {
                    ip: caps.get(1).map(|m| m.as_str()).unwrap_or("-").to_string(),
                    timestamp: caps.get(4).map(|m| m.as_str()).unwrap_or("").to_string(),
                    request: request.to_string(),
                    method: parts.first().map(|s| s.to_string()),
                    path: parts.get(1).map(|s| s.to_string()),
                    status: caps.get(6).and_then(|m| m.as_str().parse().ok()),
                    size: caps.get(7).and_then(|m| {
                        let s = m.as_str();
                        if s == "-" { None } else { s.parse().ok() }
                    }),
                    referer: caps.get(8).map(|m| m.as_str().to_string()),
                    user_agent: caps.get(9).map(|m| m.as_str().to_string()),
                    raw: line.to_string(),
                }
            } else {
                // Line didn't match, store with defaults
                ApacheLogEntry {
                    ip: "-".to_string(),
                    timestamp: "".to_string(),
                    request: line.to_string(),
                    method: None,
                    path: None,
                    status: None,
                    size: None,
                    referer: None,
                    user_agent: None,
                    raw: line.to_string(),
                }
            }
        })
        .collect();

    // Build vectors from entries (single pass)
    let mut ips = Vec::with_capacity(line_count);
    let mut timestamps = Vec::with_capacity(line_count);
    let mut requests = Vec::with_capacity(line_count);
    let mut methods: Vec<Option<String>> = Vec::with_capacity(line_count);
    let mut paths: Vec<Option<String>> = Vec::with_capacity(line_count);
    let mut status_codes: Vec<Option<i32>> = Vec::with_capacity(line_count);
    let mut sizes: Vec<Option<i64>> = Vec::with_capacity(line_count);
    let mut referers: Vec<Option<String>> = Vec::with_capacity(line_count);
    let mut user_agents: Vec<Option<String>> = Vec::with_capacity(line_count);
    let mut raw_lines = Vec::with_capacity(line_count);

    for entry in entries {
        ips.push(entry.ip);
        timestamps.push(entry.timestamp);
        requests.push(entry.request);
        methods.push(entry.method);
        paths.push(entry.path);
        status_codes.push(entry.status);
        sizes.push(entry.size);
        referers.push(entry.referer);
        user_agents.push(entry.user_agent);
        raw_lines.push(entry.raw);
    }

    let df = DataFrame::new(vec![
        Column::new("ip".into(), ips),
        Column::new("timestamp".into(), timestamps),
        Column::new("request".into(), requests),
        Column::new("method".into(), methods),
        Column::new("path".into(), paths),
        Column::new("status".into(), status_codes),
        Column::new("size".into(), sizes),
        Column::new("referer".into(), referers),
        Column::new("user_agent".into(), user_agents),
        Column::new("raw".into(), raw_lines),
    ])?;

    Ok(df.lazy())
}
