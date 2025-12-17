# Forensic Log MCP Benchmark Suite

Benchmark suite comparing forensic-log-mcp against traditional log analysis tools.

## Quick Start

```bash
# 1. Generate test data
cd generate_logs
cargo build --release
./target/release/generate_logs -o ../data/apache_1M.log -l 1000000 -f apache

# 2. Build MCP server
cd ../../mcp
cargo build --release

# 3. Run benchmarks
cd ../benchmark
./run_benchmark.sh
```

## Directory Structure

```
benchmark/
├── BENCHMARK.md          # Detailed benchmark results
├── run_benchmark.sh      # Main benchmark runner
├── data/                 # Test data (generated, gitignored)
│   └── .gitkeep
├── generate_logs/        # Rust log generator
│   └── src/main.rs
└── scripts/
    ├── mcp_client.py     # MCP protocol client
    └── stats.py          # Statistical analysis
```

## Tools Compared

| Tool | Description |
|------|-------------|
| **forensic-log-mcp** | SIMD-accelerated MCP server with Polars |
| **grep/ripgrep** | Pattern matching |
| **awk** | Text processing |
| **jq** | JSON processing |

## Benchmark Configuration

- **Runs per test**: 10
- **Warmup runs**: 3
- **Statistics**: Mean, median, stddev, outlier detection

## Log Generator

Generate realistic test data:

```bash
cd generate_logs
cargo build --release

# Apache logs (125MB per million lines)
./target/release/generate_logs -o ../data/apache_1M.log -l 1000000 -f apache -e 0.05

# JSON logs (180MB per million lines)
./target/release/generate_logs -o ../data/json_1M.log -l 1000000 -f json -e 0.05

# Syslog (75MB per million lines)
./target/release/generate_logs -o ../data/syslog_1M.log -l 1000000 -f syslog -e 0.05
```

Options:
- `-o, --output` - Output file path
- `-l, --lines` - Number of lines (default: 1000000)
- `-f, --format` - Log format: apache, json, syslog
- `-e, --error_rate` - Error rate 0.0-1.0 (default: 0.05)

## Requirements

**Required:**
- Rust toolchain (1.75+)
- grep, awk

**Optional (for full comparison):**
- ripgrep (`rg`)
- jq

## Results Summary

See [BENCHMARK.md](BENCHMARK.md) for detailed results.

**MCP excels at:**
- GROUP BY aggregations (5-50x faster than awk)
- JSON log analysis (11x faster than jq)
- Complex multi-column queries

**grep excels at:**
- Simple line counting (~24x faster than MCP)
