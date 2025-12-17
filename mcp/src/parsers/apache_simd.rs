//! Generalized SIMD-accelerated Apache log parser
//!
//! Key optimizations:
//! 1. SIMD field boundary detection using memchr
//! 2. Lazy field extraction - only parse fields needed for query
//! 3. Predicate pushdown - filter during scan, not after
//! 4. Zero-copy where possible - work with byte slices
//! 5. Parallel chunk processing with local aggregation
//! 6. Regex support via regex crate (has SIMD optimizations)
//! 7. Numeric aggregations (sum, avg, min, max) on size field
//! 8. Time range filtering with fast timestamp comparison

use memchr::memchr;
use memmap2::Mmap;
use rayon::prelude::*;
use regex::bytes::Regex;
use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

use super::ParseError;

/// Field positions within a log line (byte offsets)
/// Apache format: IP - - [TIMESTAMP] "METHOD PATH PROTO" STATUS SIZE "REF" "UA"
#[derive(Debug, Clone, Copy)]
struct FieldOffsets {
    ip_end: usize,
    timestamp_start: usize, // After '['
    timestamp_end: usize,   // Before ']'
    request_start: usize,
    request_end: usize,
    status_start: usize,
    size_start: usize,
    size_end: usize,
    referer_start: usize,   // After opening quote
    referer_end: usize,     // Before closing quote
    user_agent_start: usize,
    user_agent_end: usize,
}

/// Find field boundaries using SIMD-accelerated byte search
/// Returns None if line is malformed
#[inline]
fn find_fields(line: &[u8]) -> Option<FieldOffsets> {
    let len = line.len();
    if len < 20 {
        return None; // Too short to be valid
    }

    // IP ends at first space
    let ip_end = memchr(b' ', line)?;

    // Timestamp is between [ and ]
    let bracket_open = memchr(b'[', line)?;
    let timestamp_start = bracket_open + 1;
    let bracket_close = memchr(b']', &line[bracket_open..])?;
    let timestamp_end = bracket_open + bracket_close;

    // Request is between first pair of quotes after timestamp
    let quote1 = memchr(b'"', &line[timestamp_end..])?;
    let request_start = timestamp_end + quote1 + 1;
    let quote2 = memchr(b'"', &line[request_start..])?;
    let request_end = request_start + quote2;

    // Status starts after the closing quote + space
    let status_start = request_end + 2; // skip '" '

    if status_start + 3 > len {
        return None;
    }

    // Size starts after status + space (status is 3 digits)
    let size_start = status_start + 4; // skip "XXX "

    // Size ends at next space or end of field section
    let size_end = if size_start < len {
        memchr(b' ', &line[size_start..])
            .map(|i| size_start + i)
            .unwrap_or_else(|| {
                // Find end before next quote or end of line
                memchr(b'"', &line[size_start..])
                    .map(|i| size_start + i - 1)
                    .unwrap_or(len)
            })
    } else {
        len
    };

    // Referer is the next quoted field after size: "REFERER"
    let (referer_start, referer_end) = if size_end < len {
        if let Some(ref_quote1) = memchr(b'"', &line[size_end..]) {
            let ref_start = size_end + ref_quote1 + 1;
            if let Some(ref_quote2) = memchr(b'"', &line[ref_start..]) {
                (ref_start, ref_start + ref_quote2)
            } else {
                (0, 0)
            }
        } else {
            (0, 0)
        }
    } else {
        (0, 0)
    };

    // User-Agent is the next quoted field after referer: "USER_AGENT"
    let (user_agent_start, user_agent_end) = if referer_end > 0 && referer_end + 1 < len {
        if let Some(ua_quote1) = memchr(b'"', &line[referer_end + 1..]) {
            let ua_start = referer_end + 1 + ua_quote1 + 1;
            if let Some(ua_quote2) = memchr(b'"', &line[ua_start..]) {
                (ua_start, ua_start + ua_quote2)
            } else {
                (0, 0)
            }
        } else {
            (0, 0)
        }
    } else {
        (0, 0)
    };

    Some(FieldOffsets {
        ip_end,
        timestamp_start,
        timestamp_end,
        request_start,
        request_end,
        status_start,
        size_start,
        size_end,
        referer_start,
        referer_end,
        user_agent_start,
        user_agent_end,
    })
}

/// Extract IP from line using pre-computed offsets
#[inline]
fn extract_ip<'a>(line: &'a [u8], offsets: &FieldOffsets) -> &'a [u8] {
    &line[..offsets.ip_end]
}

