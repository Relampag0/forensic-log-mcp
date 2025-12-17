#!/usr/bin/env python3
"""
Statistical analysis helper for benchmarks.
Computes mean, median, stddev, IQR, and percentiles.
"""

import sys
import json
import statistics
from typing import List, Dict, Any

def compute_stats(values: List[float]) -> Dict[str, Any]:
    """Compute comprehensive statistics for a list of values."""
    if not values:
        return {"error": "no values"}

    n = len(values)
    sorted_vals = sorted(values)

    # Basic stats
    mean = statistics.mean(values)

    if n >= 2:
        stddev = statistics.stdev(values)
    else:
        stddev = 0.0

    # Median
    median = statistics.median(values)

    # Quartiles and IQR
    if n >= 4:
        q1 = sorted_vals[n // 4]
        q3 = sorted_vals[(3 * n) // 4]
    else:
        q1 = sorted_vals[0]
        q3 = sorted_vals[-1]

    iqr = q3 - q1

    # Percentiles
    def percentile(p: float) -> float:
        idx = int(p * (n - 1))
        return sorted_vals[idx]

    # Outlier detection (1.5 * IQR rule)
    lower_bound = q1 - 1.5 * iqr
    upper_bound = q3 + 1.5 * iqr
    outliers = [v for v in values if v < lower_bound or v > upper_bound]
    clean_values = [v for v in values if lower_bound <= v <= upper_bound]

    # Clean mean (without outliers)
    clean_mean = statistics.mean(clean_values) if clean_values else mean

    return {
        "n": n,
        "mean": round(mean, 6),
        "median": round(median, 6),
        "stddev": round(stddev, 6),
        "min": round(min(values), 6),
        "max": round(max(values), 6),
        "q1": round(q1, 6),
        "q3": round(q3, 6),
        "iqr": round(iqr, 6),
        "p50": round(percentile(0.50), 6),
        "p95": round(percentile(0.95), 6) if n >= 20 else round(max(values), 6),
        "p99": round(percentile(0.99), 6) if n >= 100 else round(max(values), 6),
        "outliers": len(outliers),
        "clean_mean": round(clean_mean, 6),
        "cv": round(stddev / mean * 100, 2) if mean > 0 else 0,  # Coefficient of variation %
    }


def main():
    """Read values from stdin (one per line) and output stats as JSON."""
    values = []
    for line in sys.stdin:
        line = line.strip()
        if line:
            try:
                values.append(float(line))
            except ValueError:
                pass

    stats = compute_stats(values)
    print(json.dumps(stats, indent=2))


if __name__ == "__main__":
    main()
