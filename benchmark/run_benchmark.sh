#!/bin/bash
set -e

# Benchmark Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
DATA_DIR="$SCRIPT_DIR/data"
RESULTS_DIR="$SCRIPT_DIR/results"
GENERATOR="$SCRIPT_DIR/generate_logs/target/release/generate_logs"
MCP_SERVER="$PROJECT_DIR/mcp/target/release/forensic-log-mcp"

# Test sizes (number of lines)
SIZES=(100000 500000 1000000 5000000)
FORMATS=("apache" "json" "syslog")

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[OK]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check required tools
check_dependencies() {
    log_info "Checking dependencies..."

    local missing=()

    command -v grep >/dev/null 2>&1 || missing+=("grep")
    command -v awk >/dev/null 2>&1 || missing+=("awk")
    command -v python3 >/dev/null 2>&1 || missing+=("python3")
    command -v jq >/dev/null 2>&1 || missing+=("jq")
    command -v hyperfine >/dev/null 2>&1 || missing+=("hyperfine")

    if [ ${#missing[@]} -ne 0 ]; then
        log_warn "Missing optional tools: ${missing[*]}"
        log_warn "Some benchmarks may be skipped"
    fi

    # Check if generator exists
    if [ ! -f "$GENERATOR" ]; then
        log_info "Building log generator..."
        (cd "$SCRIPT_DIR/generate_logs" && cargo build --release)
    fi

    # Check if MCP server exists
    if [ ! -f "$MCP_SERVER" ]; then
        log_info "Building MCP server..."
        (cd "$PROJECT_DIR/mcp" && cargo build --release)
    fi

    log_success "Dependencies checked"
}

# Generate test data
generate_test_data() {
    log_info "Generating test data..."
    mkdir -p "$DATA_DIR"

    for format in "${FORMATS[@]}"; do
        for size in "${SIZES[@]}"; do
            local filename="${format}_${size}.log"
            local filepath="$DATA_DIR/$filename"

            if [ -f "$filepath" ]; then
                log_info "Skipping $filename (already exists)"
                continue
            fi

            log_info "Generating $filename..."
            "$GENERATOR" -o "$filepath" -l "$size" -f "$format" -e 0.05
        done
    done

    log_success "Test data generated"
}

# Run a single benchmark
run_benchmark() {
    local name="$1"
    local cmd="$2"
    local warmup="${3:-1}"
    local runs="${4:-5}"

    if command -v hyperfine >/dev/null 2>&1; then
        hyperfine --warmup "$warmup" --runs "$runs" --export-json "$RESULTS_DIR/${name}.json" "$cmd" 2>/dev/null
    else
        # Fallback to simple timing
        local total=0
        for i in $(seq 1 $runs); do
            local start=$(date +%s.%N)
            eval "$cmd" >/dev/null 2>&1
            local end=$(date +%s.%N)
            local elapsed=$(echo "$end - $start" | bc)
            total=$(echo "$total + $elapsed" | bc)
        done
        local avg=$(echo "scale=3; $total / $runs" | bc)
        echo "{\"mean\": $avg, \"command\": \"$cmd\"}" > "$RESULTS_DIR/${name}.json"
        echo "Average: ${avg}s"
    fi
}

# Benchmark: Count lines with status >= 400
benchmark_filter_errors() {
    local logfile="$1"
    local format="$2"
    local size="$3"
    local prefix="${format}_${size}_filter_errors"

    log_info "Benchmark: Filter errors (status >= 400) - $format $size lines"

    case "$format" in
        apache)
            # grep
            if command -v grep >/dev/null 2>&1; then
                log_info "  Running grep..."
                run_benchmark "${prefix}_grep" "grep -E '\" [45][0-9]{2} ' '$logfile' | wc -l"
            fi

            # awk
            if command -v awk >/dev/null 2>&1; then
                log_info "  Running awk..."
                run_benchmark "${prefix}_awk" "awk '\$9 >= 400' '$logfile' | wc -l"
            fi

            # ripgrep
            if command -v rg >/dev/null 2>&1; then
                log_info "  Running ripgrep..."
                run_benchmark "${prefix}_rg" "rg '\" [45][0-9]{2} ' '$logfile' | wc -l"
            fi
            ;;
        json)
            # jq
            if command -v jq >/dev/null 2>&1; then
                log_info "  Running jq..."
                run_benchmark "${prefix}_jq" "jq -c 'select(.level == \"ERROR\")' '$logfile' | wc -l"
            fi

            # grep (fallback)
            if command -v grep >/dev/null 2>&1; then
                log_info "  Running grep..."
                run_benchmark "${prefix}_grep" "grep '\"level\":\"ERROR\"' '$logfile' | wc -l"
            fi
            ;;
        syslog)
            # grep
            if command -v grep >/dev/null 2>&1; then
                log_info "  Running grep..."
                run_benchmark "${prefix}_grep" "grep -i 'error' '$logfile' | wc -l"
            fi
            ;;
    esac

    # MCP tool (via direct binary call for benchmarking)
    log_info "  Running forensic-log-mcp..."
    run_benchmark "${prefix}_mcp" "$SCRIPT_DIR/scripts/mcp_filter_errors.sh '$logfile' '$format'"
}