/// Extract status code (3 digits) as u16
#[inline]
fn extract_status(line: &[u8], offsets: &FieldOffsets) -> Option<u16> {
    let start = offsets.status_start;
    if start + 3 > line.len() {
        return None;
    }
    let s0 = line[start];
    let s1 = line[start + 1];
    let s2 = line[start + 2];
    if s0.is_ascii_digit() && s1.is_ascii_digit() && s2.is_ascii_digit() {
        Some((s0 - b'0') as u16 * 100 + (s1 - b'0') as u16 * 10 + (s2 - b'0') as u16)
    } else {
        None
    }
}

/// Extract method from request (first word)
#[inline]
fn extract_method<'a>(line: &'a [u8], offsets: &FieldOffsets) -> &'a [u8] {
    let request = &line[offsets.request_start..offsets.request_end];
    let end = memchr(b' ', request).unwrap_or(request.len());
    &request[..end]
}

/// Extract path from request (second word)
#[inline]
fn extract_path<'a>(line: &'a [u8], offsets: &FieldOffsets) -> &'a [u8] {
    let request = &line[offsets.request_start..offsets.request_end];
    let method_end = memchr(b' ', request).unwrap_or(request.len());
    if method_end >= request.len() {
        return &[];
    }
    let path_start = method_end + 1;
    let path_end = memchr(b' ', &request[path_start..])
        .map(|i| path_start + i)
        .unwrap_or(request.len());
    &request[path_start..path_end]
}

/// Extract full request line
#[inline]
fn extract_request<'a>(line: &'a [u8], offsets: &FieldOffsets) -> &'a [u8] {
    &line[offsets.request_start..offsets.request_end]
}

/// Extract referer field
#[inline]
fn extract_referer<'a>(line: &'a [u8], offsets: &FieldOffsets) -> &'a [u8] {
    if offsets.referer_start > 0 && offsets.referer_end > offsets.referer_start && offsets.referer_end <= line.len() {
        &line[offsets.referer_start..offsets.referer_end]
    } else {
        b"-"
    }
}

/// Extract user-agent field
#[inline]
fn extract_user_agent<'a>(line: &'a [u8], offsets: &FieldOffsets) -> &'a [u8] {
    if offsets.user_agent_start > 0 && offsets.user_agent_end > offsets.user_agent_start && offsets.user_agent_end <= line.len() {
        &line[offsets.user_agent_start..offsets.user_agent_end]
    } else {
        b"-"
    }
}

/// Extract size as i64 (handles "-" for missing size)
#[inline]
fn extract_size(line: &[u8], offsets: &FieldOffsets) -> Option<i64> {
    if offsets.size_start >= offsets.size_end || offsets.size_end > line.len() {
        return None;
    }
    let size_bytes = &line[offsets.size_start..offsets.size_end];
    if size_bytes == b"-" {
        return Some(0);
    }
    // Fast integer parsing without allocation
    let mut result: i64 = 0;
    for &b in size_bytes {
        if b.is_ascii_digit() {
            result = result * 10 + (b - b'0') as i64;
        } else {
            return None;
        }
    }
    Some(result)
}

/// Extract timestamp as bytes
#[inline]
fn extract_timestamp<'a>(line: &'a [u8], offsets: &FieldOffsets) -> &'a [u8] {
    &line[offsets.timestamp_start..offsets.timestamp_end]
}

/// Month name to number lookup for timestamp parsing
const MONTHS: [(&[u8], u8); 12] = [
    (b"Jan", 1), (b"Feb", 2), (b"Mar", 3), (b"Apr", 4),
    (b"May", 5), (b"Jun", 6), (b"Jul", 7), (b"Aug", 8),
    (b"Sep", 9), (b"Oct", 10), (b"Nov", 11), (b"Dec", 12),
];

/// Parse Apache timestamp to comparable i64 (YYYYMMDDHHmmss format)
/// Input format: "16/Dec/2025:11:26:41 +0000"
#[inline]
fn parse_timestamp_to_i64(ts: &[u8]) -> Option<i64> {
    if ts.len() < 20 {
        return None;
    }

    // Day: bytes 0-1
    let day = parse_2digit(&ts[0..2])?;

    // Month: bytes 3-5
    let month_bytes = &ts[3..6];
    let month = MONTHS.iter()
        .find(|(name, _)| *name == month_bytes)
        .map(|(_, num)| *num)?;

    // Year: bytes 7-10
    let year = parse_4digit(&ts[7..11])?;

    // Hour: bytes 12-13
    let hour = parse_2digit(&ts[12..14])?;

    // Minute: bytes 15-16
    let minute = parse_2digit(&ts[15..17])?;

    // Second: bytes 18-19
    let second = parse_2digit(&ts[18..20])?;

    // Combine into sortable i64: YYYYMMDDHHmmss
    Some(
        (year as i64) * 10000000000 +
        (month as i64) * 100000000 +
        (day as i64) * 1000000 +
        (hour as i64) * 10000 +
        (minute as i64) * 100 +
        (second as i64)
    )
}

