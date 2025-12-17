#!/bin/bash
# Benchmark wrapper for MCP search_pattern

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$(dirname "$SCRIPT_DIR")")"
MCP_SERVER="$PROJECT_DIR/mcp/target/release/forensic-log-mcp"

LOGFILE="$1"
FORMAT="$2"
PATTERN="$3"

python3 "$SCRIPT_DIR/mcp_client.py" "$MCP_SERVER" "search_pattern" \
    "{\"path\": \"$LOGFILE\", \"format\": \"$FORMAT\", \"pattern\": \"$PATTERN\", \"limit\": 100}"
