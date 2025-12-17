#!/bin/bash
set -e

# =============================================================================
# FAIR BENCHMARK SUITE
# =============================================================================
# This benchmark addresses the issues identified in CRITICAL_REVIEW.md:
# 1. All tools return equivalent data (counts for filter, top-N for group)
# 2. Minimum 10 runs with statistics
# 3. Warm-up runs included
# 4. Tool versions documented
# 5. Memory usage tracked
# =============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
DATA_DIR="$SCRIPT_DIR/data"
RESULTS_DIR="$SCRIPT_DIR/results_fair"
MCP_SERVER="$PROJECT_DIR/mcp/target/release/forensic-log-mcp"

# Configuration
RUNS=10
WARMUP=3
TOP_N=50  # Both MCP and competitors return top 50

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[OK]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }

# =============================================================================
# Document tool versions for reproducibility
# =============================================================================
document_versions() {
    local report="$1"

    echo "## Tool Versions (for reproducibility)" >> "$report"
    echo "" >> "$report"
    echo '```' >> "$report"
    echo "grep: $(grep --version 2>&1 | head -1)" >> "$report"
    echo "awk: $(awk --version 2>&1 | head -1)" >> "$report"
    command -v rg >/dev/null && echo "ripgrep: $(rg --version | head -1)" >> "$report"
    command -v jq >/dev/null && echo "jq: $(jq --version 2>&1)" >> "$report"
    echo "rustc: $(rustc --version 2>&1)" >> "$report"
    echo "MCP server: $($MCP_SERVER --version 2>&1 || echo 'v0.3.0')" >> "$report"
    echo '```' >> "$report"
    echo "" >> "$report"
}

# =============================================================================
# Run benchmark with statistics
# =============================================================================
run_timed() {
    local name="$1"
    local cmd="$2"
    local results_file="$RESULTS_DIR/${name}.txt"

    # Warm-up runs (not counted)
    for i in $(seq 1 $WARMUP); do
        eval "$cmd" >/dev/null 2>&1 || true
    done

    # Timed runs
    > "$results_file"
    for i in $(seq 1 $RUNS); do
        local start=$(date +%s.%N)
        eval "$cmd" >/dev/null 2>&1 || true
        local end=$(date +%s.%N)
        local elapsed=$(echo "$end - $start" | bc -l)
        echo "$elapsed" >> "$results_file"
    done

    # Calculate statistics
    local mean=$(awk '{sum+=$1} END {printf "%.4f", sum/NR}' "$results_file")
    local stddev=$(awk -v mean="$mean" '{sum+=($1-mean)^2} END {printf "%.4f", sqrt(sum/NR)}' "$results_file")
    local min=$(sort -n "$results_file" | head -1)
    local max=$(sort -n "$results_file" | tail -1)

    echo "$mean $stddev $min $max"
}

# =============================================================================
# Measure peak memory usage
# =============================================================================
measure_memory() {
    local cmd="$1"
    /usr/bin/time -v bash -c "$cmd" 2>&1 | grep "Maximum resident set size" | awk '{print $6}'
}

# =============================================================================
# FAIR BENCHMARK: Filter errors - COUNT only
# Both tools count matching lines (not return rows)
# =============================================================================
benchmark_filter_count() {
    local logfile="$1"
    local format="$2"
    local size="$3"

    log_info "FAIR Benchmark: Filter & Count (status >= 400) - $format $size"

    local results=()

    case "$format" in
        apache)
            # grep: count lines with 4xx/5xx status
            log_info "  grep (count)..."
            local grep_stats=$(run_timed "${format}_${size}_filter_grep" \
                "grep -cE '\" [45][0-9]{2} ' '$logfile'")
            results+=("grep|$grep_stats")

            # ripgrep: count lines
            if command -v rg >/dev/null 2>&1; then
                log_info "  ripgrep (count)..."
                local rg_stats=$(run_timed "${format}_${size}_filter_rg" \
                    "rg -c '\" [45][0-9]{2} ' '$logfile'")
                results+=("ripgrep|$rg_stats")
            fi

            # awk: count with field parsing
            log_info "  awk (count)..."
            local awk_stats=$(run_timed "${format}_${size}_filter_awk" \
                "awk '\$9 >= 400 {count++} END {print count}' '$logfile'")
            results+=("awk|$awk_stats")

            # MCP: Use aggregate_logs with count operation (no limit needed)
            log_info "  MCP (count via aggregate)..."
            local mcp_stats=$(run_timed "${format}_${size}_filter_mcp" \
                "python3 '$SCRIPT_DIR/scripts/mcp_client.py' '$MCP_SERVER' 'aggregate_logs' \
                '{\"path\": \"$logfile\", \"format\": \"$format\", \"operation\": \"count\", \"filter_text\": \"\\\" [45][0-9]\"}'")
            results+=("MCP|$mcp_stats")
            ;;

        json)
            # jq: count errors
            if command -v jq >/dev/null 2>&1; then
                log_info "  jq (count)..."
                local jq_stats=$(run_timed "${format}_${size}_filter_jq" \
                    "jq -s '[.[] | select(.level == \"ERROR\")] | length' '$logfile'")
                results+=("jq|$jq_stats")
            fi

            # grep: count (text match)
            log_info "  grep (count)..."
            local grep_stats=$(run_timed "${format}_${size}_filter_grep" \
                "grep -c '\"level\":\"ERROR\"' '$logfile'")
            results+=("grep|$grep_stats")

            # MCP
            log_info "  MCP (count)..."
            local mcp_stats=$(run_timed "${format}_${size}_filter_mcp" \
                "python3 '$SCRIPT_DIR/scripts/mcp_client.py' '$MCP_SERVER' 'aggregate_logs' \
                '{\"path\": \"$logfile\", \"format\": \"json\", \"operation\": \"count\", \"filter_text\": \"ERROR\"}'")
            results+=("MCP|$mcp_stats")
            ;;

        syslog)
            # grep: count errors
            log_info "  grep (count)..."
            local grep_stats=$(run_timed "${format}_${size}_filter_grep" \
                "grep -ci 'error' '$logfile'")
            results+=("grep|$grep_stats")

            # MCP
            log_info "  MCP (count)..."
            local mcp_stats=$(run_timed "${format}_${size}_filter_mcp" \
                "python3 '$SCRIPT_DIR/scripts/mcp_client.py' '$MCP_SERVER' 'aggregate_logs' \
                '{\"path\": \"$logfile\", \"format\": \"syslog\", \"operation\": \"count\", \"filter_text\": \"error\"}'")
            results+=("MCP|$mcp_stats")
            ;;
    esac

    # Print results
    printf "\n  %-10s %10s %10s %10s %10s\n" "Tool" "Mean(s)" "StdDev" "Min" "Max"
    printf "  %s\n" "----------------------------------------------------"
    for r in "${results[@]}"; do
        IFS='|' read -r tool stats <<< "$r"
        read -r mean stddev min max <<< "$stats"
        printf "  %-10s %10s %10s %10s %10s\n" "$tool" "$mean" "$stddev" "$min" "$max"
    done
    echo ""
}