#[inline]
fn parse_2digit(b: &[u8]) -> Option<i32> {
    if b.len() < 2 {
        return None;
    }
    let d0 = b[0].wrapping_sub(b'0');
    let d1 = b[1].wrapping_sub(b'0');
    if d0 < 10 && d1 < 10 {
        Some((d0 as i32) * 10 + (d1 as i32))
    } else {
        None
    }
}

#[inline]
fn parse_4digit(b: &[u8]) -> Option<i32> {
    if b.len() < 4 {
        return None;
    }
    let d0 = b[0].wrapping_sub(b'0');
    let d1 = b[1].wrapping_sub(b'0');
    let d2 = b[2].wrapping_sub(b'0');
    let d3 = b[3].wrapping_sub(b'0');
    if d0 < 10 && d1 < 10 && d2 < 10 && d3 < 10 {
        Some((d0 as i32) * 1000 + (d1 as i32) * 100 + (d2 as i32) * 10 + (d3 as i32))
    } else {
        None
    }
}

/// Time range filter
#[derive(Debug, Clone, Copy)]
pub struct TimeFilter {
    pub start: Option<i64>,  // Inclusive
    pub end: Option<i64>,    // Inclusive
}

impl TimeFilter {
    pub fn new(start: Option<&str>, end: Option<&str>) -> Option<Self> {
        let start_val = start.and_then(|s| Self::parse_time_input(s));
        let end_val = end.and_then(|s| Self::parse_time_input(s));

        if start_val.is_none() && end_val.is_none() {
            return None;
        }

        Some(TimeFilter {
            start: start_val,
            end: end_val,
        })
    }

    /// Parse various time input formats to i64
    /// Supports: "2025-12-16", "2025-12-16T11:26:41", "16/Dec/2025:11:26:41"
    fn parse_time_input(s: &str) -> Option<i64> {
        let s = s.trim();

        // ISO format: 2025-12-16 or 2025-12-16T11:26:41
        if s.len() >= 10 && s.as_bytes()[4] == b'-' {
            let year = s[0..4].parse::<i32>().ok()?;
            let month = s[5..7].parse::<i32>().ok()?;
            let day = s[8..10].parse::<i32>().ok()?;

            let (hour, minute, second) = if s.len() >= 19 && s.as_bytes()[10] == b'T' {
                (
                    s[11..13].parse::<i32>().ok()?,
                    s[14..16].parse::<i32>().ok()?,
                    s[17..19].parse::<i32>().ok()?,
                )
            } else {
                (0, 0, 0)
            };

            return Some(
                (year as i64) * 10000000000 +
                (month as i64) * 100000000 +
                (day as i64) * 1000000 +
                (hour as i64) * 10000 +
                (minute as i64) * 100 +
                (second as i64)
            );
        }

        // Apache format: 16/Dec/2025:11:26:41
        if s.len() >= 20 && s.as_bytes()[2] == b'/' {
            return parse_timestamp_to_i64(s.as_bytes());
        }

        None
    }

    #[inline]
    pub fn matches(&self, ts_i64: i64) -> bool {
        if let Some(start) = self.start {
            if ts_i64 < start {
                return false;
            }
        }
        if let Some(end) = self.end {
            if ts_i64 > end {
                return false;
            }
        }
        true
    }
}

/// Status filter operations
#[derive(Debug, Clone, Copy)]
pub enum StatusFilter {
    GreaterOrEqual(u16),
    LessOrEqual(u16),
    Equal(u16),
    Range(u16, u16), // inclusive
}

impl StatusFilter {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.starts_with(">=") {
            s[2..].trim().parse().ok().map(StatusFilter::GreaterOrEqual)
        } else if s.starts_with("<=") {
            s[2..].trim().parse().ok().map(StatusFilter::LessOrEqual)
        } else if s.starts_with('>') {
            s[1..].trim().parse::<u16>().ok().map(|v| StatusFilter::GreaterOrEqual(v + 1))
        } else if s.starts_with('<') {
            s[1..].trim().parse::<u16>().ok().map(|v| StatusFilter::LessOrEqual(v - 1))
        } else if s.starts_with('=') {
            s[1..].trim().parse().ok().map(StatusFilter::Equal)
        } else if s.contains("xx") {
            // Handle 4xx, 5xx patterns
            let first = s.chars().next()?;
            if first.is_ascii_digit() {
                let base = (first as u16 - b'0' as u16) * 100;
                Some(StatusFilter::Range(base, base + 99))
            } else {
                None
            }
        } else {
            s.parse().ok().map(StatusFilter::Equal)
        }
    }

    #[inline]
    pub fn matches(&self, status: u16) -> bool {
        match self {
            StatusFilter::GreaterOrEqual(v) => status >= *v,
            StatusFilter::LessOrEqual(v) => status <= *v,
            StatusFilter::Equal(v) => status == *v,
            StatusFilter::Range(lo, hi) => status >= *lo && status <= *hi,
        }
    }
}

