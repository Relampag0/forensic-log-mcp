#!/bin/bash
# Benchmark wrapper for MCP analyze_logs with error filter

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$(dirname "$SCRIPT_DIR")")"
MCP_SERVER="$PROJECT_DIR/mcp/target/release/forensic-log-mcp"

LOGFILE="$1"
FORMAT="$2"

python3 "$SCRIPT_DIR/mcp_client.py" "$MCP_SERVER" "analyze_logs" \
    "{\"path\": \"$LOGFILE\", \"format\": \"$FORMAT\", \"filter_status\": \">=400\", \"limit\": 1000}"