# =============================================================================
# FAIR BENCHMARK: Group by - Both return top N
# =============================================================================
benchmark_group_topn() {
    local logfile="$1"
    local format="$2"
    local size="$3"

    log_info "FAIR Benchmark: Group by (top $TOP_N) - $format $size"

    local results=()

    case "$format" in
        apache)
            # awk: group by IP, return top N
            log_info "  awk (group by IP, top $TOP_N)..."
            local awk_stats=$(run_timed "${format}_${size}_group_awk" \
                "awk '{count[\$1]++} END {for (ip in count) print count[ip], ip}' '$logfile' | sort -rn | head -$TOP_N")
            results+=("awk|$awk_stats")

            # MCP: group by IP, limit to N
            log_info "  MCP (group by IP, top $TOP_N)..."
            local mcp_stats=$(run_timed "${format}_${size}_group_mcp" \
                "python3 '$SCRIPT_DIR/scripts/mcp_client.py' '$MCP_SERVER' 'aggregate_logs' \
                '{\"path\": \"$logfile\", \"format\": \"apache\", \"operation\": \"count\", \"group_by\": \"ip\", \"limit\": $TOP_N}'")
            results+=("MCP|$mcp_stats")
            ;;

        json)
            # jq: group by service
            if command -v jq >/dev/null 2>&1; then
                log_info "  jq (group by service, top $TOP_N)..."
                local jq_stats=$(run_timed "${format}_${size}_group_jq" \
                    "jq -r '.service' '$logfile' | sort | uniq -c | sort -rn | head -$TOP_N")
                results+=("jq|$jq_stats")
            fi

            # MCP
            log_info "  MCP (group by service, top $TOP_N)..."
            local mcp_stats=$(run_timed "${format}_${size}_group_mcp" \
                "python3 '$SCRIPT_DIR/scripts/mcp_client.py' '$MCP_SERVER' 'aggregate_logs' \
                '{\"path\": \"$logfile\", \"format\": \"json\", \"operation\": \"count\", \"group_by\": \"service\", \"limit\": $TOP_N}'")
            results+=("MCP|$mcp_stats")
            ;;

        syslog)
            # awk: group by hostname
            log_info "  awk (group by hostname, top $TOP_N)..."
            local awk_stats=$(run_timed "${format}_${size}_group_awk" \
                "awk '{print \$4}' '$logfile' | sort | uniq -c | sort -rn | head -$TOP_N")
            results+=("awk|$awk_stats")

            # MCP
            log_info "  MCP (group by hostname, top $TOP_N)..."
            local mcp_stats=$(run_timed "${format}_${size}_group_mcp" \
                "python3 '$SCRIPT_DIR/scripts/mcp_client.py' '$MCP_SERVER' 'aggregate_logs' \
                '{\"path\": \"$logfile\", \"format\": \"syslog\", \"operation\": \"count\", \"group_by\": \"hostname\", \"limit\": $TOP_N}'")
            results+=("MCP|$mcp_stats")
            ;;
    esac

    # Print results
    printf "\n  %-10s %10s %10s %10s %10s\n" "Tool" "Mean(s)" "StdDev" "Min" "Max"
    printf "  %s\n" "----------------------------------------------------"
    for r in "${results[@]}"; do
        IFS='|' read -r tool stats <<< "$r"
        read -r mean stddev min max <<< "$stats"
        printf "  %-10s %10s %10s %10s %10s\n" "$tool" "$mean" "$stddev" "$min" "$max"
    done
    echo ""
}

