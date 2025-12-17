//! SIMD-accelerated Syslog parser
//!
//! Syslog format: <priority>timestamp hostname process[pid]: message
//! Example: <134>Dec 17 10:30:45 server01 sshd[12345]: Accepted password for user

use memchr::memchr;
use memmap2::Mmap;
use rayon::prelude::*;
use regex::bytes::Regex;
use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

use super::ParseError;

/// Field offsets for syslog lines
#[derive(Debug, Clone, Copy)]
struct SyslogOffsets {
    hostname_start: usize,
    hostname_end: usize,
    process_start: usize,
    process_end: usize,
    message_start: usize,
}

/// Find field boundaries in syslog line
#[inline]
fn find_syslog_fields(line: &[u8]) -> Option<SyslogOffsets> {
    if line.is_empty() {
        return None;
    }

    // Skip priority if present (e.g., <134>)
    let start = if line[0] == b'<' {
        memchr(b'>', line).map(|i| i + 1).unwrap_or(0)
    } else {
        0
    };

    // Skip timestamp (typically "Mon DD HH:MM:SS" or ISO format)
    // Find first space after timestamp - usually after 3 spaces for BSD format
    let mut space_count = 0;
    let mut hostname_start = start;
    for (i, &b) in line[start..].iter().enumerate() {
        if b == b' ' {
            space_count += 1;
            if space_count == 3 {
                hostname_start = start + i + 1;
                break;
            }
        }
    }

    if hostname_start >= line.len() {
        return None;
    }

    // Hostname ends at next space
    let hostname_end = memchr(b' ', &line[hostname_start..])
        .map(|i| hostname_start + i)
        .unwrap_or(line.len());

    if hostname_end >= line.len() {
        return None;
    }

    // Process starts after hostname space
    let process_start = hostname_end + 1;

    // Process ends at '[' (pid) or ':' (no pid)
    let process_end = line[process_start..]
        .iter()
        .position(|&b| b == b'[' || b == b':')
        .map(|i| process_start + i)
        .unwrap_or(line.len());

    // Message starts after ": "
    let colon_pos = memchr(b':', &line[process_start..])
        .map(|i| process_start + i)
        .unwrap_or(line.len());
    let message_start = if colon_pos + 2 < line.len() {
        colon_pos + 2 // Skip ": "
    } else {
        line.len()
    };

    Some(SyslogOffsets {
        hostname_start,
        hostname_end,
        process_start,
        process_end,
        message_start,
    })
}

#[inline]
fn extract_hostname<'a>(line: &'a [u8], offsets: &SyslogOffsets) -> &'a [u8] {
    &line[offsets.hostname_start..offsets.hostname_end]
}

#[inline]
fn extract_process<'a>(line: &'a [u8], offsets: &SyslogOffsets) -> &'a [u8] {
    &line[offsets.process_start..offsets.process_end]
}

#[inline]
fn extract_message<'a>(line: &'a [u8], offsets: &SyslogOffsets) -> &'a [u8] {
    if offsets.message_start < line.len() {
        &line[offsets.message_start..]
    } else {
        &[]
    }
}

/// Column to group by for syslog
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyslogGroupBy {
    Hostname,
    Process,
}

impl SyslogGroupBy {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "hostname" | "host" | "server" => Some(SyslogGroupBy::Hostname),
            "process" | "program" | "service" | "app" => Some(SyslogGroupBy::Process),
            _ => None,
        }
    }
}

/// Find chunk boundaries at newlines
fn find_chunk_boundaries(data: &[u8], chunk_size: usize) -> Vec<usize> {
    let mut boundaries = vec![0];
    let mut pos = 0;

    while pos < data.len() {
        pos += chunk_size;
        if pos < data.len() {
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

/// Filter syslog lines by text pattern
pub fn filter_lines(
    path: &Path,
    text_pattern: Option<&[u8]>,
    limit: usize,
) -> Result<(usize, Vec<String>), ParseError> {
    let file = File::open(path)?;
    let mmap = unsafe { Mmap::map(&file)? };
    let data = &mmap[..];

    let chunk_size = 4 * 1024 * 1024;
    let chunk_bounds = find_chunk_boundaries(data, chunk_size);
    let text_finder = text_pattern.map(memchr::memmem::Finder::new);

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

                let matches = match &text_finder {
                    Some(finder) => finder.find(line).is_some(),
                    None => true,
                };

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

/// Regex search in syslog
pub fn regex_search(
    path: &Path,
    pattern: &str,
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

                if regex.is_match(line) {
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

/// Group by hostname or process with count
pub fn group_by_count(
    path: &Path,
    column: SyslogGroupBy,
    text_pattern: Option<&[u8]>,
) -> Result<Vec<(String, u64)>, ParseError> {
    let file = File::open(path)?;
    let mmap = unsafe { Mmap::map(&file)? };
    let data = &mmap[..];

    let chunk_size = 4 * 1024 * 1024;
    let chunk_bounds = find_chunk_boundaries(data, chunk_size);
    let text_finder = text_pattern.map(memchr::memmem::Finder::new);

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

                let matches = match &text_finder {
                    Some(finder) => finder.find(line).is_some(),
                    None => true,
                };

                if matches {
                    if let Some(offsets) = find_syslog_fields(line) {
                        let key = match column {
                            SyslogGroupBy::Hostname => extract_hostname(line, &offsets),
                            SyslogGroupBy::Process => extract_process(line, &offsets),
                        };
                        *counts.entry(key.to_vec()).or_insert(0) += 1;
                    }
                }

                pos = line_end + 1;
            }

            counts
        })
        .collect();

    // Merge
    let mut global_counts: HashMap<Vec<u8>, u64> = HashMap::new();
    for local in local_maps {
        for (key, count) in local {
            *global_counts.entry(key).or_insert(0) += count;
        }
    }

    let mut result: Vec<(String, u64)> = global_counts
        .into_iter()
        .filter_map(|(key, count)| {
            std::str::from_utf8(&key).ok().map(|s| (s.to_string(), count))
        })
        .collect();

    result.sort_by(|a, b| b.1.cmp(&a.1));

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_syslog_fields() {
        let line = b"Dec 17 10:30:45 server01 sshd[12345]: Accepted password for user";
        let offsets = find_syslog_fields(line).expect("Should parse");

        assert_eq!(extract_hostname(line, &offsets), b"server01");
        assert_eq!(extract_process(line, &offsets), b"sshd");
    }

    #[test]
    fn test_syslog_with_priority() {
        let line = b"<134>Dec 17 10:30:45 server01 nginx: GET /index.html";
        let offsets = find_syslog_fields(line).expect("Should parse");

        assert_eq!(extract_hostname(line, &offsets), b"server01");
        assert_eq!(extract_process(line, &offsets), b"nginx");
    }
}
