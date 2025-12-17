use chrono::{DateTime, Duration, Utc};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use rand::prelude::*;
use serde::Serialize;
use std::fs::File;
use std::io::{BufWriter, Write};

#[derive(Parser)]
#[command(name = "generate_logs")]
#[command(about = "Generate realistic log files for benchmarking")]
struct Args {
    /// Output file path
    #[arg(short, long)]
    output: String,

    /// Number of log lines to generate
    #[arg(short, long, default_value = "1000000")]
    lines: usize,

    /// Log format: apache, json, syslog
    #[arg(short, long, default_value = "apache")]
    format: String,

    /// Error rate (0.0 - 1.0)
    #[arg(short, long, default_value = "0.05")]
    error_rate: f64,
}

// Realistic data pools
const IPS: &[&str] = &[
    "192.168.1.100", "192.168.1.101", "192.168.1.102", "192.168.1.103",
    "10.0.0.50", "10.0.0.51", "10.0.0.52", "10.0.0.53",
    "172.16.0.10", "172.16.0.11", "172.16.0.12",
    "203.0.113.50", "203.0.113.51", // Suspicious IPs for attacks
];

const PATHS: &[&str] = &[
    "/", "/index.html", "/about", "/contact", "/products", "/api/users",
    "/api/products", "/api/orders", "/api/checkout", "/api/login", "/api/logout",
    "/static/css/style.css", "/static/js/app.js", "/static/img/logo.png",
    "/admin", "/admin/dashboard", "/health", "/metrics", "/favicon.ico",
];

const METHODS: &[&str] = &["GET", "GET", "GET", "GET", "POST", "PUT", "DELETE"];

const USER_AGENTS: &[&str] = &[
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36",
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36",
    "Mozilla/5.0 (iPhone; CPU iPhone OS 14_0 like Mac OS X)",
    "curl/7.68.0",
    "PostmanRuntime/7.28.4",
    "python-requests/2.25.1",
];

const SERVICES: &[&str] = &[
    "api-gateway", "user-service", "payment-service", "order-service",
    "notification-service", "cache-service", "auth-service",
];

const LOG_LEVELS: &[&str] = &["DEBUG", "INFO", "INFO", "INFO", "WARN", "ERROR"];

const ERROR_MESSAGES: &[&str] = &[
    "Database connection timeout",
    "Connection refused",
    "Out of memory",
    "Disk space low",
    "Authentication failed",
    "Rate limit exceeded",
    "Service unavailable",
    "Invalid request",
];

const HOSTNAMES: &[&str] = &[
    "webserver01", "webserver02", "appserver01", "appserver02",
    "dbserver01", "cacheserver01", "loadbalancer",
];

const PROCESSES: &[&str] = &[
    "nginx", "sshd", "mysqld", "redis-server", "app", "haproxy", "kernel",
];

#[derive(Serialize)]
struct JsonLog {
    timestamp: String,
    level: String,
    service: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration_ms: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<u16>,
}

fn main() -> std::io::Result<()> {
    let args = Args::parse();

    let file = File::create(&args.output)?;
    let mut writer = BufWriter::with_capacity(1024 * 1024, file); // 1MB buffer

    let mut rng = rand::thread_rng();
    let start_time = Utc::now() - Duration::days(1);

    let pb = ProgressBar::new(args.lines as u64);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
        .unwrap()
        .progress_chars("#>-"));

    for i in 0..args.lines {
        let timestamp = start_time + Duration::milliseconds((i as i64) * 50 + rng.gen_range(0..50));
        let is_error = rng.r#gen::<f64>() < args.error_rate;

        let line = match args.format.as_str() {
            "apache" => generate_apache_log(&mut rng, timestamp, is_error),
            "json" => generate_json_log(&mut rng, timestamp, is_error),
            "syslog" => generate_syslog(&mut rng, timestamp, is_error),
            _ => panic!("Unknown format: {}", args.format),
        };

        writeln!(writer, "{}", line)?;

        if i % 10000 == 0 {
            pb.set_position(i as u64);
        }
    }

    pb.finish_with_message("Done!");
    writer.flush()?;

    // Print file size
    let metadata = std::fs::metadata(&args.output)?;
    let size_mb = metadata.len() as f64 / (1024.0 * 1024.0);
    println!("Generated {} lines ({:.2} MB) to {}", args.lines, size_mb, args.output);

    Ok(())
}

fn generate_apache_log(rng: &mut ThreadRng, timestamp: DateTime<Utc>, is_error: bool) -> String {
    let ip = IPS.choose(rng).unwrap();
    let method = METHODS.choose(rng).unwrap();
    let path = PATHS.choose(rng).unwrap();
    let user_agent = USER_AGENTS.choose(rng).unwrap();

    let status = if is_error {
        *[400, 401, 403, 404, 500, 502, 503].choose(rng).unwrap()
    } else {
        *[200, 200, 200, 200, 201, 204, 301, 304].choose(rng).unwrap()
    };

    let size = if status >= 400 { rng.gen_range(50..200) } else { rng.gen_range(500..50000) };
    let ts = timestamp.format("%d/%b/%Y:%H:%M:%S %z");

    format!(
        "{} - - [{}] \"{} {} HTTP/1.1\" {} {} \"-\" \"{}\"",
        ip, ts, method, path, status, size, user_agent
    )
}

fn generate_json_log(rng: &mut ThreadRng, timestamp: DateTime<Utc>, is_error: bool) -> String {
    let service = SERVICES.choose(rng).unwrap();
    let level = if is_error {
        "ERROR"
    } else {
        LOG_LEVELS.choose(rng).unwrap()
    };

    let (message, error, duration, path, status) = if is_error {
        let err_msg = ERROR_MESSAGES.choose(rng).unwrap();
        (err_msg.to_string(), Some(err_msg.to_string()), None, None, None)
    } else {
        let path = PATHS.choose(rng).unwrap();
        (
            format!("Request processed for {}", path),
            None,
            Some(rng.gen_range(10..500)),
            Some(path.to_string()),
            Some(*[200u16, 200, 200, 201, 204].choose(rng).unwrap()),
        )
    };

    let log = JsonLog {
        timestamp: timestamp.to_rfc3339(),
        level: level.to_string(),
        service: service.to_string(),
        message,
        error,
        duration_ms: duration,
        path,
        status,
    };

    serde_json::to_string(&log).unwrap()
}

fn generate_syslog(rng: &mut ThreadRng, timestamp: DateTime<Utc>, is_error: bool) -> String {
    let hostname = HOSTNAMES.choose(rng).unwrap();
    let process = PROCESSES.choose(rng).unwrap();
    let pid = rng.gen_range(1000..50000);
    let ts = timestamp.format("%b %d %H:%M:%S");

    let message = if is_error {
        let err = ERROR_MESSAGES.choose(rng).unwrap();
        format!("ERROR {}", err)
    } else {
        match *process {
            "sshd" => format!("Accepted publickey for user{} from {} port {}",
                rng.gen_range(1..10), IPS.choose(rng).unwrap(), rng.gen_range(40000..60000)),
            "nginx" => format!("*{} upstream response time: {}ms",
                rng.gen_range(1000..9999), rng.gen_range(10..500)),
            "mysqld" => format!("Query executed in {}ms", rng.gen_range(1..100)),
            _ => format!("Operation completed successfully"),
        }
    };

    format!("{} {} {}[{}]: {}", ts, hostname, process, pid, message)
}