# Benchmark: Count by group (IP or service)
benchmark_group_count() {
    local logfile="$1"
    local format="$2"
    local size="$3"
    local prefix="${format}_${size}_group_count"

    log_info "Benchmark: Group count - $format $size lines"

    case "$format" in
        apache)
            # awk
            if command -v awk >/dev/null 2>&1; then
                log_info "  Running awk..."
                run_benchmark "${prefix}_awk" "awk '{count[\$1]++} END {for (ip in count) print ip, count[ip]}' '$logfile' | sort -k2 -rn | head -20"
            fi
            ;;
        json)
            # jq
            if command -v jq >/dev/null 2>&1; then
                log_info "  Running jq..."
                run_benchmark "${prefix}_jq" "jq -r '.service' '$logfile' | sort | uniq -c | sort -rn | head -20"
            fi
            ;;
    esac

    # MCP tool
    log_info "  Running forensic-log-mcp..."
    run_benchmark "${prefix}_mcp" "$SCRIPT_DIR/scripts/mcp_group_count.sh '$logfile' '$format'"
}

# Benchmark: Search pattern
benchmark_search() {
    local logfile="$1"
    local format="$2"
    local size="$3"
    local prefix="${format}_${size}_search"

    log_info "Benchmark: Search pattern - $format $size lines"

    local pattern="timeout|connection"

    # grep
    if command -v grep >/dev/null 2>&1; then
        log_info "  Running grep..."
        run_benchmark "${prefix}_grep" "grep -Ei '$pattern' '$logfile' | wc -l"
    fi

    # ripgrep
    if command -v rg >/dev/null 2>&1; then
        log_info "  Running ripgrep..."
        run_benchmark "${prefix}_rg" "rg -i '$pattern' '$logfile' | wc -l"
    fi

    # MCP tool
    log_info "  Running forensic-log-mcp..."
    run_benchmark "${prefix}_mcp" "$SCRIPT_DIR/scripts/mcp_search.sh '$logfile' '$format' '$pattern'"
}

# Generate report
generate_report() {
    log_info "Generating report..."

    local report="$RESULTS_DIR/benchmark_report.md"

    cat > "$report" << 'EOF'
# Forensic Log MCP Benchmark Results

## Test Environment
EOF

    echo "- **Date**: $(date)" >> "$report"
    echo "- **OS**: $(uname -s) $(uname -r)" >> "$report"
    echo "- **CPU**: $(grep 'model name' /proc/cpuinfo | head -1 | cut -d: -f2 | xargs)" >> "$report"
    echo "- **RAM**: $(free -h | grep Mem | awk '{print $2}')" >> "$report"
    echo "" >> "$report"

    echo "## Results Summary" >> "$report"
    echo "" >> "$report"

    # Parse JSON results if hyperfine was used
    if command -v jq >/dev/null 2>&1; then
        echo "| Test | Tool | Mean (s) | Stddev (s) |" >> "$report"
        echo "|------|------|----------|------------|" >> "$report"

        for json_file in "$RESULTS_DIR"/*.json; do
            if [ -f "$json_file" ]; then
                local test_name=$(basename "$json_file" .json)
                local mean=$(jq -r '.results[0].mean // .mean // "N/A"' "$json_file" 2>/dev/null)
                local stddev=$(jq -r '.results[0].stddev // "N/A"' "$json_file" 2>/dev/null)
                echo "| $test_name | - | $mean | $stddev |" >> "$report"
            fi
        done
    fi

    log_success "Report generated: $report"
}

# Main
main() {
    echo "========================================"
    echo "  Forensic Log MCP Benchmark Suite"
    echo "========================================"
    echo ""

    mkdir -p "$RESULTS_DIR"

    check_dependencies
    generate_test_data

    # Run benchmarks for each format and size
    for format in "${FORMATS[@]}"; do
        for size in "${SIZES[@]}"; do
            local logfile="$DATA_DIR/${format}_${size}.log"

            if [ ! -f "$logfile" ]; then
                log_warn "Skipping $logfile (not found)"
                continue
            fi

            echo ""
            log_info "========== $format format, $size lines =========="

            benchmark_filter_errors "$logfile" "$format" "$size"
            benchmark_group_count "$logfile" "$format" "$size"
            benchmark_search "$logfile" "$format" "$size"
        done
    done

    generate_report

    echo ""
    log_success "Benchmark complete! Results in $RESULTS_DIR"
}

# Parse arguments
case "${1:-}" in
    --generate-only)
        check_dependencies
        generate_test_data
        ;;
    --small)
        SIZES=(100000)
        main
        ;;
    *)
        main
        ;;
esac