/// Column to group by
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupByColumn {
    Ip,
    Path,
    Method,
    Status,
    Referer,
    UserAgent,
}

impl GroupByColumn {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "ip" | "remote_addr" | "client" => Some(GroupByColumn::Ip),
            "path" | "uri" | "url" | "request_path" => Some(GroupByColumn::Path),
            "method" | "request_method" | "http_method" => Some(GroupByColumn::Method),
            "status" | "status_code" | "http_status" => Some(GroupByColumn::Status),
            "referer" | "referrer" | "http_referer" => Some(GroupByColumn::Referer),
            "user_agent" | "useragent" | "ua" | "http_user_agent" => Some(GroupByColumn::UserAgent),
            _ => None,
        }
    }
}

// ============================================================================
// GREP-LIKE FAST COUNTING (no parsing, maximum speed)
// ============================================================================

/// Pure grep-like counting - NO parsing, just regex match + count
/// This is the fastest possible path for simple line counting
/// Uses SIMD via memchr for line splitting and regex crate's SIMD for matching
pub fn count_matches(path: &Path, pattern: &str) -> Result<usize, ParseError> {
    let file = File::open(path)?;
    let mmap = unsafe { Mmap::map(&file)? };
    let data = &mmap[..];

    let regex = Regex::new(pattern)
        .map_err(|e| ParseError::ParseFailed(format!("Invalid regex: {}", e)))?;

    let chunk_size = 4 * 1024 * 1024; // 4MB chunks
    let chunk_bounds = find_chunk_boundaries(data, chunk_size);

    let count: usize = chunk_bounds
        .par_windows(2)
        .map(|window| {
            let chunk = &data[window[0]..window[1]];
            let mut local_count = 0;
            let mut pos = 0;

            while pos < chunk.len() {
                let line_end = memchr(b'\n', &chunk[pos..])
                    .map(|i| pos + i)
                    .unwrap_or(chunk.len());
                let line = &chunk[pos..line_end];

                if regex.is_match(line) {
                    local_count += 1;
                }

                pos = line_end + 1;
            }
            local_count
        })
        .sum();

    Ok(count)
}

/// Count matches across multiple files (glob pattern support)
pub fn count_matches_multi(paths: &[&Path], pattern: &str) -> Result<usize, ParseError> {
    let regex = Regex::new(pattern)
        .map_err(|e| ParseError::ParseFailed(format!("Invalid regex: {}", e)))?;

    let count: usize = paths
        .par_iter()
        .map(|path| {
            let file = match File::open(path) {
                Ok(f) => f,
                Err(_) => return 0,
            };
            let mmap = match unsafe { Mmap::map(&file) } {
                Ok(m) => m,
                Err(_) => return 0,
            };
            let data = &mmap[..];

            let chunk_size = 4 * 1024 * 1024;
            let chunk_bounds = find_chunk_boundaries(data, chunk_size);

            chunk_bounds
                .par_windows(2)
                .map(|window| {
                    let chunk = &data[window[0]..window[1]];
                    let mut local_count = 0;
                    let mut pos = 0;

                    while pos < chunk.len() {
                        let line_end = memchr(b'\n', &chunk[pos..])
                            .map(|i| pos + i)
                            .unwrap_or(chunk.len());
                        let line = &chunk[pos..line_end];

                        if regex.is_match(line) {
                            local_count += 1;
                        }

                        pos = line_end + 1;
                    }
                    local_count
                })
                .sum::<usize>()
        })
        .sum();

    Ok(count)
}

// ============================================================================
// GENERALIZED FAST OPERATIONS
// ============================================================================

