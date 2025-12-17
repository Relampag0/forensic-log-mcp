# Forensic Log MCP Benchmark Suite

Benchmark suite to compare the forensic-log-mcp server against traditional log analysis tools.

## Tools Compared

| Tool | Description |
|------|-------------|
| **forensic-log-mcp** | Our Polars-powered MCP server |
| **grep/ripgrep** | Pattern matching |
| **awk** | Text processing |
| **jq** | JSON processing |
| **pandas** | Python DataFrame library |

## Benchmark Operations

1. **Filter Errors** - Find all log entries with status >= 400 (or ERROR level)
2. **Group Count** - Count entries grouped by IP/service/hostname
3. **Pattern Search** - Search for regex patterns in log content

## Directory Structure

```
benchmark/
├── README.md
├── run_benchmark.sh        # Main benchmark runner
├── data/                   # Generated test data
│   ├── apache_*.log
│   ├── json_*.log
│   └── syslog_*.log
├── results/                # Benchmark results
│   └── benchmark_report.md
├── generate_logs/          # Rust log generator
│   └── src/main.rs
└── scripts/                # Helper scripts
    ├── mcp_client.py       # MCP communication client
    ├── mcp_filter_errors.sh
    ├── mcp_group_count.sh
    ├── mcp_search.sh
    └── pandas_benchmark.py
```

## Usage

### Generate Test Data Only

```bash
./run_benchmark.sh --generate-only
```

### Run Small Benchmark (100k lines)

```bash
./run_benchmark.sh --small
```

### Run Full Benchmark Suite

```bash
./run_benchmark.sh
```

This will:
1. Generate test data (100k, 500k, 1M, 5M lines) in Apache, JSON, and Syslog formats
2. Run each benchmark operation with all tools
3. Generate a markdown report in `results/benchmark_report.md`

## Requirements

- **Required**: Rust toolchain (nightly for edition 2024), grep, awk
- **Optional**: ripgrep (`rg`), jq, python3 + pandas, hyperfine

Install optional tools:
```bash
# Arch/CachyOS
sudo pacman -S ripgrep jq python-pandas hyperfine

# Ubuntu/Debian
sudo apt install ripgrep jq python3-pandas
cargo install hyperfine
```

## Log Generator

The log generator creates realistic log data with configurable parameters:

```bash
cd generate_logs
cargo +nightly build --release

# Generate 1M Apache logs with 5% error rate
./target/release/generate_logs -o test.log -l 1000000 -f apache -e 0.05

# Options:
#   -o, --output      Output file path
#   -l, --lines       Number of lines (default: 1000000)
#   -f, --format      Log format: apache, json, syslog
#   -e, --error_rate  Error rate 0.0-1.0 (default: 0.05)
```

## Expected Results

The MCP server should excel at:
- **Complex queries** - Aggregations, grouping, multi-column filters
- **Large files** - Streaming via Polars handles files larger than RAM
- **Structured output** - JSON results ready for further processing

Traditional tools may be faster for:
- **Simple grep** - Single pattern matching on small files
- **One-liners** - Quick awk scripts for basic counting

## Sample Results

Run `./run_benchmark.sh --small` to see comparative benchmarks on your system.
