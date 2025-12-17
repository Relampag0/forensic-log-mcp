use polars::prelude::*;
use rayon::prelude::*;
use regex::Regex;
use std::fs::File;
use std::path::Path;
use memmap2::Mmap;
use super::ParseError;

/// Syslog parser - Optimized version
/// Uses memory-mapped I/O + parallel processing with rayon
///
/// Supports RFC 3164 format (most common):
/// Example: Dec 10 10:45:23 myhost sshd[12345]: Accepted publickey for user

// Parsed syslog entry
struct SyslogEntry {
    priority: Option<i32>,
    timestamp: String,
    hostname: String,
    process: String,
    pid: Option<i32>,
    message: String,
    level: String,
    raw: String,
}

/// Convert syslog priority to human-readable level
fn priority_to_level(priority: Option<i32>) -> String {
    match priority {
        Some(p) => {
            let severity = p % 8;
            match severity {
                0 => "EMERGENCY",
                1 => "ALERT",
                2 => "CRITICAL",
                3 => "ERROR",
                4 => "WARNING",
                5 => "NOTICE",
                6 => "INFO",
                7 => "DEBUG",
                _ => "UNKNOWN",
            }.to_string()
        }
        None => "UNKNOWN".to_string()
    }
}

pub fn parse(path: &Path) -> Result<LazyFrame, ParseError> {
    // Memory-map the file for efficient reading
    let file = File::open(path)?;
    let mmap = unsafe { Mmap::map(&file)? };
    let content = std::str::from_utf8(&mmap)
        .map_err(|e| ParseError::ParseFailed(format!("Invalid UTF-8: {}", e)))?;

    // RFC 3164 style regex
    let re_3164 = Regex::new(
        r"^(?:<(\d+)>)?(\w{3}\s+\d+\s+\d+:\d+:\d+)\s+(\S+)\s+(\S+?)(?:\[(\d+)\])?:\s*(.*)$"
    ).unwrap();

    // Collect lines
    let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    let line_count = lines.len();

    // Parse in parallel
    let entries: Vec<SyslogEntry> = lines
        .par_iter()
        .map(|line| {
            let line = line.trim();

            if let Some(caps) = re_3164.captures(line) {
                let pri: Option<i32> = caps.get(1).and_then(|m| m.as_str().parse().ok());

                SyslogEntry {
                    priority: pri,
                    timestamp: caps.get(2).map(|m| m.as_str()).unwrap_or("").to_string(),
                    hostname: caps.get(3).map(|m| m.as_str()).unwrap_or("-").to_string(),
                    process: caps.get(4).map(|m| m.as_str()).unwrap_or("-").to_string(),
                    pid: caps.get(5).and_then(|m| m.as_str().parse().ok()),
                    message: caps.get(6).map(|m| m.as_str()).unwrap_or("").to_string(),
                    level: priority_to_level(pri),
                    raw: line.to_string(),
                }
            } else {
                // Unstructured log line
                SyslogEntry {
                    priority: None,
                    timestamp: "".to_string(),
                    hostname: "-".to_string(),
                    process: "-".to_string(),
                    pid: None,
                    message: line.to_string(),
                    level: "UNKNOWN".to_string(),
                    raw: line.to_string(),
                }
            }
        })
        .collect();

    // Build vectors from entries
    let mut priorities: Vec<Option<i32>> = Vec::with_capacity(line_count);
    let mut timestamps = Vec::with_capacity(line_count);
    let mut hostnames = Vec::with_capacity(line_count);
    let mut processes = Vec::with_capacity(line_count);
    let mut pids: Vec<Option<i32>> = Vec::with_capacity(line_count);
    let mut messages = Vec::with_capacity(line_count);
    let mut levels = Vec::with_capacity(line_count);
    let mut raw_lines = Vec::with_capacity(line_count);

    for entry in entries {
        priorities.push(entry.priority);
        timestamps.push(entry.timestamp);
        hostnames.push(entry.hostname);
        processes.push(entry.process);
        pids.push(entry.pid);
        messages.push(entry.message);
        levels.push(entry.level);
        raw_lines.push(entry.raw);
    }

    let df = DataFrame::new(vec![
        Column::new("priority".into(), priorities),
        Column::new("timestamp".into(), timestamps),
        Column::new("hostname".into(), hostnames),
        Column::new("process".into(), processes),
        Column::new("pid".into(), pids),
        Column::new("message".into(), messages),
        Column::new("level".into(), levels),
        Column::new("raw".into(), raw_lines),
    ])?;

    Ok(df.lazy())
}
