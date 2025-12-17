# Fair Benchmark Results

**Generated**: mer 17 déc 2025 15:45:31 CET
**Methodology**: 3 runs per test, 1 warmup runs, statistics reported

## Corrections from Original Benchmarks

1. All tools return equivalent data (counts for filter, top-50 for group)
2. Statistics: mean ± stddev reported
3. Warm-up runs to eliminate cold cache effects

## Tool Versions (for reproducibility)

```
grep: grep (GNU grep) 3.12-modified
awk: GNU Awk 5.3.2, API 4.0, PMA Avon 8-g1, (GNU MPFR 4.2.2, GNU MP 6.3.0)
ripgrep: ripgrep 14.1.1
jq: jq-1.8.1
rustc: rustc 1.90.0 (1159e78c4 2025-09-14)
MCP server: Error: ConnectionClosed("initialized request")
v0.3.0
```


## apache format - 100000 lines (13M)

[0;34m[INFO][0m FAIR Benchmark: Filter & Count (status >= 400) - apache 100000
[0;34m[INFO][0m   grep (count)...
[0;34m[INFO][0m   ripgrep (count)...
[0;34m[INFO][0m   awk (count)...
[0;34m[INFO][0m   MCP (count via aggregate)...

  Tool          Mean(s)     StdDev        Min        Max
  ----------------------------------------------------
  grep           0.0015     0.0004 .001117893 .002155898
  ripgrep        0.0091     0.0005 .008366911 .009613174
  awk            0.0350     0.0004 .034462901 .035472804
  MCP            0.1330     0.0028 .129106831 .135586877

[0;34m[INFO][0m FAIR Benchmark: Group by (top 50) - apache 100000
[0;34m[INFO][0m   awk (group by IP, top 50)...
[0;34m[INFO][0m   MCP (group by IP, top 50)...

  Tool          Mean(s)     StdDev        Min        Max
  ----------------------------------------------------
  awk            0.0278     0.0018 .025514717 .029885361
  MCP            0.0290     0.0015 .027577932 .031069067

[0;34m[INFO][0m FAIR Benchmark: Regex count '(POST|PUT|DELETE)' - apache 100000
[0;34m[INFO][0m   grep -E (count)...
[0;34m[INFO][0m   ripgrep (count)...
[0;34m[INFO][0m   MCP (count)...

  Tool          Mean(s)     StdDev        Min        Max
  ----------------------------------------------------
  grep           0.0017     0.0003 .001466313 .002088633
  ripgrep        0.0080     0.0003 .007559678 .008266875
  MCP            0.1265     0.0042 .122326827 .132303548


## apache format - 1M lines (125M)

[0;34m[INFO][0m FAIR Benchmark: Filter & Count (status >= 400) - apache 1M
[0;34m[INFO][0m   grep (count)...
[0;34m[INFO][0m   ripgrep (count)...
[0;34m[INFO][0m   awk (count)...
[0;34m[INFO][0m   MCP (count via aggregate)...

  Tool          Mean(s)     StdDev        Min        Max
  ----------------------------------------------------
  grep           0.0017     0.0005 .001168247 .002440768
  ripgrep        0.0552     0.0019 .052547432 .056797863
  awk            0.3292     0.0044 .324880826 .335284714
  MCP            0.9103     0.0073 .899996294 .916193789

[0;34m[INFO][0m FAIR Benchmark: Group by (top 50) - apache 1M
[0;34m[INFO][0m   awk (group by IP, top 50)...
[0;34m[INFO][0m   MCP (group by IP, top 50)...

  Tool          Mean(s)     StdDev        Min        Max
  ----------------------------------------------------
  awk            0.2401     0.0026 .237282286 .243539487
  MCP            0.0422     0.0009 .041061788 .043163515

[0;34m[INFO][0m FAIR Benchmark: Regex count '(POST|PUT|DELETE)' - apache 1M
[0;34m[INFO][0m   grep -E (count)...
[0;34m[INFO][0m   ripgrep (count)...
[0;34m[INFO][0m   MCP (count)...

  Tool          Mean(s)     StdDev        Min        Max
  ----------------------------------------------------
  grep           0.0016     0.0003 .001273513 .001908567
  ripgrep        0.0558     0.0015 .053981655 .057707138
  MCP            0.8943     0.0049 .888747421 .900693363


## apache format - 5M lines (621M)

[0;34m[INFO][0m FAIR Benchmark: Filter & Count (status >= 400) - apache 5M
[0;34m[INFO][0m   grep (count)...
[0;34m[INFO][0m   ripgrep (count)...
[0;34m[INFO][0m   awk (count)...
[0;34m[INFO][0m   MCP (count via aggregate)...

  Tool          Mean(s)     StdDev        Min        Max
  ----------------------------------------------------
  grep           0.0023     0.0008 .001539158 .003356735
  ripgrep        0.2503     0.0029 .246394596 .253470262
  awk            1.6257     0.0179 1.604882900 1.648576874
  MCP            4.9138     0.1592 4.743146180 5.126381955

[0;34m[INFO][0m FAIR Benchmark: Group by (top 50) - apache 5M
[0;34m[INFO][0m   awk (group by IP, top 50)...
[0;34m[INFO][0m   MCP (group by IP, top 50)...

  Tool          Mean(s)     StdDev        Min        Max
  ----------------------------------------------------
  awk            1.1802     0.0026 1.177502794 1.183744265
  MCP            0.0892     0.0009 .088421433 .090396554

[0;34m[INFO][0m FAIR Benchmark: Regex count '(POST|PUT|DELETE)' - apache 5M
[0;34m[INFO][0m   grep -E (count)...
[0;34m[INFO][0m   ripgrep (count)...
[0;34m[INFO][0m   MCP (count)...

  Tool          Mean(s)     StdDev        Min        Max
  ----------------------------------------------------
  grep           0.0022     0.0003 .001847703 .002556794
  ripgrep        0.2690     0.0031 .264772272 .272005812
  MCP            5.2374     0.1002 5.110520890 5.355461882


## json format - 100000 lines (19M)

[0;34m[INFO][0m FAIR Benchmark: Filter & Count (status >= 400) - json 100000
[0;34m[INFO][0m   jq (count)...
[0;34m[INFO][0m   grep (count)...
[0;34m[INFO][0m   MCP (count)...

  Tool          Mean(s)     StdDev        Min        Max
  ----------------------------------------------------
  jq             0.3126     0.0140 .296530463 .330573092
  grep           0.0018     0.0001 .001600543 .001914167
  MCP            0.0248     0.0005 .024147038 .025167409

[0;34m[INFO][0m FAIR Benchmark: Group by (top 50) - json 100000
[0;34m[INFO][0m   jq (group by service, top 50)...
[0;34m[INFO][0m   MCP (group by service, top 50)...

  Tool          Mean(s)     StdDev        Min        Max
  ----------------------------------------------------
  jq             0.1584     0.0015 .157148676 .160530127
  MCP            0.0401     0.0006 .039274107 .040748595


## json format - 1M lines (182M)

[0;34m[INFO][0m FAIR Benchmark: Filter & Count (status >= 400) - json 1M
[0;34m[INFO][0m   jq (count)...
[0;34m[INFO][0m   grep (count)...
[0;34m[INFO][0m   MCP (count)...

  Tool          Mean(s)     StdDev        Min        Max
  ----------------------------------------------------
  jq             3.0264     0.0268 3.002637088 3.063789475
  grep           0.0019     0.0002 .001568843 .002153653
  MCP            0.0278     0.0022 .026039385 .030804083

[0;34m[INFO][0m FAIR Benchmark: Group by (top 50) - json 1M
[0;34m[INFO][0m   jq (group by service, top 50)...
[0;34m[INFO][0m   MCP (group by service, top 50)...

  Tool          Mean(s)     StdDev        Min        Max
  ----------------------------------------------------
  jq             1.4974     0.0126 1.481696923 1.512616481
  MCP            0.1490     0.0035 .144097064 .151694542


## json format - 5M lines (907M)

[0;34m[INFO][0m FAIR Benchmark: Filter & Count (status >= 400) - json 5M
[0;34m[INFO][0m   jq (count)...
[0;34m[INFO][0m   grep (count)...
[0;34m[INFO][0m   MCP (count)...

  Tool          Mean(s)     StdDev        Min        Max
  ----------------------------------------------------
  jq            14.1932     0.1631 13.962952448 14.319889840
  grep           0.0022     0.0004 .001654173 .002745076
  MCP            0.0264     0.0013 .024604099 .027595965

[0;34m[INFO][0m FAIR Benchmark: Group by (top 50) - json 5M
[0;34m[INFO][0m   jq (group by service, top 50)...
[0;34m[INFO][0m   MCP (group by service, top 50)...

  Tool          Mean(s)     StdDev        Min        Max
  ----------------------------------------------------
  jq             7.9201     0.0437 7.878771395 7.980623883
  MCP            0.5359     0.0011 .534380611 .536755867


## syslog format - 100000 lines (7,5M)

[0;34m[INFO][0m FAIR Benchmark: Filter & Count (status >= 400) - syslog 100000
[0;34m[INFO][0m   grep (count)...
[0;34m[INFO][0m   MCP (count)...

  Tool          Mean(s)     StdDev        Min        Max
  ----------------------------------------------------
  grep           0.0018     0.0002 .001541984 .001995418
  MCP            0.1063     0.0012 .104647620 .107544438

[0;34m[INFO][0m FAIR Benchmark: Group by (top 50) - syslog 100000
[0;34m[INFO][0m   awk (group by hostname, top 50)...
[0;34m[INFO][0m   MCP (group by hostname, top 50)...

  Tool          Mean(s)     StdDev        Min        Max
  ----------------------------------------------------
  awk            0.0785     0.0032 .074336796 .082100674
  MCP            0.0316     0.0022 .028555603 .033662029


## syslog format - 1M lines (75M)

[0;34m[INFO][0m FAIR Benchmark: Filter & Count (status >= 400) - syslog 1M
[0;34m[INFO][0m   grep (count)...
[0;34m[INFO][0m   MCP (count)...

  Tool          Mean(s)     StdDev        Min        Max
  ----------------------------------------------------
  grep           0.0018     0.0003 .001607235 .002219035
  MCP            0.7906     0.0059 .784130436 .798288068

[0;34m[INFO][0m FAIR Benchmark: Group by (top 50) - syslog 1M
[0;34m[INFO][0m   awk (group by hostname, top 50)...
[0;34m[INFO][0m   MCP (group by hostname, top 50)...

  Tool          Mean(s)     StdDev        Min        Max
  ----------------------------------------------------
  awk            0.7493     0.0070 .739505357 .755896482
  MCP            0.0402     0.0007 .039139776 .040748184


## Methodology Notes

- **Filter benchmarks**: All tools count matching lines (not return rows)
- **Group benchmarks**: All tools return top 50 results
- **Regex benchmarks**: All tools count matching lines
- **Statistics**: Mean and standard deviation from 3 runs
- **Warmup**: 1 runs before measurement to warm caches
