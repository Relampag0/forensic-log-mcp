#!/usr/bin/env python3
"""
Pandas-based log analysis for benchmark comparison.
"""

import argparse
import json
import re
import sys
import time
from pathlib import Path

try:
    import pandas as pd
except ImportError:
    print("pandas not installed. Run: pip install pandas")
    sys.exit(1)


def parse_apache_log(filepath: str) -> pd.DataFrame:
    """Parse Apache combined log format into DataFrame."""
    pattern = r'^(\S+) \S+ \S+ \[([^\]]+)\] "(\S+) (\S+) [^"]*" (\d+) (\d+|-)'

    records = []
    with open(filepath, 'r') as f:
        for line in f:
            match = re.match(pattern, line)
            if match:
                records.append({
                    'ip': match.group(1),
                    'timestamp': match.group(2),
                    'method': match.group(3),
                    'path': match.group(4),
                    'status': int(match.group(5)),
                    'size': int(match.group(6)) if match.group(6) != '-' else 0,
                })

    return pd.DataFrame(records)


def parse_json_log(filepath: str) -> pd.DataFrame:
    """Parse JSON lines format into DataFrame."""
    return pd.read_json(filepath, lines=True)


def parse_syslog(filepath: str) -> pd.DataFrame:
    """Parse syslog format into DataFrame."""
    pattern = r'^(\w{3}\s+\d+\s+\d+:\d+:\d+)\s+(\S+)\s+(\S+)\[(\d+)\]:\s+(.*)$'

    records = []
    with open(filepath, 'r') as f:
        for line in f:
            match = re.match(pattern, line)
            if match:
                records.append({
                    'timestamp': match.group(1),
                    'hostname': match.group(2),
                    'process': match.group(3),
                    'pid': int(match.group(4)),
                    'message': match.group(5),
                })

    return pd.DataFrame(records)


def filter_errors(df: pd.DataFrame, format_type: str) -> pd.DataFrame:
    """Filter error records."""
    if format_type == 'apache':
        return df[df['status'] >= 400]
    elif format_type == 'json':
        return df[df['level'] == 'ERROR']
    elif format_type == 'syslog':
        return df[df['message'].str.contains('ERROR', case=False, na=False)]
    return df


def group_count(df: pd.DataFrame, format_type: str) -> pd.DataFrame:
    """Count by group."""
    if format_type == 'apache':
        return df.groupby('ip').size().reset_index(name='count').sort_values('count', ascending=False).head(20)
    elif format_type == 'json':
        return df.groupby('service').size().reset_index(name='count').sort_values('count', ascending=False).head(20)
    elif format_type == 'syslog':
        return df.groupby('hostname').size().reset_index(name='count').sort_values('count', ascending=False).head(20)
    return df


def search_pattern(df: pd.DataFrame, pattern: str) -> pd.DataFrame:
    """Search for pattern in all text columns."""
    mask = pd.Series([False] * len(df))
    for col in df.select_dtypes(include=['object']).columns:
        mask |= df[col].str.contains(pattern, case=False, na=False, regex=True)
    return df[mask]


def main():
    parser = argparse.ArgumentParser(description='Pandas log analysis benchmark')
    parser.add_argument('filepath', help='Path to log file')
    parser.add_argument('--format', '-f', choices=['apache', 'json', 'syslog'], required=True)
    parser.add_argument('--operation', '-o', choices=['filter_errors', 'group_count', 'search'], required=True)
    parser.add_argument('--pattern', '-p', default='timeout|connection', help='Search pattern')
    parser.add_argument('--json-output', action='store_true', help='Output as JSON')

    args = parser.parse_args()

    start = time.time()

    # Parse log file
    if args.format == 'apache':
        df = parse_apache_log(args.filepath)
    elif args.format == 'json':
        df = parse_json_log(args.filepath)
    elif args.format == 'syslog':
        df = parse_syslog(args.filepath)

    parse_time = time.time() - start

    # Run operation
    op_start = time.time()
    if args.operation == 'filter_errors':
        result = filter_errors(df, args.format)
    elif args.operation == 'group_count':
        result = group_count(df, args.format)
    elif args.operation == 'search':
        result = search_pattern(df, args.pattern)

    op_time = time.time() - op_start
    total_time = time.time() - start

    if args.json_output:
        output = {
            'rows_in': len(df),
            'rows_out': len(result),
            'parse_time': parse_time,
            'operation_time': op_time,
            'total_time': total_time,
        }
        print(json.dumps(output, indent=2))
    else:
        print(f"Input rows: {len(df)}")
        print(f"Output rows: {len(result)}")
        print(f"Parse time: {parse_time:.3f}s")
        print(f"Operation time: {op_time:.3f}s")
        print(f"Total time: {total_time:.3f}s")


if __name__ == '__main__':
    main()