/// Count lines matching a status filter - accurate version
/// No false positives because we properly parse field boundaries
pub fn count_status(path: &Path, filter: StatusFilter) -> Result<usize, ParseError> {
    let file = File::open(path)?;
    let mmap = unsafe { Mmap::map(&file)? };
    let data = &mmap[..];

    let chunk_size = 4 * 1024 * 1024; // 4MB chunks
    let chunk_bounds = find_chunk_boundaries(data, chunk_size);

    let count: usize = chunk_bounds
        .par_windows(2)
        .map(|window| {
            let chunk = &data[window[0]..window[1]];
            let mut local_count = 0;
            let mut pos = 0;

            while pos < chunk.len() {
                // Find end of line
                let line_end = memchr(b'\n', &chunk[pos..])
                    .map(|i| pos + i)
                    .unwrap_or(chunk.len());
                let line = &chunk[pos..line_end];

                if let Some(offsets) = find_fields(line) {
                    if let Some(status) = extract_status(line, &offsets) {
                        if filter.matches(status) {
                            local_count += 1;
                        }
                    }
                }

                pos = line_end + 1;
            }
            local_count
        })
        .sum();

    Ok(count)
}

/// Filter lines matching status and optionally text pattern
/// Returns (total_count, matching_lines)
pub fn filter_lines(
    path: &Path,
    status_filter: Option<StatusFilter>,
    time_filter: Option<TimeFilter>,
    text_pattern: Option<&[u8]>,
    limit: usize,
) -> Result<(usize, Vec<String>), ParseError> {
    let file = File::open(path)?;
    let mmap = unsafe { Mmap::map(&file)? };
    let data = &mmap[..];

    let chunk_size = 4 * 1024 * 1024;
    let chunk_bounds = find_chunk_boundaries(data, chunk_size);

    // Use memmem for text pattern if provided
    let text_finder = text_pattern.map(memchr::memmem::Finder::new);

    // Parallel scan with early termination awareness
    let results: Vec<(usize, Vec<&[u8]>)> = chunk_bounds
        .par_windows(2)
        .map(|window| {
            let chunk = &data[window[0]..window[1]];
            let mut local_count = 0;
            let mut local_lines: Vec<&[u8]> = Vec::new();
            let mut pos = 0;

            while pos < chunk.len() {
                let line_end = memchr(b'\n', &chunk[pos..])
                    .map(|i| pos + i)
                    .unwrap_or(chunk.len());
                let line = &chunk[pos..line_end];

                let mut matches = true;
                let mut offsets_cached: Option<FieldOffsets> = None;

                // Check status filter or time filter (both need field parsing)
                if status_filter.is_some() || time_filter.is_some() {
                    if let Some(offsets) = find_fields(line) {
                        offsets_cached = Some(offsets);

                        // Check status filter
                        if let Some(ref filter) = status_filter {
                            if let Some(status) = extract_status(line, &offsets) {
                                if !filter.matches(status) {
                                    matches = false;
                                }
                            } else {
                                matches = false;
                            }
                        }

                        // Check time filter
                        if matches {
                            if let Some(ref tfilter) = time_filter {
                                let ts = extract_timestamp(line, &offsets);
                                if let Some(ts_i64) = parse_timestamp_to_i64(ts) {
                                    if !tfilter.matches(ts_i64) {
                                        matches = false;
                                    }
                                } else {
                                    matches = false;
                                }
                            }
                        }
                    } else {
                        matches = false;
                    }
                }

                // Check text pattern (only if previous filters matched)
                if matches {
                    if let Some(ref finder) = text_finder {
                        if finder.find(line).is_none() {
                            matches = false;
                        }
                    }
                }

                if matches {
                    local_count += 1;
                    local_lines.push(line);
                }

                pos = line_end + 1;
            }

            (local_count, local_lines)
        })
        .collect();

    // Merge results
    let total_count: usize = results.iter().map(|(c, _)| c).sum();
    let lines: Vec<String> = results
        .into_iter()
        .flat_map(|(_, lines)| lines)
        .take(limit)
        .filter_map(|line| std::str::from_utf8(line).ok().map(|s| s.to_string()))
        .collect();

    Ok((total_count, lines))
}

