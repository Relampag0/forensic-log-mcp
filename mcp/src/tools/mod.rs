use rmcp::{ServerHandler, model::*, tool, tool_router, handler::server::tool::ToolRouter};
use rmcp::handler::server::tool::ToolCallContext;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::service::{RequestContext, RoleServer};
use rmcp::ErrorData as McpError;
use serde::Deserialize;
use schemars::JsonSchema;
use std::future::Future;

use crate::parsers::{self, LogFormat, apache_simd, syslog_simd};
use crate::engine::{QueryBuilder, query::dataframe_to_json, query::get_schema_info};

#[derive(Clone)]
pub struct LogForensicsServer {
    tool_router: ToolRouter<Self>,
}

// Tool parameter structures
#[derive(Debug, Deserialize, JsonSchema)]
pub struct AnalyzeLogsParams {
    /// Path to log file, directory, or glob pattern (e.g., "/var/log/nginx/*.log")
    pub path: String,
    /// Log format: "auto", "apache", "nginx", "syslog", "json", "csv"
    #[serde(default = "default_format")]
    pub format: String,
    /// Filter by status code (e.g., ">=400", "500", "4xx")
    pub filter_status: Option<String>,
    /// Filter by text/regex pattern in message or request
    pub filter_text: Option<String>,
    /// Filter by start time (ISO format or log-native format)
    pub filter_time_start: Option<String>,
    /// Filter by end time (ISO format or log-native format)
    pub filter_time_end: Option<String>,
    /// Column to group results by
    pub group_by: Option<String>,
    /// Column to sort results by
    pub sort_by: Option<String>,
    /// Sort in descending order
    #[serde(default = "default_true")]
    pub sort_desc: bool,
    /// Maximum number of rows to return (default 50)
    #[serde(default = "default_limit")]
    pub limit: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetSchemaParams {
    /// Path to log file
    pub path: String,
    /// Log format hint: "auto", "apache", "nginx", "syslog", "json", "csv"
    #[serde(default = "default_format")]
    pub format: String,
    /// Number of sample rows to include (default 5)
    #[serde(default = "default_sample")]
    pub sample_rows: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AggregateLogsParams {
    /// Path to log file, directory, or glob pattern
    pub path: String,
    /// Aggregation operation: "count", "sum", "avg", "min", "max", "unique"
    pub operation: String,
    /// Column to aggregate (required for sum/avg/min/max)
    pub column: Option<String>,
    /// Column to group by
    pub group_by: Option<String>,
    /// Filter by text pattern
    pub filter_text: Option<String>,
    /// Log format
    #[serde(default = "default_format")]
    pub format: String,
    /// Maximum rows to return
    #[serde(default = "default_limit")]
    pub limit: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchPatternParams {
    /// Path to log file, directory, or glob pattern
    pub path: String,
    /// Regex pattern to search for
    pub pattern: String,
    /// Column to search in (searches all text columns if not specified)
    pub column: Option<String>,
    /// Case sensitive search (default false)
    #[serde(default)]
    pub case_sensitive: bool,
    /// Log format
    #[serde(default = "default_format")]
    pub format: String,
    /// Maximum rows to return
    #[serde(default = "default_limit")]
    pub limit: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TimeAnalysisParams {
    /// Path to log file, directory, or glob pattern
    pub path: String,
    /// Time bucket: "minute", "hour", "day"
    pub bucket: String,
    /// Time column to use (auto-detected if not specified)
    pub time_column: Option<String>,
    /// What to count per bucket
    pub count_column: Option<String>,
    /// Filter by text pattern
    pub filter_text: Option<String>,
    /// Log format
    #[serde(default = "default_format")]
    pub format: String,
    /// Maximum buckets to return
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_format() -> String { "auto".to_string() }
fn default_limit() -> u32 { 50 }
fn default_sample() -> u32 { 5 }
fn default_true() -> bool { true }

#[tool_router]
impl LogForensicsServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "Analyze log files with filtering, grouping, and sorting. Supports massive files via streaming. Use this to find specific errors, filter by status codes, group by IP/path, etc.")]
    async fn analyze_logs(&self, Parameters(params): Parameters<AnalyzeLogsParams>) -> Result<CallToolResult, McpError> {
        let format = LogFormat::from_str(&params.format);
        let path = std::path::Path::new(&params.path);

        // GENERALIZED FAST PATH: Apache/Nginx with SIMD acceleration
        let is_apache = matches!(format, LogFormat::Apache | LogFormat::Nginx)
            || (format == LogFormat::Auto && params.path.contains("access"));

        // Fast path works for:
        // - Any status filter (>=400, =200, 4xx, etc.)
        // - Time range filters (start and/or end)
        // - Optional text filter
        // - Single files or can expand globs
        // Does NOT work for: grouping (use aggregate_logs for that)
        if is_apache && params.group_by.is_none() {
            // Parse status filter if provided
            let status_filter = params.filter_status.as_ref()
                .and_then(|s| apache_simd::StatusFilter::parse(s));

            // Parse time filter if provided
            let time_filter = apache_simd::TimeFilter::new(
                params.filter_time_start.as_deref(),
                params.filter_time_end.as_deref(),
            );

            // Convert text filter to bytes
            let text_pattern = params.filter_text.as_ref().map(|s| s.as_bytes());

            // Handle glob patterns
            let paths = match parsers::expand_glob(&params.path) {
                Ok(p) => p,
                Err(_) => vec![path.to_path_buf()],
            };

            // Use fast path for single file
            if paths.len() == 1 && paths[0].is_file() {
                match apache_simd::filter_lines(
                    &paths[0],
                    status_filter,
                    time_filter,
                    text_pattern,
                    params.limit as usize,
                ) {
                    Ok((count, lines)) => {
                        let json = serde_json::to_string(&lines).unwrap_or_default();
                        let filter_desc = match (&params.filter_status, &params.filter_text) {
                            (Some(s), Some(t)) => format!("status {} and text '{}'", s, t),
                            (Some(s), None) => format!("status {}", s),
                            (None, Some(t)) => format!("text '{}'", t),
                            (None, None) => "all".to_string(),
                        };
                        let summary = format!(
                            "Found {} rows matching {} (showing {})\n\nData:\n{}",
                            count, filter_desc, lines.len(), json
                        );
                        return Ok(CallToolResult::success(vec![Content::text(summary)]));
                    }
                    Err(e) => {
                        tracing::warn!("SIMD fast path failed, using regular parser: {}", e);
                    }
                }
            } else if paths.len() > 1 {
                // Multi-file: just count for now (can extend later)
                if let Some(filter) = status_filter {
                    let path_refs: Vec<&std::path::Path> = paths.iter().map(|p| p.as_path()).collect();
                    match apache_simd::count_status_multi(&path_refs, filter) {
                        Ok(count) => {
                            let summary = format!(
                                "Found {} rows matching status {} across {} files",
                                count,
                                params.filter_status.as_ref().unwrap(),
                                paths.len()
                            );
                            return Ok(CallToolResult::success(vec![Content::text(summary)]));
                        }
                        Err(e) => {
                            tracing::warn!("Multi-file fast path failed: {}", e);
                        }
                    }
                }
            }
        }

        // SYSLOG FAST PATH
        let is_syslog = matches!(format, LogFormat::Syslog)
            || (format == LogFormat::Auto && (params.path.contains("syslog") || params.path.contains("messages")));

        if is_syslog
            && params.filter_status.is_none()
            && params.filter_time_start.is_none()
            && params.filter_time_end.is_none()
            && params.group_by.is_none()
            && path.is_file()
        {
            let text_pattern = params.filter_text.as_ref().map(|s| s.as_bytes());

            match syslog_simd::filter_lines(path, text_pattern, params.limit as usize) {
                Ok((count, lines)) => {
                    let json = serde_json::to_string(&lines).unwrap_or_default();
                    let filter_desc = params.filter_text.as_ref()
                        .map(|t| format!("text '{}'", t))
                        .unwrap_or_else(|| "all".to_string());
                    let summary = format!(
                        "Found {} rows matching {} (SIMD fast path, showing {})\n\nData:\n{}",
                        count, filter_desc, lines.len(), json
                    );
                    return Ok(CallToolResult::success(vec![Content::text(summary)]));
                }
                Err(e) => {
                    tracing::warn!("Syslog SIMD fast path failed: {}", e);
                }
            }
        }

        // REGULAR PATH
        let lf = match parsers::parse_multiple(&params.path, format) {
            Ok(lf) => lf,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!("Error parsing logs: {}", e))])),
        };

        let mut qb = QueryBuilder::new(lf);

        // Apply filters
        if let Some(status_filter) = &params.filter_status {
            qb = qb.filter_status(status_filter);
        }

        if let Some(text_filter) = &params.filter_text {
            // Use appropriate column based on format
            let text_col = match format {
                LogFormat::Json => "message",
                LogFormat::Csv => "message",  // CSV may vary, message is common
                _ => "raw",  // Apache/Nginx/Syslog have raw line
            };
            qb = qb.filter_text(text_col, text_filter, false);
        }

        if let Some(start) = &params.filter_time_start {
            qb = qb.filter_time_range("timestamp", Some(start), None);
        }
        if let Some(end) = &params.filter_time_end {
            qb = qb.filter_time_range("timestamp", None, Some(end));
        }

        // Group or sort
        if let Some(group_col) = &params.group_by {
            qb = qb.group_by(group_col).count();
        } else if let Some(sort_col) = &params.sort_by {
            qb = qb.sort(sort_col, params.sort_desc);
        }

        qb = qb.limit(params.limit);

        match qb.collect() {
            Ok(df) => {
                let json = dataframe_to_json(&df).unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e));
                let row_count = df.height();
                let summary = format!("Found {} rows matching criteria.\n\nData:\n{}", row_count, json);
                Ok(CallToolResult::success(vec![Content::text(summary)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!("Query error: {}", e))])),
        }
    }

    #[tool(description = "Get the schema and sample data from a log file. Use this first to understand what columns are available before querying.")]
    async fn get_log_schema(&self, Parameters(params): Parameters<GetSchemaParams>) -> Result<CallToolResult, McpError> {
        let format = LogFormat::from_str(&params.format);
        let path = std::path::Path::new(&params.path);

        let lf = match parsers::parse_logs(path, format) {
            Ok(lf) => lf,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!("Error parsing logs: {}", e))])),
        };

        match lf.limit(params.sample_rows).collect() {
            Ok(df) => {
                let schema_info = get_schema_info(&df);
                let mut output = String::from("## Schema\n\n| Column | Type |\n|--------|------|\n");
                for (name, dtype) in &schema_info {
                    output.push_str(&format!("| {} | {} |\n", name, dtype));
                }

                output.push_str(&format!("\n## Sample Data ({} rows)\n\n", df.height()));
                let json = dataframe_to_json(&df).unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e));
                output.push_str(&json);

                Ok(CallToolResult::success(vec![Content::text(output)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!("Error: {}", e))])),
        }
    }

    #[tool(description = "Perform aggregations on log data: count, sum, avg, min, max, or unique counts. Group by any column for breakdowns.")]
    async fn aggregate_logs(&self, Parameters(params): Parameters<AggregateLogsParams>) -> Result<CallToolResult, McpError> {
        let format = LogFormat::from_str(&params.format);
        let path = std::path::Path::new(&params.path);

        // GENERALIZED FAST PATH: Apache/Nginx count grouped by ip/path/method/status
        let is_apache = matches!(format, LogFormat::Apache | LogFormat::Nginx)
            || (format == LogFormat::Auto && params.path.contains("access"));

        let op_lower = params.operation.to_lowercase();

        if is_apache {
            // Check if we can use fast path for this group_by column
            let group_col = params.group_by.as_ref()
                .and_then(|s| apache_simd::GroupByColumn::parse(s));

            // FAST PATH for count operations
            if op_lower == "count" {
                if let Some(column) = group_col {
                    let text_pattern = params.filter_text.as_ref().map(|s| s.as_bytes());

                    let paths = match parsers::expand_glob(&params.path) {
                        Ok(p) => p,
                        Err(_) => vec![path.to_path_buf()],
                    };

                    let result = if paths.len() == 1 && paths[0].is_file() {
                        apache_simd::group_by_count(&paths[0], column, None, text_pattern)
                    } else {
                        let path_refs: Vec<&std::path::Path> = paths.iter().map(|p| p.as_path()).collect();
                        apache_simd::group_by_count_multi(&path_refs, column, None, text_pattern)
                    };

                    match result {
                        Ok(results) => {
                            let col_name = params.group_by.as_ref().unwrap();
                            let limited: Vec<_> = results.into_iter().take(params.limit as usize).collect();
                            let json_data: Vec<serde_json::Value> = limited.iter()
                                .map(|(key, count)| serde_json::json!({col_name: key, "count": count}))
                                .collect();
                            let json = serde_json::to_string(&json_data).unwrap_or_default();
                            let summary = format!(
                                "Aggregation: count by {} (SIMD fast path, {} groups)\n\n{}",
                                col_name, json_data.len(), json
                            );
                            return Ok(CallToolResult::success(vec![Content::text(summary)]));
                        }
                        Err(e) => {
                            tracing::warn!("SIMD count fast path failed: {}", e);
                        }
                    }
                }
            }

// FAST PATH for sum/avg/min/max on size field
            if matches!(op_lower.as_str(), "sum" | "avg" | "min" | "max") {
                let text_pattern = params.filter_text.as_ref().map(|s| s.as_bytes());

                let paths = match parsers::expand_glob(&params.path) {
                    Ok(p) => p,
                    Err(_) => vec![path.to_path_buf()],
                };

                let result = if paths.len() == 1 && paths[0].is_file() {
                    apache_simd::aggregate_size(&paths[0], group_col, None, text_pattern)
                } else {
                    let path_refs: Vec<&std::path::Path> = paths.iter().map(|p| p.as_path()).collect();
                    apache_simd::aggregate_size_multi(&path_refs, group_col, None, text_pattern)
                };

                match result {
                    Ok(aggs) => {
                        let col_name = params.group_by.as_deref().unwrap_or("total");
                        let mut sorted: Vec<_> = aggs.into_iter().collect();
                        sorted.sort_by(|a, b| b.1.sum.cmp(&a.1.sum));
                        let limited: Vec<_> = sorted.into_iter().take(params.limit as usize).collect();

                        let json_data: Vec<serde_json::Value> = limited.iter()
                            .map(|(key, agg)| {
                                let value = match op_lower.as_str() {
                                    "sum" => serde_json::json!(agg.sum),
                                    "avg" => serde_json::json!(agg.avg()),
                                    "min" => serde_json::json!(if agg.min == i64::MAX { 0 } else { agg.min }),
                                    "max" => serde_json::json!(if agg.max == i64::MIN { 0 } else { agg.max }),
                                    _ => serde_json::json!(agg.sum),
                                };
                                serde_json::json!({col_name: key, &op_lower: value, "count": agg.count})
                            })
                            .collect();
                        let json = serde_json::to_string(&json_data).unwrap_or_default();
                        let summary = format!(
                            "Aggregation: {} of size by {} (SIMD fast path, {} groups)\n\n{}",
                            op_lower, col_name, json_data.len(), json
                        );
                        return Ok(CallToolResult::success(vec![Content::text(summary)]));
                    }
                    Err(e) => {
                        tracing::warn!("SIMD aggregate fast path failed: {}", e);
                    }
                }
            }
        }

        // SYSLOG SIMD FAST PATH for count grouped by hostname/process
        let is_syslog = matches!(format, LogFormat::Syslog)
            || (format == LogFormat::Auto && params.path.contains("syslog"));

        if is_syslog && op_lower == "count" && params.group_by.is_some() {
            let group_col = syslog_simd::SyslogGroupBy::parse(params.group_by.as_ref().unwrap());
            if let Some(column) = group_col {
                let text_pattern = params.filter_text.as_ref().map(|s| s.as_bytes());

                let paths = match parsers::expand_glob(&params.path) {
                    Ok(p) => p,
                    Err(_) => vec![path.to_path_buf()],
                };

                // Only use fast path for single files
                if paths.len() == 1 && paths[0].is_file() {
                    match syslog_simd::group_by_count(&paths[0], column, text_pattern) {
                        Ok(results) => {
                            let col_name = params.group_by.as_ref().unwrap();
                            let limited: Vec<_> = results.into_iter().take(params.limit as usize).collect();
                            let json_data: Vec<serde_json::Value> = limited.iter()
                                .map(|(key, count)| serde_json::json!({col_name: key, "count": count}))
                                .collect();
                            let json = serde_json::to_string(&json_data).unwrap_or_default();
                            let summary = format!(
                                "Aggregation: count by {} (SIMD fast path, {} groups)\n\n{}",
                                col_name, json_data.len(), json
                            );
                            return Ok(CallToolResult::success(vec![Content::text(summary)]));
                        }
                        Err(e) => {
                            tracing::warn!("Syslog SIMD count fast path failed: {}", e);
                        }
                    }
                }
            }
        }

        // REGULAR PATH
        let lf = match parsers::parse_multiple(&params.path, format) {
            Ok(lf) => lf,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!("Error parsing logs: {}", e))])),
        };

        let mut qb = QueryBuilder::new(lf);

        if let Some(text_filter) = &params.filter_text {
            qb = qb.filter_text("message", text_filter, false);
        }

        let result_qb = if let Some(group_col) = &params.group_by {
            let gb = qb.group_by(group_col);
            match params.operation.to_lowercase().as_str() {
                "count" => gb.count(),
                "sum" => {
                    let col = params.column.as_deref().unwrap_or("size");
                    gb.sum(col)
                }
                "avg" => {
                    let col = params.column.as_deref().unwrap_or("size");
                    gb.avg(col)
                }
                "min" => {
                    let col = params.column.as_deref().unwrap_or("size");
                    gb.min(col)
                }
                "max" => {
                    let col = params.column.as_deref().unwrap_or("size");
                    gb.max(col)
                }
                "unique" => {
                    let col = params.column.as_deref().unwrap_or("ip");
                    gb.unique_count(col)
                }
                _ => return Ok(CallToolResult::error(vec![Content::text(format!("Unknown operation: {}", params.operation))])),
            }
        } else {
            return Ok(CallToolResult::error(vec![Content::text("group_by is required for aggregations".to_string())]));
        };

        match result_qb.limit(params.limit).collect() {
            Ok(df) => {
                let json = dataframe_to_json(&df).unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e));
                let summary = format!("Aggregation: {} by {}\n\n{}",
                    params.operation,
                    params.group_by.as_deref().unwrap_or("all"),
                    json
                );
                Ok(CallToolResult::success(vec![Content::text(summary)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!("Query error: {}", e))])),
        }
    }

    #[tool(description = "Search for regex patterns in log files. Returns matching rows with full context.")]
    async fn search_pattern(&self, Parameters(params): Parameters<SearchPatternParams>) -> Result<CallToolResult, McpError> {
        let format = LogFormat::from_str(&params.format);
        let path = std::path::Path::new(&params.path);

        // SIMD FAST PATH for Apache/Nginx
        let is_apache = matches!(format, LogFormat::Apache | LogFormat::Nginx)
            || (format == LogFormat::Auto && params.path.contains("access"));

        if is_apache && path.is_file() {
            match apache_simd::regex_search(path, &params.pattern, None, params.limit as usize) {
                Ok((count, lines)) => {
                    let json = serde_json::to_string(&lines).unwrap_or_default();
                    let summary = format!(
                        "Found {} matches for pattern '{}' (SIMD fast path)\n\nData:\n{}",
                        count, params.pattern, json
                    );
                    return Ok(CallToolResult::success(vec![Content::text(summary)]));
                }
                Err(e) => {
                    tracing::warn!("SIMD regex search failed: {}", e);
                }
            }
        }

        // SIMD FAST PATH for Syslog
        let is_syslog = matches!(format, LogFormat::Syslog)
            || (format == LogFormat::Auto && (params.path.contains("syslog") || params.path.contains("messages")));

        if is_syslog && path.is_file() {
            match syslog_simd::regex_search(path, &params.pattern, params.limit as usize) {
                Ok((count, lines)) => {
                    let json = serde_json::to_string(&lines).unwrap_or_default();
                    let summary = format!(
                        "Found {} matches for pattern '{}' (SIMD fast path)\n\nData:\n{}",
                        count, params.pattern, json
                    );
                    return Ok(CallToolResult::success(vec![Content::text(summary)]));
                }
                Err(e) => {
                    tracing::warn!("SIMD syslog regex search failed: {}", e);
                }
            }
        }

        // REGULAR PATH (Polars)
        let lf = match parsers::parse_multiple(&params.path, format) {
            Ok(lf) => lf,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!("Error parsing logs: {}", e))])),
        };

        // Use appropriate default column based on format
        let default_col = match format {
            LogFormat::Json => "message",
            LogFormat::Csv => "message",
            _ => "raw",
        };
        let search_col = params.column.as_deref().unwrap_or(default_col);
        let qb = QueryBuilder::new(lf)
            .filter_regex(search_col, &params.pattern)
            .limit(params.limit);

        match qb.collect() {
            Ok(df) => {
                let json = dataframe_to_json(&df).unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e));
                let summary = format!("Found {} matches for pattern '{}'\n\n{}",
                    df.height(),
                    params.pattern,
                    json
                );
                Ok(CallToolResult::success(vec![Content::text(summary)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!("Query error: {}", e))])),
        }
    }

    #[tool(description = "Analyze logs over time. Bucket by minute, hour, or day to identify trends and spikes.")]
    async fn time_analysis(&self, Parameters(params): Parameters<TimeAnalysisParams>) -> Result<CallToolResult, McpError> {
        let format = LogFormat::from_str(&params.format);

        let lf = match parsers::parse_multiple(&params.path, format) {
            Ok(lf) => lf,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(format!("Error parsing logs: {}", e))])),
        };

        let time_col = params.time_column.as_deref().unwrap_or("timestamp");

        let qb = QueryBuilder::new(lf).group_by(time_col).count().limit(params.limit);

        match qb.collect() {
            Ok(df) => {
                let json = dataframe_to_json(&df).unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e));
                let summary = format!("Time analysis by {} ({})\n\n{}",
                    time_col,
                    params.bucket,
                    json
                );
                Ok(CallToolResult::success(vec![Content::text(summary)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!("Query error: {}", e))])),
        }
    }
}

impl ServerHandler for LogForensicsServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "High-performance log analysis server powered by Polars. Use get_log_schema first \
                 to understand available columns, then use analyze_logs, aggregate_logs, \
                 search_pattern, or time_analysis to query the data. Supports Apache, Nginx, \
                 Syslog, JSON, and CSV log formats. Can handle files larger than RAM via streaming.".to_string()
            ),
        }
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        async move {
            Ok(ListToolsResult {
                tools: self.tool_router.list_all(),
                next_cursor: None,
                meta: None,
            })
        }
    }

    fn call_tool(
        &self,
        request: CallToolRequestParam,
        context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<CallToolResult, McpError>> + Send + '_ {
        let ctx = ToolCallContext::new(self, request, context);
        self.tool_router.call(ctx)
    }
}
