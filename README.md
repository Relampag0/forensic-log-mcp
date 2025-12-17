# Forensic Log MCP Server

A high-performance [Model Context Protocol](https://modelcontextprotocol.io/) (MCP) server for log analysis, powered by [Polars](https://pola.rs/) and custom SIMD-accelerated parsers.

Give Claude the ability to analyze massive log files (gigabytes of data) that would never fit in its context window.

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org/)

## Features

- **Blazing Fast**: 3-80x faster than grep, awk, ripgrep, and jq on common operations
- **SIMD-Accelerated**: Custom parsers using `memchr` for near-native performance
- **Multi-Format**: Apache, Nginx, Syslog, JSON Lines, CSV/TSV
- **Streaming**: Handles files larger than RAM via lazy evaluation
- **AI-Native**: Designed for Claude integration via MCP protocol

## Performance

| Operation | File Size | MCP | Best Alternative | Speedup |
|-----------|-----------|-----|------------------|---------|
| Filter status >= 400 | 5M lines (620MB) | 0.048s | ripgrep: 0.24s | **5x** |
| Group by IP | 5M lines (620MB) | 0.068s | awk: 0.77s | **12x** |
| Regex search | 5M lines (620MB) | 0.114s | ripgrep: 0.41s | **3.6x** |
| Sum/Avg size | 5M lines (620MB) | 0.066s | awk: 1.27s | **19x** |
| JSON aggregation | 1M lines (181MB) | 0.131s | jq: 1.52s | **12x** |

See [benchmark/BENCHMARK.md](benchmark/BENCHMARK.md) for detailed methodology and results.

## Quick Start

### Installation

```bash
# Clone the repository
git clone https://github.com/YOUR_USERNAME/forensic-log-mcp.git
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