/// Group by any column with count aggregation
pub fn group_by_count(
    path: &Path,
    column: GroupByColumn,
    status_filter: Option<StatusFilter>,
    text_pattern: Option<&[u8]>,
) -> Result<Vec<(String, u64)>, ParseError> {
    let file = File::open(path)?;
    let mmap = unsafe { Mmap::map(&file)? };
    let data = &mmap[..];

    let chunk_size = 4 * 1024 * 1024;
    let chunk_bounds = find_chunk_boundaries(data, chunk_size);

    let text_finder = text_pattern.map(memchr::memmem::Finder::new);

    // Each chunk builds a local HashMap
    let local_maps: Vec<HashMap<Vec<u8>, u64>> = chunk_bounds
        .par_windows(2)
        .map(|window| {
            let chunk = &data[window[0]..window[1]];
            let mut counts: HashMap<Vec<u8>, u64> = HashMap::new();
            let mut pos = 0;

            while pos < chunk.len() {
                let line_end = memchr(b'\n', &chunk[pos..])
                    .map(|i| pos + i)
                    .unwrap_or(chunk.len());
                let line = &chunk[pos..line_end];

                if let Some(offsets) = find_fields(line) {
                    let mut matches = true;

                    // Apply status filter
                    if let Some(ref filter) = status_filter {
                        if let Some(status) = extract_status(line, &offsets) {
                            if !filter.matches(status) {
                                matches = false;
                            }
                        } else {
                            matches = false;
                        }
                    }

                    // Apply text filter
                    if matches {
                        if let Some(ref finder) = text_finder {
                            if finder.find(line).is_none() {
                                matches = false;
                            }
                        }
                    }

                    if matches {
                        // Extract the grouping key
                        let key: &[u8] = match column {
                            GroupByColumn::Ip => extract_ip(line, &offsets),
                            GroupByColumn::Path => extract_path(line, &offsets),
                            GroupByColumn::Method => extract_method(line, &offsets),
                            GroupByColumn::Status => {
                                // For status, we use the raw 3 bytes
                                let start = offsets.status_start;
                                if start + 3 <= line.len() {
                                    &line[start..start + 3]
                                } else {
                                    b"???"
                                }
                            }
                            GroupByColumn::Referer => extract_referer(line, &offsets),
                            GroupByColumn::UserAgent => extract_user_agent(line, &offsets),
                        };

                        *counts.entry(key.to_vec()).or_insert(0) += 1;
                    }
                }

                pos = line_end + 1;
            }

            counts
        })
        .collect();

    // Merge local maps
    let mut global_counts: HashMap<Vec<u8>, u64> = HashMap::new();
    for local in local_maps {
        for (key, count) in local {
            *global_counts.entry(key).or_insert(0) += count;
        }
    }

    // Convert to sorted result
    let mut result: Vec<(String, u64)> = global_counts
        .into_iter()
        .filter_map(|(key, count)| {
            std::str::from_utf8(&key).ok().map(|s| (s.to_string(), count))
        })
        .collect();

    result.sort_by(|a, b| b.1.cmp(&a.1));

    Ok(result)
}

/// Find chunk boundaries at newlines for parallel processing
fn find_chunk_boundaries(data: &[u8], chunk_size: usize) -> Vec<usize> {
    let mut boundaries = vec![0];
    let mut pos = 0;

    while pos < data.len() {
        pos += chunk_size;
        if pos < data.len() {
            // Find next newline after chunk boundary
            if let Some(nl) = memchr(b'\n', &data[pos..]) {
                boundaries.push(pos + nl + 1);
                pos += nl + 1;
            } else {
                break;
            }
        }
    }
    boundaries.push(data.len());
    boundaries
}

// ============================================================================
// MULTI-FILE SUPPORT
// ============================================================================

/// Count across multiple files (glob pattern support)
pub fn count_status_multi(paths: &[&Path], filter: StatusFilter) -> Result<usize, ParseError> {
    let counts: Result<Vec<usize>, ParseError> = paths
        .par_iter()
        .map(|path| count_status(path, filter))
        .collect();

    Ok(counts?.into_iter().sum())
}

/// Group by across multiple files
pub fn group_by_count_multi(
    paths: &[&Path],
    column: GroupByColumn,
    status_filter: Option<StatusFilter>,
    text_pattern: Option<&[u8]>,
) -> Result<Vec<(String, u64)>, ParseError> {
    let local_results: Result<Vec<Vec<(String, u64)>>, ParseError> = paths
        .par_iter()
        .map(|path| group_by_count(path, column, status_filter, text_pattern))
        .collect();

    // Merge all results
    let mut global_counts: HashMap<String, u64> = HashMap::new();
    for results in local_results? {
        for (key, count) in results {
            *global_counts.entry(key).or_insert(0) += count;
        }
    }

    let mut result: Vec<(String, u64)> = global_counts.into_iter().collect();
    result.sort_by(|a, b| b.1.cmp(&a.1));

    Ok(result)
}

// ============================================================================
// REGEX SEARCH (uses regex crate with SIMD optimizations)
// ============================================================================

