#!/bin/bash
# Benchmark wrapper for MCP aggregate_logs with group by

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$(dirname "$SCRIPT_DIR")")"
MCP_SERVER="$PROJECT_DIR/mcp/target/release/forensic-log-mcp"

LOGFILE="$1"
FORMAT="$2"

# Choose group_by column based on format
case "$FORMAT" in
    apache)
        GROUP_BY="ip"
        ;;
    json)
        GROUP_BY="service"
        ;;
    syslog)
        GROUP_BY="hostname"
        ;;
    *)
        GROUP_BY="ip"
        ;;
esac

python3 "$SCRIPT_DIR/mcp_client.py" "$MCP_SERVER" "aggregate_logs" \
    "{\"path\": \"$LOGFILE\", \"format\": \"$FORMAT\", \"operation\": \"count\", \"group_by\": \"$GROUP_BY\", \"limit\": 20}"
