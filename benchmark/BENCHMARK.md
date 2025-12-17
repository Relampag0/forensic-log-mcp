# Forensic Log MCP Benchmark Results

## Test Environment

- **Date**: 2025-12-17
- **OS**: Linux 6.17.3-2-cachyos
- **CPU**: AMD Ryzen 7 8840U (16 threads)
- **RAM**: 14 GB
- **CPU Governor**: powersave
- **Methodology**: 10 runs per test, 3 warmup runs, mean reported

## Tool Versions

| Tool | Version |
|------|---------|
| forensic-log-mcp | 0.3.1 |
| rustc | 1.90.0 |
| grep | GNU 3.12 |
| ripgrep | 14.1.1 |
| awk | GNU 5.3.2 |
| jq | 1.8.1 |

---

## Executive Summary

### MCP Excels At

| Operation | vs Tool | Speedup | Notes |
|-----------|---------|---------|-------|
| Group by IP | awk | **5.4x faster** | SIMD-accelerated |
| Group by method | awk | **28x faster** | SIMD-accelerated |
| Group by user_agent | awk | **50x faster** | SIMD-accelerated |
| Group by referer | awk | **25x faster** | SIMD-accelerated |
| JSON aggregations | jq | **11x faster** | Polars native parser |
| Sum/Avg size | awk | **7.7x faster** | SIMD-accelerated |

### grep/awk Excel At

| Operation | vs MCP | Notes |
|-----------|--------|-------|
| Simple line counting | **~24x faster** | `grep -c` is highly optimized C code |
| Tiny files (<10K lines) | **~2x faster** | MCP has ~57ms Python/MCP overhead |

---

## MCP Protocol Overhead Analysis

MCP has a fixed startup overhead from process spawn, MCP handshake, and Polars initialization:

| File Size | Lines | Time (s) | Overhead % |
|-----------|-------|----------|------------|
| Tiny | 10 | 0.049 | 100% (baseline) |
| 10K | 10,000 | 0.058 | 84% |
| 100K | 100,000 | 0.133 | 37% |
| 1M | 1,000,000 | 0.900 | 5% |

**Conclusion**: MCP's ~50ms fixed overhead is significant for tiny files but negligible (<5%) for files over 100K lines.

---

## Detailed Results

### Apache Log Format (1M Lines, 125 MB)

#### Aggregation Benchmarks

| Operation | awk (s) | MCP (s) | Speedup |
|-----------|---------|---------|---------|
| Group by IP | 0.261 | 0.048 | **5.4x** |
| Group by method | 1.312 | 0.047 | **28x** |
| Group by user_agent | 2.401 | 0.049 | **50x** |
| Group by referer | 1.138 | 0.046 | **25x** |
| Sum size | 0.367 | 0.047 | **7.7x** |

##### Statistical Details (Group by IP)

| Metric | awk | MCP |
|--------|-----|-----|
| Mean | 0.261s | 0.048s |
| Median | 0.260s | 0.048s |
| Std Dev | 0.005s | 0.002s |
| CV% | 1.88% | 3.35% |
| Outliers | 0 | 0 |

#### Filter Benchmarks (Simple Line Counting)

| Operation | grep | MCP (grep-like) | Winner |
|-----------|------|-----------------|--------|
| Count 4xx/5xx errors | 0.004s | 0.086s | **grep (24x)** |

**Note**: MCP now uses a grep-like fast path for simple counting (no parsing).
The ~57ms Python/MCP overhead means grep is faster for simple operations.
MCP's value is in structured queries with GROUP BY.

### JSON Log Format (1M Lines, 182 MB)

| Operation | jq + sort | MCP | Speedup |
|-----------|-----------|-----|---------|
| Group by service | 1.65s | 0.155s | **11x** |

### Multi-File Glob (10 files x 10K lines)

| Scenario | awk (cat + pipe) | MCP (glob) | Notes |
|----------|------------------|------------|-------|
| Group by IP | 0.027s | 0.033s | MCP overhead dominates small files |

---

## Key Findings

### 1. MCP Dominates GROUP BY Operations

For aggregation queries on large files, MCP is 5-50x faster than awk:

- **Simple columns** (IP, method): 5-28x faster
- **Complex columns** (user_agent, referer): 25-50x faster due to awk's sort/uniq overhead
- **JSON logs**: 11x faster than jq

### 2. grep is Faster for Simple Counting

grep -c is ~24x faster than MCP for simple line counting due to:
- No Python/MCP protocol overhead (~57ms)
- Highly optimized C implementation
- Direct syscall access

However, MCP now uses a grep-like fast path (no parsing) that is **10x faster** than the previous implementation.

### 3. MCP Overhead is Amortized on Large Files

The ~50ms fixed overhead:
- **Dominates** on files < 10K lines
- **Significant** on files ~100K lines (37%)
- **Negligible** on files > 1M lines (<5%)

---

## When to Use Each Tool

| Use Case | Best Tool | Why |
|----------|-----------|-----|
| "How many errors?" | grep -c | Minimal overhead |
| "Top IPs by request count" | MCP | GROUP BY optimized |
| "Average response size by path" | MCP | Aggregation + grouping |
| "Find lines matching pattern" | grep/rg | Text search optimized |
| "JSON log analysis" | MCP | 11x faster than jq |
| "Time series analysis" | MCP | Built-in bucketing |
| "Quick file search" | grep | No startup cost |
| "Complex analytics" | MCP | Multiple operations in one query |

---

## Methodology Notes

### Fair Comparison Principles

1. **Equal Output**: All tools return equivalent data (top 50 results, sorted)
2. **Warm Cache**: 3 warmup runs before measurement
3. **Multiple Runs**: 10 timed runs per benchmark
4. **Statistical Reporting**: Mean, median, stddev, CV%, outlier detection

### Limitations

1. **Synthetic Data**: Test logs generated with 5% error rate
2. **Single Machine**: Results may vary on different hardware
3. **CPU Governor**: Tests run with `powersave` (not performance mode)
4. **Memory**: Peak RSS measurement via /proc sampling (may underestimate)

### Reproducibility

```bash
cd benchmark
./run_benchmark.sh
```