/// Search for regex pattern in log lines - SIMD accelerated via regex crate
pub fn regex_search(
    path: &Path,
    pattern: &str,
    status_filter: Option<StatusFilter>,
    limit: usize,
) -> Result<(usize, Vec<String>), ParseError> {
    let file = File::open(path)?;
    let mmap = unsafe { Mmap::map(&file)? };
    let data = &mmap[..];

    let regex = Regex::new(pattern)
        .map_err(|e| ParseError::ParseFailed(format!("Invalid regex: {}", e)))?;

    let chunk_size = 4 * 1024 * 1024;
    let chunk_bounds = find_chunk_boundaries(data, chunk_size);

    let results: Vec<(usize, Vec<&[u8]>)> = chunk_bounds
        .par_windows(2)
        .map(|window| {
            let chunk = &data[window[0]..window[1]];
            let mut local_count = 0;
            let mut local_lines: Vec<&[u8]> = Vec::new();
            let mut pos = 0;

            while pos < chunk.len() {
                let line_end = memchr(b'\n', &chunk[pos..])
                    .map(|i| pos + i)
                    .unwrap_or(chunk.len());
                let line = &chunk[pos..line_end];

                let mut matches = regex.is_match(line);

                // Apply status filter if provided
                if matches {
                    if let Some(ref filter) = status_filter {
                        if let Some(offsets) = find_fields(line) {
                            if let Some(status) = extract_status(line, &offsets) {
                                if !filter.matches(status) {
                                    matches = false;
                                }
                            } else {
                                matches = false;
                            }
                        } else {
                            matches = false;
                        }
                    }
                }

                if matches {
                    local_count += 1;
                    local_lines.push(line);
                }

                pos = line_end + 1;
            }

            (local_count, local_lines)
        })
        .collect();

    let total_count: usize = results.iter().map(|(c, _)| c).sum();
    let lines: Vec<String> = results
        .into_iter()
        .flat_map(|(_, lines)| lines)
        .take(limit)
        .filter_map(|line| std::str::from_utf8(line).ok().map(|s| s.to_string()))
        .collect();

    Ok((total_count, lines))
}

// ============================================================================
// NUMERIC AGGREGATIONS (sum, avg, min, max on size field)
// ============================================================================

/// Aggregation operation types
#[derive(Debug, Clone, Copy)]
pub enum AggOp {
    Sum,
    Avg,
    Min,
    Max,
}

/// Result of numeric aggregation
#[derive(Debug, Clone)]
pub struct AggResult {
    pub sum: i64,
    pub count: u64,
    pub min: i64,
    pub max: i64,
}

impl AggResult {
    fn new() -> Self {
        Self {
            sum: 0,
            count: 0,
            min: i64::MAX,
            max: i64::MIN,
        }
    }

    fn merge(&mut self, other: &AggResult) {
        self.sum += other.sum;
        self.count += other.count;
        self.min = self.min.min(other.min);
        self.max = self.max.max(other.max);
    }

    pub fn avg(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.sum as f64 / self.count as f64
        }
    }
}

/// Aggregate size field with optional grouping
pub fn aggregate_size(
    path: &Path,
    group_by: Option<GroupByColumn>,
    status_filter: Option<StatusFilter>,
    text_pattern: Option<&[u8]>,
) -> Result<HashMap<String, AggResult>, ParseError> {
    let file = File::open(path)?;
    let mmap = unsafe { Mmap::map(&file)? };
    let data = &mmap[..];

    let chunk_size = 4 * 1024 * 1024;
    let chunk_bounds = find_chunk_boundaries(data, chunk_size);
    let text_finder = text_pattern.map(memchr::memmem::Finder::new);

    let local_results: Vec<HashMap<Vec<u8>, AggResult>> = chunk_bounds
        .par_windows(2)
        .map(|window| {
            let chunk = &data[window[0]..window[1]];
            let mut aggs: HashMap<Vec<u8>, AggResult> = HashMap::new();
            let mut pos = 0;

            while pos < chunk.len() {
                let line_end = memchr(b'\n', &chunk[pos..])
                    .map(|i| pos + i)
                    .unwrap_or(chunk.len());
                let line = &chunk[pos..line_end];

                if let Some(offsets) = find_fields(line) {
                    let mut matches = true;

                    // Apply status filter
                    if let Some(ref filter) = status_filter {
                        if let Some(status) = extract_status(line, &offsets) {
                            if !filter.matches(status) {
                                matches = false;
                            }
                        } else {
                            matches = false;
                        }
                    }

                    // Apply text filter
                    if matches {
                        if let Some(ref finder) = text_finder {
                            if finder.find(line).is_none() {
                                matches = false;
                            }
                        }
                    }

                    if matches {
                        if let Some(size) = extract_size(line, &offsets) {
                            let key = match group_by {
                                Some(GroupByColumn::Ip) => extract_ip(line, &offsets).to_vec(),
                                Some(GroupByColumn::Path) => extract_path(line, &offsets).to_vec(),
                                Some(GroupByColumn::Method) => extract_method(line, &offsets).to_vec(),
                                Some(GroupByColumn::Status) => {
                                    let start = offsets.status_start;
                                    if start + 3 <= line.len() {
                                        line[start..start + 3].to_vec()
                                    } else {
                                        b"???".to_vec()
                                    }
                                }
                                Some(GroupByColumn::Referer) => extract_referer(line, &offsets).to_vec(),
                                Some(GroupByColumn::UserAgent) => extract_user_agent(line, &offsets).to_vec(),
                                None => b"_total".to_vec(),
                            };

                            let agg = aggs.entry(key).or_insert_with(AggResult::new);
                            agg.sum += size;
                            agg.count += 1;
                            agg.min = agg.min.min(size);
                            agg.max = agg.max.max(size);
                        }
                    }
                }

                pos = line_end + 1;
            }

            aggs
        })
        .collect();

    // Merge results
    let mut global_aggs: HashMap<String, AggResult> = HashMap::new();
    for local in local_results {
        for (key, agg) in local {
            let key_str = std::str::from_utf8(&key).unwrap_or("???").to_string();
            global_aggs
                .entry(key_str)
                .or_insert_with(AggResult::new)
                .merge(&agg);
        }
    }

    Ok(global_aggs)
}

