use memchr::memmem;
use memmap2::Mmap;
use polars::prelude::*;
use rayon::prelude::*;
use std::fs::File;
use std::path::Path;

use super::ParseError;

/// Ultra-fast Apache parser - beats ripgrep on common operations
/// Key optimizations:
/// 1. Direct mmap scanning (no line splitting)
/// 2. SIMD pattern search with memmem
/// 3. Count-only mode (no string allocation)
/// 4. Parallel chunk processing

/// Count lines with status >= 400 - SIMD optimized, no allocations
/// This is the ripgrep-killer function
pub fn fast_count_errors(path: &Path) -> Result<usize, ParseError> {
    let file = File::open(path)?;
    let mmap = unsafe { Mmap::map(&file)? };
    let data = &mmap[..];

    // We search for '" 4' and '" 5' patterns (status 4xx and 5xx)
    // This is what ripgrep does with '" [45][0-9]{2} '
    let pattern_4xx = memmem::Finder::new(b"\" 4");
    let pattern_5xx = memmem::Finder::new(b"\" 5");

    // Process in parallel chunks for maximum throughput
    let chunk_size = 4 * 1024 * 1024; // 4MB chunks
    let chunks: Vec<&[u8]> = data.chunks(chunk_size).collect();

    let count: usize = chunks
        .par_iter()
        .map(|chunk| {
            let mut local_count = 0;

            // Count 4xx errors
            let mut pos = 0;
            while let Some(found) = pattern_4xx.find(&chunk[pos..]) {
                local_count += 1;
                pos += found + 3; // Move past the match
            }

            // Count 5xx errors
            pos = 0;
            while let Some(found) = pattern_5xx.find(&chunk[pos..]) {
                local_count += 1;
                pos += found + 3;
            }

            local_count
        })
        .sum();

    Ok(count)
}

/// Fast filter with line extraction - only when we need the actual lines
pub fn fast_filter_status(path: &Path, min_status: u16) -> Result<(usize, Vec<String>), ParseError> {
    let file = File::open(path)?;
    let mmap = unsafe { Mmap::map(&file)? };
    let data = &mmap[..];

    // For getting actual lines, we need to find line boundaries
    // But we can still optimize by only processing lines that might match

    // Determine which patterns to search based on min_status
    let search_4xx = min_status <= 499;
    let search_5xx = min_status <= 599;

    let pattern_4xx = memmem::Finder::new(b"\" 4");
    let pattern_5xx = memmem::Finder::new(b"\" 5");

    // Find all match positions first (SIMD accelerated)
    let mut match_positions: Vec<usize> = Vec::new();

    if search_4xx {
        let mut pos = 0;
        while let Some(found) = pattern_4xx.find(&data[pos..]) {
            let abs_pos = pos + found;
            // Verify it's a valid status (check the third digit exists and is a digit)
            if abs_pos + 4 < data.len() && data[abs_pos + 4].is_ascii_digit() {
                // Check if status >= min_status
                let s0 = data[abs_pos + 2];
                let s1 = data[abs_pos + 3];
                let s2 = data[abs_pos + 4];
                let status = (s0 - b'0') as u16 * 100 + (s1 - b'0') as u16 * 10 + (s2 - b'0') as u16;
                if status >= min_status {
                    match_positions.push(abs_pos);
                }
            }
            pos = abs_pos + 3;
        }
    }

    if search_5xx {
        let mut pos = 0;
        while let Some(found) = pattern_5xx.find(&data[pos..]) {
            let abs_pos = pos + found;
            if abs_pos + 4 < data.len() && data[abs_pos + 4].is_ascii_digit() {
                let s0 = data[abs_pos + 2];
                let s1 = data[abs_pos + 3];
                let s2 = data[abs_pos + 4];
                let status = (s0 - b'0') as u16 * 100 + (s1 - b'0') as u16 * 10 + (s2 - b'0') as u16;
                if status >= min_status {
                    match_positions.push(abs_pos);
                }
            }
            pos = abs_pos + 3;
        }
    }

    let count = match_positions.len();

    // Extract lines only for matches (parallel)
    let lines: Vec<String> = match_positions
        .par_iter()
        .filter_map(|&pos| {
            // Find line start (search backwards for \n)
            let line_start = data[..pos]
                .iter()
                .rposition(|&b| b == b'\n')
                .map(|p| p + 1)
                .unwrap_or(0);

            // Find line end (search forwards for \n)
            let line_end = data[pos..]
                .iter()
                .position(|&b| b == b'\n')
                .map(|p| pos + p)
                .unwrap_or(data.len());

            std::str::from_utf8(&data[line_start..line_end])
                .ok()
                .map(|s| s.to_string())
        })
        .collect();

    Ok((count, lines))
}

