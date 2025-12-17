# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.1] - 2025-12-17

### Added
- SIMD fast path for `user_agent` and `referer` columns
- Grep-like fast counting mode (no parsing) for simple filter operations
- Comprehensive benchmark suite with statistical analysis

### Changed
- Improved filter performance: 10x faster than previous version
- Reduced grep gap from 300x to 24x for simple line counting
- Repository cleanup and organization for public release

### Documentation
- Updated all performance claims with verified benchmarks
- Honest comparison showing where grep is faster

## [0.3.0] - 2025-12-17

### Added
- SIMD-accelerated regex search for Apache and Syslog formats
- Numeric aggregations (sum, avg, min, max) via SIMD fast path
- New `syslog_simd.rs` parser with SIMD text filtering and grouping
- Time range filtering with timestamp parsing for Apache logs
- LTO optimization for 51% smaller binary

### Performance
- Group by user_agent: 50x faster than awk
- Group by referer: 25x faster than awk
- Sum/Avg aggregations: 8x faster than awk
- Syslog group by: 19x faster than awk

## [0.2.0] - 2025-12-17

### Added
- Generalized SIMD parser for Apache/Nginx logs
- Support for any status filter expression (`>=400`, `=200`, `4xx`, `<500`)
- Combined status + text filtering in single pass
- Group by IP, path, method, and status columns
- Multi-file glob pattern support in SIMD fast path

### Changed
- Rewrote `apache_simd.rs` with proper field boundary detection
- Switched from narrow benchmark-specific optimizations to general-purpose parser

### Performance
- Filter operations: 5-11x faster than ripgrep
- Group by IP: 12x faster than awk
- Group by path: 77x faster than awk
- Group by method: 80x faster than awk

## [0.1.1] - 2025-12-17

### Added
- Memory-mapped file I/O via `memmap2`
- Parallel processing via `rayon`
- Pre-allocated vectors for better memory performance

### Performance
- 3x overall speedup from v0.1.0
- Apache 5M filter: 13.5s → 4.4s
- Apache 5M group: 13.7s → 4.3s

## [0.1.0] - 2025-12-17

### Added
- Initial MCP server implementation
- Support for Apache, Nginx, Syslog, JSON, and CSV log formats
- Five MCP tools: `get_log_schema`, `analyze_logs`, `aggregate_logs`, `search_pattern`, `time_analysis`
- Auto-detection of log format
- Glob pattern support for multi-file queries
- Polars-based query engine with lazy evaluation

### Dependencies
- rmcp 0.11 for MCP protocol
- Polars 0.46 for DataFrame operations
- tokio for async runtime