/// Aggregate size across multiple files
pub fn aggregate_size_multi(
    paths: &[&Path],
    group_by: Option<GroupByColumn>,
    status_filter: Option<StatusFilter>,
    text_pattern: Option<&[u8]>,
) -> Result<HashMap<String, AggResult>, ParseError> {
    let local_results: Result<Vec<HashMap<String, AggResult>>, ParseError> = paths
        .par_iter()
        .map(|path| aggregate_size(path, group_by, status_filter, text_pattern))
        .collect();

    let mut global_aggs: HashMap<String, AggResult> = HashMap::new();
    for local in local_results? {
        for (key, agg) in local {
            global_aggs
                .entry(key)
                .or_insert_with(AggResult::new)
                .merge(&agg);
        }
    }

    Ok(global_aggs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_fields() {
        let line = b"192.168.1.1 - - [10/Oct/2024:13:55:36 +0000] \"GET /index.html HTTP/1.1\" 200 2326";
        let offsets = find_fields(line).expect("Should parse");

        assert_eq!(&line[..offsets.ip_end], b"192.168.1.1");
        assert_eq!(
            &line[offsets.request_start..offsets.request_end],
            b"GET /index.html HTTP/1.1"
        );

        let status = extract_status(line, &offsets);
        assert_eq!(status, Some(200));

        let size = extract_size(line, &offsets);
        assert_eq!(size, Some(2326));
    }

    #[test]
    fn test_extract_method_path() {
        let line = b"192.168.1.1 - - [10/Oct/2024:13:55:36 +0000] \"POST /api/users HTTP/1.1\" 201 100";
        let offsets = find_fields(line).expect("Should parse");

        assert_eq!(extract_method(line, &offsets), b"POST");
        assert_eq!(extract_path(line, &offsets), b"/api/users");
    }

    #[test]
    fn test_status_filter_parse() {
        assert!(matches!(
            StatusFilter::parse(">=400"),
            Some(StatusFilter::GreaterOrEqual(400))
        ));
        assert!(matches!(
            StatusFilter::parse("=200"),
            Some(StatusFilter::Equal(200))
        ));
        assert!(matches!(
            StatusFilter::parse("4xx"),
            Some(StatusFilter::Range(400, 499))
        ));
        assert!(matches!(
            StatusFilter::parse("500"),
            Some(StatusFilter::Equal(500))
        ));
    }

    #[test]
    fn test_no_false_positives() {
        // Valid line with 404 in URL path but status 200
        let line = b"192.168.1.1 - - [10/Oct/2024:13:55:36 +0000] \"GET /error/404/page HTTP/1.1\" 200 100";
        let offsets = find_fields(line).expect("Should parse");
        let status = extract_status(line, &offsets);
        assert_eq!(status, Some(200)); // Should be 200, not 404
    }

    #[test]
    fn test_extract_size() {
        let line = b"192.168.1.1 - - [10/Oct/2024:13:55:36 +0000] \"GET /index.html HTTP/1.1\" 200 12345";
        let offsets = find_fields(line).expect("Should parse");
        let size = extract_size(line, &offsets);
        assert_eq!(size, Some(12345));
    }
}
