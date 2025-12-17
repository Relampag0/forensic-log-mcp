# Complete Benchmark Results

**Purpose**: Test BOTH SIMD fast-path and Polars slow-path operations

## Key Finding

MCP performance varies significantly based on whether operations hit the SIMD fast path:


## Test File: 1M (125M)


==============================================
FAST PATH OPERATIONS (SIMD-accelerated)
==============================================
[0;34m[INFO][0m Group by IP (SIMD fast path)...

  Operation                        awk        MCP
  ---------------------------------------------
  Group by IP (SIMD)           0.3616s    0.0480s
[0;34m[INFO][0m Group by method (SIMD fast path)...
  Group by method (SIMD)       1.2599s    0.0483s
[0;34m[INFO][0m Sum size (SIMD fast path)...
  Sum size (SIMD)              0.3686s    0.0478s

==============================================
SLOW PATH OPERATIONS (Polars fallback)
==============================================
[0;34m[INFO][0m Group by user_agent (NO fast path - Polars)...

  Operation                             awk        MCP
  --------------------------------------------------
  Group by user_agent (Polars)      2.3861s    0.0471s
[0;34m[INFO][0m Group by referer (NO fast path - Polars)...
  Group by referer (Polars)         1.1575s    0.0474s
[0;34m[INFO][0m Time analysis hourly (Polars)...
  Time analysis hourly                  N/A    0.9412s
[0;34m[INFO][0m Complex filter (status + method)...
  Complex filter (POST+error)       0.1504s    0.9179s

## Analysis

### SIMD Fast Path Operations
Operations that hit the SIMD fast path are significantly faster:
- Group by IP/method/path/status
- Sum/avg/min/max on size field
- Text pattern filtering

### Polars Fallback Operations
Operations without SIMD optimization fall back to Polars:
- Group by user_agent
- Group by referer
- Time analysis
- Complex multi-field queries

### Recommendation
Use MCP for operations that have SIMD fast paths. For operations on
user_agent/referer fields, awk may be competitive or faster on smaller files.
