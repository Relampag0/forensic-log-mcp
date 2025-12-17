# Forensic Log MCP Server

A high-performance Model Context Protocol (MCP) server for log analysis, powered by [Polars](https://pola.rs/). This gives Claude the ability to analyze massive log files (gigabytes of data) that would never fit in its context window.

## Features

- **Fast Aggregations**: 10-19x faster than awk/jq on GROUP BY operations
- **Streaming**: Handles files larger than RAM via lazy evaluation
- **Multi-Format**: Apache, Nginx, Syslog, JSON Lines, CSV/TSV
- **Auto-Detection**: Automatically detects log format from content
- **Glob Support**: Query multiple files with patterns like `/var/log/*.log`
- **Rich Queries**: Filter, aggregate, search, and analyze time series

## Performance

MCP excels at **aggregation queries** (GROUP BY, COUNT, SUM, AVG) on large files:

| Operation | awk | MCP | Speedup |
|-----------|-----|-----|---------|
| Group by IP | 0.36s | 0.048s | **7.5x faster** |
| Group by method | 1.26s | 0.048s | **26x faster** |
| Group by user_agent | 2.39s | 0.047s | **51x faster** |
| Group by referer | 1.16s | 0.047s | **24x faster** |
| Sum size | 0.37s | 0.048s | **7.7x faster** |

*(Tested on 1M line Apache log, 125MB)*

### Honest Assessment

**MCP is faster for:**
- All GROUP BY aggregations (7-51x faster than awk)
- JSON log analysis (10-15x faster than jq)
- Complex multi-column queries

**grep is faster for:**
- Simple text counting (`grep -c` is 6x faster due to MCP startup overhead)

### How It's So Fast

1. **SIMD-Accelerated Parsing**: Custom parsers using `memchr` for field detection
2. **Zero-Copy**: Works with memory-mapped byte slices directly
3. **Parallel Processing**: Splits files into 4MB chunks processed in parallel via `rayon`
4. **Lazy Evaluation**: Only extracts fields needed for the query
5. **Predicate Pushdown**: Filters during scan, not after loading

See [benchmark/BENCHMARK.md](../benchmark/BENCHMARK.md) for detailed methodology and results.

## Installation

### Build from Source

```bash
cd mcp
cargo build --release
```

The binary will be at `target/release/forensic-log-mcp`.

## Configuration

### Claude Code

Add to your project's `.mcp.json`:

```json
{
  "mcpServers": {
    "forensic-logs": {
      "type": "stdio",
      "command": "/path/to/forensic-log-mcp",
      "args": [],
      "env": {}
    }
  }
}
```

Or add globally to `~/.claude.json`.

## Available Tools

### `get_log_schema`

Discover available columns and sample data from a log file.

| Parameter | Type | Description |
|-----------|------|-------------|
| `path` | string | Path to log file (required) |
| `format` | string | Log format hint: auto, apache, nginx, syslog, json, csv |
| `sample_rows` | number | Number of sample rows (default: 5) |

### `analyze_logs`

Main tool for filtering, grouping, and sorting logs.

| Parameter | Type | Description |
|-----------|------|-------------|
| `path` | string | Path to log file, directory, or glob pattern (required) |
| `format` | string | Log format: auto, apache, nginx, syslog, json, csv |
| `filter_status` | string | Filter by status code: `>=400`, `500`, `4xx`, `5xx` |
| `filter_text` | string | Filter by text/regex pattern |
| `filter_time_start` | string | Start time filter |
| `filter_time_end` | string | End time filter |
| `group_by` | string | Column to group results by |
| `sort_by` | string | Column to sort results by |
| `sort_desc` | boolean | Sort descending (default: true) |
| `limit` | number | Max rows to return (default: 50) |

### `aggregate_logs`

Perform statistical aggregations on log data.

| Parameter | Type | Description |
|-----------|------|-------------|
| `path` | string | Path to log file(s) (required) |
| `operation` | string | count, sum, avg, min, max, unique (required) |
| `column` | string | Column to aggregate |
| `group_by` | string | Column to group by |
| `filter_text` | string | Filter by text pattern |
| `format` | string | Log format |
| `limit` | number | Max rows (default: 50) |

### `search_pattern`

Search for regex patterns in log files.

| Parameter | Type | Description |
|-----------|------|-------------|
| `path` | string | Path to log file(s) (required) |
| `pattern` | string | Regex pattern to search for (required) |
| `column` | string | Column to search (default: raw) |
| `case_sensitive` | boolean | Case sensitive search (default: false) |
| `format` | string | Log format |
| `limit` | number | Max rows (default: 50) |

### `time_analysis`

Analyze logs over time with bucketing.

| Parameter | Type | Description |
|-----------|------|-------------|
| `path` | string | Path to log file(s) (required) |
| `bucket` | string | Time bucket: minute, hour, day (required) |
| `time_column` | string | Time column to use |
| `count_column` | string | What to count per bucket |
| `filter_text` | string | Filter by text pattern |
| `format` | string | Log format |
| `limit` | number | Max buckets (default: 50) |

## Supported Log Formats

### Apache/Nginx Combined Log Format
```
192.168.1.1 - - [10/Dec/2024:10:15:32 +0000] "GET /index.html HTTP/1.1" 200 2326 "-" "Mozilla/5.0"
```

**Columns**: `ip`, `timestamp`, `request`, `method`, `path`, `status`, `size`, `referer`, `user_agent`, `raw`

### JSON Lines (NDJSON)
```json
{"timestamp":"2024-12-10T10:15:32Z","level":"ERROR","message":"Connection timeout"}
```

**Columns**: All JSON fields are automatically extracted

### Syslog (RFC 3164/5424)
```
Dec 10 10:15:32 myhost sshd[12345]: Accepted publickey for user
```

**Columns**: `priority`, `timestamp`, `hostname`, `process`, `pid`, `message`, `level`, `raw`

### CSV/TSV
Automatically detects headers and delimiter.

## Examples

### Find all 500 errors
```
analyze_logs(path="/var/log/nginx/access.log", filter_status="500")
```

### Count requests by IP
```
aggregate_logs(path="/var/log/nginx/*.log", operation="count", group_by="ip")
```

### Search for timeout errors
```
search_pattern(path="/var/log/app.json", pattern="timeout|connection refused", column="message")
```

### Analyze error rate by hour
```
time_analysis(path="/var/log/nginx/access.log", bucket="hour", filter_status=">=400")
```

### Query multiple log files
```
analyze_logs(path="/var/log/**/*.log", filter_text="error", limit=100)
```

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Claude / MCP Client                   │
└─────────────────────────────────────────────────────────┘
                           │ stdio
                           ▼
┌─────────────────────────────────────────────────────────┐
│                    MCP Server (rmcp)                     │
│  ┌─────────────────────────────────────────────────────┐│
│  │                   Tool Router                        ││
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌─────────┐ ││
│  │  │ analyze  │ │  schema  │ │aggregate │ │ search  │ ││
│  │  │  _logs   │ │  _logs   │ │  _logs   │ │_pattern │ ││
│  │  └──────────┘ └──────────┘ └──────────┘ └─────────┘ ││
│  └─────────────────────────────────────────────────────┘│
│  ┌─────────────────────────────────────────────────────┐│
│  │            SIMD Fast Path (apache_simd.rs)          ││
│  │  • memchr field detection  • Zero-copy extraction   ││
│  │  • Parallel chunk processing  • Status/time filter  ││
│  └─────────────────────────────────────────────────────┘│
│  ┌─────────────────────────────────────────────────────┐│
│  │               Log Parser Engine                      ││
│  │  • Apache/Nginx  • Syslog  • JSON  • CSV  • Auto    ││
│  └─────────────────────────────────────────────────────┘│
│  ┌─────────────────────────────────────────────────────┐│
│  │              Polars LazyFrame Engine                 ││
│  │  • Streaming reads  • Filters  • Aggregations       ││
│  └─────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────┘
```

## Dependencies

- [rmcp](https://github.com/modelcontextprotocol/rust-sdk) - Rust MCP SDK
- [Polars](https://pola.rs/) - Fast DataFrame library
- [Tokio](https://tokio.rs/) - Async runtime
- [memchr](https://github.com/BurntSushi/memchr) - SIMD-accelerated byte search
- [memmap2](https://github.com/RazrFalcon/memmap2-rs) - Memory-mapped file I/O
- [rayon](https://github.com/rayon-rs/rayon) - Parallel processing

## License

MIT