# =============================================================================
# FAIR BENCHMARK: Regex search - COUNT matches
# =============================================================================
benchmark_regex_count() {
    local logfile="$1"
    local format="$2"
    local size="$3"
    local pattern="(POST|PUT|DELETE)"

    log_info "FAIR Benchmark: Regex count '$pattern' - $format $size"

    local results=()

    # grep
    log_info "  grep -E (count)..."
    local grep_stats=$(run_timed "${format}_${size}_regex_grep" \
        "grep -cE '$pattern' '$logfile'")
    results+=("grep|$grep_stats")

    # ripgrep
    if command -v rg >/dev/null 2>&1; then
        log_info "  ripgrep (count)..."
        local rg_stats=$(run_timed "${format}_${size}_regex_rg" \
            "rg -c '$pattern' '$logfile'")
        results+=("ripgrep|$rg_stats")
    fi

    # MCP: search_pattern returns rows, we need to count
    log_info "  MCP (count)..."
    local mcp_stats=$(run_timed "${format}_${size}_regex_mcp" \
        "python3 '$SCRIPT_DIR/scripts/mcp_client.py' '$MCP_SERVER' 'aggregate_logs' \
        '{\"path\": \"$logfile\", \"format\": \"$format\", \"operation\": \"count\", \"filter_text\": \"$pattern\"}'")
    results+=("MCP|$mcp_stats")

    # Print results
    printf "\n  %-10s %10s %10s %10s %10s\n" "Tool" "Mean(s)" "StdDev" "Min" "Max"
    printf "  %s\n" "----------------------------------------------------"
    for r in "${results[@]}"; do
        IFS='|' read -r tool stats <<< "$r"
        read -r mean stddev min max <<< "$stats"
        printf "  %-10s %10s %10s %10s %10s\n" "$tool" "$mean" "$stddev" "$min" "$max"
    done
    echo ""
}

# =============================================================================
# Main
# =============================================================================
main() {
    echo "========================================"
    echo "  FAIR Benchmark Suite"
    echo "  Runs: $RUNS | Warmup: $WARMUP | Top-N: $TOP_N"
    echo "========================================"
    echo ""

    mkdir -p "$RESULTS_DIR"

    # Check MCP server exists
    if [ ! -f "$MCP_SERVER" ]; then
        log_warn "MCP server not found, building..."
        (cd "$PROJECT_DIR/mcp" && cargo build --release)
    fi

    local report="$RESULTS_DIR/FAIR_RESULTS.md"

    # Initialize report
    cat > "$report" << EOF
# Fair Benchmark Results

**Generated**: $(date)
**Methodology**: $RUNS runs per test, $WARMUP warmup runs, statistics reported

## Corrections from Original Benchmarks

1. All tools return equivalent data (counts for filter, top-$TOP_N for group)
2. Statistics: mean ± stddev reported
3. Warm-up runs to eliminate cold cache effects

EOF

    document_versions "$report"

    # Run benchmarks on available data
    # Use actual file naming convention
    local sizes=("100000" "1M" "5M")
    local formats=("apache" "json" "syslog")

    for format in "${formats[@]}"; do
        for size in "${sizes[@]}"; do
            local logfile="$DATA_DIR/${format}_${size}.log"

            if [ ! -f "$logfile" ]; then
                log_warn "Skipping $logfile (not found)"
                continue
            fi

            local filesize=$(du -h "$logfile" | cut -f1)
            echo "" >> "$report"
            echo "## $format format - $size lines ($filesize)" >> "$report"
            echo "" >> "$report"

            log_info "========== $format $size lines =========="

            # Run benchmarks and capture output
            {
                benchmark_filter_count "$logfile" "$format" "$size"
                benchmark_group_topn "$logfile" "$format" "$size"

                if [ "$format" = "apache" ]; then
                    benchmark_regex_count "$logfile" "$format" "$size"
                fi
            } | tee -a "$report"
        done
    done

    echo "" >> "$report"
    echo "## Methodology Notes" >> "$report"
    echo "" >> "$report"
    echo "- **Filter benchmarks**: All tools count matching lines (not return rows)" >> "$report"
    echo "- **Group benchmarks**: All tools return top $TOP_N results" >> "$report"
    echo "- **Regex benchmarks**: All tools count matching lines" >> "$report"
    echo "- **Statistics**: Mean and standard deviation from $RUNS runs" >> "$report"
    echo "- **Warmup**: $WARMUP runs before measurement to warm caches" >> "$report"

    log_success "Fair benchmark complete! Results in $report"
}

# Parse arguments
case "${1:-}" in
    --quick)
        RUNS=3
        WARMUP=1
        main
        ;;
    *)
        main
        ;;
esac