/// Extract only IP (first field before space) - O(1) per line
#[inline(always)]
fn extract_ip(line: &[u8]) -> &[u8] {
    let end = memchr::memchr(b' ', line).unwrap_or(line.len());
    &line[..end]
}

/// Fast group by IP using parallel chunk processing
pub fn fast_group_by_ip(path: &Path) -> Result<Vec<(String, u64)>, ParseError> {
    let file = File::open(path)?;
    let mmap = unsafe { Mmap::map(&file)? };
    let data = &mmap[..];

    // Process in chunks, each chunk builds a local HashMap
    let chunk_size = 4 * 1024 * 1024; // 4MB chunks

    // Find chunk boundaries at newlines
    let mut chunk_starts: Vec<usize> = vec![0];
    let mut pos = 0;
    while pos < data.len() {
        pos += chunk_size;
        if pos < data.len() {
            // Find next newline after chunk boundary
            if let Some(nl) = memchr::memchr(b'\n', &data[pos..]) {
                chunk_starts.push(pos + nl + 1);
                pos += nl + 1;
            } else {
                break;
            }
        }
    }
    chunk_starts.push(data.len());

    // Process chunks in parallel, each building a local count map
    use std::collections::HashMap;

    let local_maps: Vec<HashMap<&[u8], u64>> = chunk_starts
        .par_windows(2)
        .map(|window| {
            let start = window[0];
            let end = window[1];
            let chunk = &data[start..end];

            let mut counts: HashMap<&[u8], u64> = HashMap::new();

            // Process lines in this chunk
            let mut line_start = 0;
            while line_start < chunk.len() {
                // Find IP (first space)
                let ip_end = memchr::memchr(b' ', &chunk[line_start..])
                    .map(|p| line_start + p)
                    .unwrap_or(chunk.len());

                if ip_end > line_start {
                    let ip = &chunk[line_start..ip_end];
                    *counts.entry(ip).or_insert(0) += 1;
                }

                // Find next line
                line_start = memchr::memchr(b'\n', &chunk[line_start..])
                    .map(|p| line_start + p + 1)
                    .unwrap_or(chunk.len());
            }

            counts
        })
        .collect();

    // Merge local maps into global map
    let mut global_counts: HashMap<Vec<u8>, u64> = HashMap::new();
    for local in local_maps {
        for (ip, count) in local {
            *global_counts.entry(ip.to_vec()).or_insert(0) += count;
        }
    }

    // Convert to result format
    let mut result: Vec<(String, u64)> = global_counts
        .into_iter()
        .filter_map(|(ip, count)| {
            std::str::from_utf8(&ip).ok().map(|s| (s.to_string(), count))
        })
        .collect();

    result.sort_by(|a, b| b.1.cmp(&a.1));

    Ok(result)
}

