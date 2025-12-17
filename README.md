# Forensic Log MCP Server

A high-performance [Model Context Protocol](https://modelcontextprotocol.io/) (MCP) server for log analysis, powered by [Polars](https://pola.rs/) and custom SIMD-accelerated parsers.

Give Claude the ability to analyze massive log files (gigabytes of data) that would never fit in its context window.

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org/)

## Features

- **Fast Aggregations**: 5-50x faster than awk on GROUP BY operations
- **SIMD-Accelerated**: Custom parsers using `memchr` for structured queries
- **Multi-Format**: Apache, Nginx, Syslog, JSON Lines, CSV/TSV
- **Streaming**: Handles files larger than RAM via lazy evaluation
- **AI-Native**: Designed for Claude integration via MCP protocol

## Performance

MCP excels at **aggregation queries** on large files (1M lines, 125MB Apache log):

| Operation | awk | MCP | Speedup |
|-----------|-----|-----|---------|
| Group by IP | 0.26s | 0.048s | **5x faster** |
| Group by method | 1.31s | 0.047s | **28x faster** |
| Group by user_agent | 2.40s | 0.049s | **50x faster** |
| Group by referer | 1.14s | 0.046s | **25x faster** |
| Sum size | 0.37s | 0.047s | **8x faster** |
| JSON group by (vs jq) | 1.65s | 0.16s | **11x faster** |

**Honest Assessment**:
- MCP dominates GROUP BY/aggregation queries (5-50x faster)
- grep is ~24x faster for simple line counting (`grep -c`)
- MCP has ~57ms Python/MCP overhead, negligible on large files

See [benchmark/BENCHMARK.md](benchmark/BENCHMARK.md) for detailed methodology and honest comparison.

## Quick Start

### Installation

```bash
# Clone the repository
git clone https://github.com/TLinvest/forensic-log-mcp.git
cd forensic-log-mcp

# Build the MCP server
cd mcp
cargo build --release
```

The binary will be at `mcp/target/release/forensic-log-mcp`.

### Configuration

Add to your Claude Code project's `.mcp.json`:

```json
{
  "mcpServers": {
    "forensic-logs": {
      "type": "stdio",
      "command": "/path/to/forensic-log-mcp",
      "args": []
    }
  }
}
```

Or add globally to `~/.claude.json`.

## Usage Examples

Once configured, Claude can analyze your logs directly:

```
"Find all 500 errors in my nginx access log"
→ Uses analyze_logs with filter_status="500"

"Count requests by IP address"
→ Uses aggregate_logs with group_by="ip"

"Search for timeout errors in the last hour"
→ Uses search_pattern with regex matching

"Show me the error rate by hour"
→ Uses time_analysis with bucket="hour"
```

## Available Tools

| Tool | Description |
|------|-------------|
| `get_log_schema` | Discover columns and sample data from a log file |
| `analyze_logs` | Filter, group, and sort log data |
| `aggregate_logs` | Perform statistical aggregations (count, sum, avg, min, max) |
| `search_pattern` | Search for regex patterns |
| `time_analysis` | Analyze logs over time with bucketing |

## Supported Formats

| Format | Auto-Detected | SIMD Fast Path |
|--------|---------------|----------------|
| Apache/Nginx Combined | Yes | Yes |
| Syslog (RFC 3164/5424) | Yes | Yes |
| JSON Lines (NDJSON) | Yes | Polars-native |
| CSV/TSV | Yes | Polars-native |

## Project Structure

```
forensic-log-mcp/
├── mcp/                    # MCP server source code
│   ├── src/
│   │   ├── main.rs        # Entry point
│   │   ├── tools/         # MCP tool implementations
│   │   ├── parsers/       # Log format parsers
│   │   │   ├── apache_simd.rs   # SIMD Apache parser
│   │   │   ├── syslog_simd.rs   # SIMD Syslog parser
│   │   │   └── ...
│   │   └── engine/        # Query engine
│   ├── Cargo.toml
│   └── README.md          # Detailed MCP documentation
├── benchmark/             # Performance benchmarks
│   ├── BENCHMARK.md       # Detailed results
│   ├── run_benchmark.sh   # Benchmark runner
│   └── scripts/           # Benchmark utilities
├── LICENSE
├── CONTRIBUTING.md
└── README.md              # This file
```

## How It's So Fast

1. **SIMD-Accelerated Parsing**: Uses `memchr` for vectorized field detection
2. **Zero-Copy Processing**: Works directly with memory-mapped byte slices
3. **Parallel Chunk Processing**: Splits files into 4MB chunks processed via `rayon`
4. **Lazy Field Extraction**: Only parses fields needed for the query
5. **Predicate Pushdown**: Filters during scan, not after loading

## Requirements

- Rust 1.75+ (for building)
- Claude Code or any MCP-compatible client

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## License

MIT License - see [LICENSE](LICENSE) for details.

## Acknowledgments

- [Polars](https://pola.rs/) - Fast DataFrame library
- [rmcp](https://github.com/modelcontextprotocol/rust-sdk) - Rust MCP SDK
- [memchr](https://github.com/BurntSushi/memchr) - SIMD byte searching
