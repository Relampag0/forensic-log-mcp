#!/usr/bin/env python3
"""
Simple MCP client for benchmarking the forensic-log-mcp server.
Sends JSON-RPC requests via stdin/stdout.
"""

import json
import subprocess
import sys
import time
from pathlib import Path


def call_mcp_tool(server_path: str, tool_name: str, arguments: dict) -> dict:
    """Call an MCP tool and return the result."""

    # Initialize request
    init_request = {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "benchmark-client",
                "version": "1.0.0"
            }
        }
    }

    # Tool call request
    tool_request = {
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": tool_name,
            "arguments": arguments
        }
    }

    # Start the MCP server process
    proc = subprocess.Popen(
        [server_path],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True
    )

    try:
        # Send initialize
        proc.stdin.write(json.dumps(init_request) + "\n")
        proc.stdin.flush()

        # Read initialize response
        init_response = proc.stdout.readline()

        # Send initialized notification
        initialized = {
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }
        proc.stdin.write(json.dumps(initialized) + "\n")
        proc.stdin.flush()

        # Send tool call
        proc.stdin.write(json.dumps(tool_request) + "\n")
        proc.stdin.flush()

        # Read tool response
        tool_response = proc.stdout.readline()

        return json.loads(tool_response)

    finally:
        proc.terminate()
        proc.wait()


def main():
    if len(sys.argv) < 4:
        print("Usage: mcp_client.py <server_path> <tool_name> <arguments_json>")
        sys.exit(1)

    server_path = sys.argv[1]
    tool_name = sys.argv[2]
    arguments = json.loads(sys.argv[3])

    start = time.time()
    result = call_mcp_tool(server_path, tool_name, arguments)
    elapsed = time.time() - start

    print(json.dumps({
        "result": result,
        "elapsed_seconds": elapsed
    }, indent=2))


if __name__ == "__main__":
    main()
