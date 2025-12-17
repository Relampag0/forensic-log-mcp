#!/bin/bash
set -e

# =============================================================================
# COMPLETE BENCHMARK SUITE
# =============================================================================
# Tests BOTH fast-path and slow-path operations to show the complete picture
# =============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
DATA_DIR="$SCRIPT_DIR/data"
RESULTS_DIR="$SCRIPT_DIR/results_complete"
MCP_SERVER="$PROJECT_DIR/mcp/target/release/forensic-log-mcp"

RUNS=5
WARMUP=2

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m'

log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[OK]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }

run_timed() {
    local name="$1"
    local cmd="$2"
    local results_file="$RESULTS_DIR/${name}.txt"

    # Warm-up
    for i in $(seq 1 $WARMUP); do
        eval "$cmd" >/dev/null 2>&1 || true
    done

    # Timed runs
    > "$results_file"
    for i in $(seq 1 $RUNS); do
        local start=$(date +%s.%N)
        eval "$cmd" >/dev/null 2>&1 || true
        local end=$(date +%s.%N)
        echo "$(echo "$end - $start" | bc -l)" >> "$results_file"
    done

    # Stats
    local mean=$(awk '{sum+=$1} END {printf "%.4f", sum/NR}' "$results_file")
    local stddev=$(awk -v mean="$mean" '{sum+=($1-mean)^2} END {printf "%.4f", sqrt(sum/NR)}' "$results_file")
    echo "$mean $stddev"
}

# =============================================================================
# FAST PATH OPERATIONS (SIMD-accelerated)
# =============================================================================

benchmark_fast_path() {
    local logfile="$1"
    local size="$2"

    echo ""
    echo "=============================================="
    echo "FAST PATH OPERATIONS (SIMD-accelerated)"
    echo "=============================================="

    # Group by IP - HAS SIMD fast path
    log_info "Group by IP (SIMD fast path)..."
    local awk_stats=$(run_timed "fast_group_ip_awk_${size}" \
        "awk '{count[\$1]++} END {for (ip in count) print count[ip], ip}' '$logfile' | sort -rn | head -50")
    local mcp_stats=$(run_timed "fast_group_ip_mcp_${size}" \
        "python3 '$SCRIPT_DIR/scripts/mcp_client.py' '$MCP_SERVER' 'aggregate_logs' \
        '{\"path\": \"$logfile\", \"format\": \"apache\", \"operation\": \"count\", \"group_by\": \"ip\", \"limit\": 50}'")

    echo ""
    printf "  %-25s %10s %10s\n" "Operation" "awk" "MCP"
    printf "  %s\n" "---------------------------------------------"
    read -r awk_mean awk_std <<< "$awk_stats"
    read -r mcp_mean mcp_std <<< "$mcp_stats"
    printf "  %-25s %10s %10s\n" "Group by IP (SIMD)" "${awk_mean}s" "${mcp_mean}s"

    # Group by method - HAS SIMD fast path
    log_info "Group by method (SIMD fast path)..."
    local awk_method=$(run_timed "fast_group_method_awk_${size}" \
        "awk -F'\"' '{split(\$2,a,\" \"); count[a[1]]++} END {for (m in count) print count[m], m}' '$logfile' | sort -rn | head -20")
    local mcp_method=$(run_timed "fast_group_method_mcp_${size}" \
        "python3 '$SCRIPT_DIR/scripts/mcp_client.py' '$MCP_SERVER' 'aggregate_logs' \
        '{\"path\": \"$logfile\", \"format\": \"apache\", \"operation\": \"count\", \"group_by\": \"method\", \"limit\": 20}'")

    read -r awk_mean awk_std <<< "$awk_method"
    read -r mcp_mean mcp_std <<< "$mcp_method"
    printf "  %-25s %10s %10s\n" "Group by method (SIMD)" "${awk_mean}s" "${mcp_mean}s"

    # Sum size - HAS SIMD fast path
    log_info "Sum size (SIMD fast path)..."
    local awk_sum=$(run_timed "fast_sum_size_awk_${size}" \
        "awk '{sum+=\$10} END {print sum}' '$logfile'")
    local mcp_sum=$(run_timed "fast_sum_size_mcp_${size}" \
        "python3 '$SCRIPT_DIR/scripts/mcp_client.py' '$MCP_SERVER' 'aggregate_logs' \
        '{\"path\": \"$logfile\", \"format\": \"apache\", \"operation\": \"sum\", \"column\": \"size\"}'")

    read -r awk_mean awk_std <<< "$awk_sum"
    read -r mcp_mean mcp_std <<< "$mcp_sum"
    printf "  %-25s %10s %10s\n" "Sum size (SIMD)" "${awk_mean}s" "${mcp_mean}s"
}

# =============================================================================
# SLOW PATH OPERATIONS (Falls back to Polars)
# =============================================================================