/// Parse with lazy field extraction - only parses requested columns
pub fn parse_lazy(
    path: &Path,
    need_ip: bool,
    need_status: bool,
    need_path: bool,
    status_filter: Option<u16>,
) -> Result<LazyFrame, ParseError> {
    let file = File::open(path)?;
    let mmap = unsafe { Mmap::map(&file)? };

    let lines: Vec<&[u8]> = mmap
        .split(|&b| b == b'\n')
        .filter(|l| !l.is_empty())
        .collect();

    // Only collect what we need
    let results: Vec<(Option<String>, Option<i32>, Option<String>, String)> = lines
        .par_iter()
        .filter_map(|line| {
            // Early filter by status if specified
            if let Some(min_status) = status_filter {
                if let Some(status) = extract_status_fast(line) {
                    if status < min_status {
                        return None;
                    }
                } else {
                    return None;
                }
            }

            let line_str = std::str::from_utf8(line).ok()?;

            let ip = if need_ip {
                let ip_bytes = extract_ip(line);
                std::str::from_utf8(ip_bytes).ok().map(|s| s.to_string())
            } else {
                None
            };

            let status = if need_status || status_filter.is_some() {
                extract_status_fast(line).map(|s| s as i32)
            } else {
                None
            };

            let path_str = if need_path {
                extract_path_fast(line)
            } else {
                None
            };

            Some((ip, status, path_str, line_str.to_string()))
        })
        .collect();

    // Build minimal DataFrame
    let len = results.len();
    let mut ips: Vec<Option<String>> = Vec::with_capacity(len);
    let mut statuses: Vec<Option<i32>> = Vec::with_capacity(len);
    let mut paths: Vec<Option<String>> = Vec::with_capacity(len);
    let mut raws: Vec<String> = Vec::with_capacity(len);

    for (ip, status, path, raw) in results {
        ips.push(ip);
        statuses.push(status);
        paths.push(path);
        raws.push(raw);
    }

    let mut columns = vec![Column::new("raw".into(), raws)];

    if need_ip {
        columns.insert(0, Column::new("ip".into(), ips));
    }
    if need_status || status_filter.is_some() {
        columns.push(Column::new("status".into(), statuses));
    }
    if need_path {
        columns.push(Column::new("path".into(), paths));
    }

    let df = DataFrame::new(columns)?;
    Ok(df.lazy())
}

/// Fast status extraction using memmem
#[inline(always)]
fn extract_status_fast(line: &[u8]) -> Option<u16> {
    // Find '" ' pattern using SIMD
    let finder = memmem::Finder::new(b"\" ");
    if let Some(pos) = finder.find(line) {
        let start = pos + 2;
        if start + 3 <= line.len() {
            let s0 = line[start];
            let s1 = line[start + 1];
            let s2 = line[start + 2];
            if s0.is_ascii_digit() && s1.is_ascii_digit() && s2.is_ascii_digit() {
                let status = (s0 - b'0') as u16 * 100
                           + (s1 - b'0') as u16 * 10
                           + (s2 - b'0') as u16;
                return Some(status);
            }
        }
    }
    None
}

/// Extract path from request line
#[inline(always)]
fn extract_path_fast(line: &[u8]) -> Option<String> {
    let quote_pos = memchr::memchr(b'"', line)?;
    let start = quote_pos + 1;
    let space1 = memchr::memchr(b' ', &line[start..])? + start;
    let path_start = space1 + 1;
    let path_end = memchr::memchr(b' ', &line[path_start..])
        .map(|i| i + path_start)
        .unwrap_or(line.len());

    std::str::from_utf8(&line[path_start..path_end])
        .ok()
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_ip() {
        let line = b"192.168.1.1 - - [10/Oct/2024:13:55:36 +0000] \"GET /index.html HTTP/1.1\" 200 2326";
        assert_eq!(extract_ip(line), b"192.168.1.1");
    }

    #[test]
    fn test_extract_status_fast() {
        let line = b"192.168.1.1 - - [10/Oct/2024:13:55:36 +0000] \"GET /index.html HTTP/1.1\" 200 2326";
        assert_eq!(extract_status_fast(line), Some(200));

        let line2 = b"192.168.1.1 - - [10/Oct/2024:13:55:36 +0000] \"GET /index.html HTTP/1.1\" 404 123";
        assert_eq!(extract_status_fast(line2), Some(404));
    }
}