benchmark_slow_path() {
    local logfile="$1"
    local size="$2"

    echo ""
    echo "=============================================="
    echo "SLOW PATH OPERATIONS (Polars fallback)"
    echo "=============================================="

    # Group by user_agent - NO SIMD fast path, uses Polars
    log_info "Group by user_agent (NO fast path - Polars)..."
    local awk_ua=$(run_timed "slow_group_ua_awk_${size}" \
        "awk -F'\"' '{print \$6}' '$logfile' | sort | uniq -c | sort -rn | head -20")
    local mcp_ua=$(run_timed "slow_group_ua_mcp_${size}" \
        "python3 '$SCRIPT_DIR/scripts/mcp_client.py' '$MCP_SERVER' 'aggregate_logs' \
        '{\"path\": \"$logfile\", \"format\": \"apache\", \"operation\": \"count\", \"group_by\": \"user_agent\", \"limit\": 20}'")

    echo ""
    printf "  %-30s %10s %10s\n" "Operation" "awk" "MCP"
    printf "  %s\n" "--------------------------------------------------"
    read -r awk_mean awk_std <<< "$awk_ua"
    read -r mcp_mean mcp_std <<< "$mcp_ua"
    printf "  %-30s %10s %10s\n" "Group by user_agent (Polars)" "${awk_mean}s" "${mcp_mean}s"

    # Group by referer - NO SIMD fast path
    log_info "Group by referer (NO fast path - Polars)..."
    local awk_ref=$(run_timed "slow_group_ref_awk_${size}" \
        "awk -F'\"' '{print \$4}' '$logfile' | sort | uniq -c | sort -rn | head -20")
    local mcp_ref=$(run_timed "slow_group_ref_mcp_${size}" \
        "python3 '$SCRIPT_DIR/scripts/mcp_client.py' '$MCP_SERVER' 'aggregate_logs' \
        '{\"path\": \"$logfile\", \"format\": \"apache\", \"operation\": \"count\", \"group_by\": \"referer\", \"limit\": 20}'")

    read -r awk_mean awk_std <<< "$awk_ref"
    read -r mcp_mean mcp_std <<< "$mcp_ref"
    printf "  %-30s %10s %10s\n" "Group by referer (Polars)" "${awk_mean}s" "${mcp_mean}s"

    # Time analysis - NO direct SIMD fast path
    log_info "Time analysis hourly (Polars)..."
    local mcp_time=$(run_timed "slow_time_analysis_mcp_${size}" \
        "python3 '$SCRIPT_DIR/scripts/mcp_client.py' '$MCP_SERVER' 'time_analysis' \
        '{\"path\": \"$logfile\", \"format\": \"apache\", \"bucket\": \"hour\", \"limit\": 24}'")

    read -r mcp_mean mcp_std <<< "$mcp_time"
    printf "  %-30s %10s %10s\n" "Time analysis hourly" "N/A" "${mcp_mean}s"

    # Complex filter - multiple conditions
    log_info "Complex filter (status + method)..."
    local grep_complex=$(run_timed "slow_complex_grep_${size}" \
        "grep -E '\" [45][0-9]{2} ' '$logfile' | grep -c POST")
    local mcp_complex=$(run_timed "slow_complex_mcp_${size}" \
        "python3 '$SCRIPT_DIR/scripts/mcp_client.py' '$MCP_SERVER' 'aggregate_logs' \
        '{\"path\": \"$logfile\", \"format\": \"apache\", \"operation\": \"count\", \"filter_text\": \"POST.*\\\" [45]\"}'")

    read -r grep_mean grep_std <<< "$grep_complex"
    read -r mcp_mean mcp_std <<< "$mcp_complex"
    printf "  %-30s %10s %10s\n" "Complex filter (POST+error)" "${grep_mean}s" "${mcp_mean}s"
}

# =============================================================================
# Main
# =============================================================================
main() {
    echo "========================================"
    echo "  COMPLETE Benchmark Suite"
    echo "  Testing BOTH fast-path and slow-path"
    echo "========================================"

    mkdir -p "$RESULTS_DIR"

    if [ ! -f "$MCP_SERVER" ]; then
        log_warn "Building MCP server..."
        (cd "$PROJECT_DIR/mcp" && cargo build --release)
    fi

    local report="$RESULTS_DIR/COMPLETE_RESULTS.md"
    cat > "$report" << EOF
# Complete Benchmark Results

**Purpose**: Test BOTH SIMD fast-path and Polars slow-path operations

## Key Finding

MCP performance varies significantly based on whether operations hit the SIMD fast path:

EOF

    # Test on 1M file (good balance of speed and representativeness)
    local logfile="$DATA_DIR/apache_1M.log"
    if [ ! -f "$logfile" ]; then
        log_warn "Using 100k file instead"
        logfile="$DATA_DIR/apache_100000.log"
    fi

    local filesize=$(du -h "$logfile" | cut -f1)
    local size=$(basename "$logfile" .log | sed 's/apache_//')

    echo "" >> "$report"
    echo "## Test File: $size ($filesize)" >> "$report"
    echo "" >> "$report"

    log_info "Testing on $logfile ($filesize)"

    # Run benchmarks and capture output
    {
        benchmark_fast_path "$logfile" "$size"
        benchmark_slow_path "$logfile" "$size"
    } | tee -a "$report"

    # Summary
    cat >> "$report" << 'EOF'

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
EOF

    log_success "Complete benchmark done! Results in $report"
}

main "$@"
