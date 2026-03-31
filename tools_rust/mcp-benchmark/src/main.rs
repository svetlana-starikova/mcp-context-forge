// -*- coding: utf-8 -*-
/**
 * MCP Tools Benchmark - Tokio-based Load Test Script
 *
 * Equivalent to the k6 and Locust MCP tools benchmarks.
 * Tests ONLY the MCP Streamable HTTP transport path through the gateway.
 *
 * Test Scenario:
 *   - Heavy tool/call in tight loop (20:1 ratio vs tools/list)
 *   - Discovers tools via MCP tools/list during initialization
 *   - Calls tools/call rapidly with proper arguments
 *
 * Usage:
 *   # Quick test (10 users, 10s)
 *   cargo run --release -- --profile quick
 *
 *   # Standard test (50 users, 30s)
 *   cargo run --release -- --profile standard
 *
 *   # Heavy test (125 users, 60s)
 *   cargo run --release -- --profile heavy
 *
 *   # Custom configuration
 *   cargo run --release -- --users 100 --run-time 120 --host http://localhost:8080
 *
 * Environment Variables:
 *   MCP_BENCHMARK_HOST       - Gateway URL (default: http://localhost:8080)
 *   MCP_BENCHMARK_SERVER_ID  - Virtual server UUID (required)
 *   MCP_BENCHMARK_TOOL_NAMES - Comma-separated tool names (optional, auto-detected)
 *   JWT_SECRET_KEY           - JWT signing secret (default: my-test-key-but-now-longer-than-32-bytes)
 *
 * Copyright 2025
 * SPDX-License-Identifier: Apache-2.0
 */

use anyhow::Result;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{Duration, Utc};
use clap::Parser;
use hmac::{Hmac, Mac};
use once_cell::sync::Lazy;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::{
    env,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, RwLock,
    },
    time::{Duration as StdDuration, Instant},
};
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

static CONFIG: Lazy<Config> = Lazy::new(|| Config::from_env());
static SHARED_STATE: Lazy<Arc<SharedState>> = Lazy::new(|| Arc::new(SharedState::new()));
static METRICS: Lazy<Arc<Metrics>> = Lazy::new(|| Arc::new(Metrics::new()));
static TEST_START_TIME: Lazy<Arc<RwLock<Option<Instant>>>> = Lazy::new(|| Arc::new(RwLock::new(None)));

#[derive(Parser, Debug, Clone)]
#[command(name = "mcp-benchmark")]
#[command(about = "MCP Tools Benchmark using Tokio async runtime")]
struct CliArgs {
    #[arg(long, default_value = "standard")]
    profile: String,
    #[arg(long)]
    users: Option<usize>,
    #[arg(long)]
    run_time: Option<u64>,
    #[arg(long)]
    host: Option<String>,
    #[arg(long)]
    server_id: Option<String>,
    #[arg(long)]
    tool_names: Option<String>,
}

#[derive(Debug, Clone)]
struct Config {
    profile: String,
    users: usize,
    run_time_secs: u64,
    host: String,
    server_id: String,
    tool_names_str: String,
    jwt_secret_key: String,
    jwt_algorithm: String,
    jwt_audience: String,
    jwt_issuer: String,
    jwt_username: String,
}

impl Config {
    fn from_env() -> Self {
        let _ = dotenvy::dotenv();
        let args = CliArgs::parse();

        let profile = env::var("MCP_BENCHMARK_PROFILE")
            .or_else(|_| env::var("MCP_BENCHMARK_PROFILE"))
            .unwrap_or_else(|_| args.profile.clone());
        
        let (default_users, default_run_time) = match profile.as_str() {
            "quick" => (10, 10),
            "standard" => (50, 30),
            "heavy" => (125, 60),
            _ => (50, 30),
        };

        let users = args.users.or(env::var("MCP_BENCHMARK_USERS").ok().and_then(|s| s.parse().ok())).unwrap_or(default_users);
        let run_time_secs = args.run_time.or(env::var("MCP_BENCHMARK_RUN_TIME").ok().and_then(|s| s.parse().ok())).unwrap_or(default_run_time);

        Config {
            profile: args.profile,
            users,
            run_time_secs,
            host: args.host
                .or_else(|| env::var("MCP_BENCHMARK_HOST").ok())
                .or_else(|| env::var("K6_MCP_HOST").ok())
                .unwrap_or_else(|| "http://localhost:8080".to_string()),
            server_id: args.server_id
                .or_else(|| env::var("MCP_BENCHMARK_SERVER_ID").ok())
                .or_else(|| env::var("K6_MCP_SERVER_ID").ok())
                .unwrap_or_default(),
            tool_names_str: args.tool_names
                .or_else(|| env::var("MCP_BENCHMARK_TOOL_NAMES").ok())
                .or_else(|| env::var("K6_MCP_TOOL_NAMES").ok())
                .unwrap_or_default(),
            jwt_secret_key: env::var("JWT_SECRET_KEY")
                .or_else(|_| env::var("K6_JWT_SECRET_KEY"))
                .unwrap_or_else(|_| "my-test-key-but-now-longer-than-32-bytes".to_string()),
            jwt_algorithm: env::var("JWT_ALGORITHM").unwrap_or_else(|_| "HS256".to_string()),
            jwt_audience: env::var("JWT_AUDIENCE").unwrap_or_else(|_| "mcpgateway-api".to_string()),
            jwt_issuer: env::var("JWT_ISSUER").unwrap_or_else(|_| "mcpgateway".to_string()),
            jwt_username: env::var("JWT_USERNAME").unwrap_or_else(|_| "admin@example.com".to_string()),
        }
    }
}

#[derive(Debug, Clone)]
struct ServerTarget {
    server_id: String,
    server_name: String,
    tool_names: Vec<String>,
}

struct SharedState {
    server_targets: RwLock<Vec<ServerTarget>>,
    jwt_token: RwLock<Option<String>>,
    initialized: AtomicBool,
}

impl SharedState {
    fn new() -> Self {
        SharedState {
            server_targets: RwLock::new(Vec::new()),
            jwt_token: RwLock::new(None),
            initialized: AtomicBool::new(false),
        }
    }

    fn get_token(&self) -> String {
        if let Some(token) = self.jwt_token.read().unwrap().clone() {
            return token;
        }
        let token = generate_jwt_token();
        *self.jwt_token.write().unwrap() = Some(token.clone());
        token
    }

    fn get_server_targets(&self) -> Vec<ServerTarget> {
        self.server_targets.read().unwrap().clone()
    }

    fn set_initialized(&self) {
        self.initialized.store(true, Ordering::SeqCst);
    }

    fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::SeqCst)
    }
}

struct Metrics {
    tool_call_requests: AtomicUsize,
    tool_list_requests: AtomicUsize,
    tool_call_success: AtomicUsize,
    tool_call_failure: AtomicUsize,
    tool_list_success: AtomicUsize,
    tool_list_failure: AtomicUsize,
    tool_call_durations: RwLock<Vec<f64>>,
    tool_list_durations: RwLock<Vec<f64>>,
}

impl Metrics {
    fn new() -> Self {
        Metrics {
            tool_call_requests: AtomicUsize::new(0),
            tool_list_requests: AtomicUsize::new(0),
            tool_call_success: AtomicUsize::new(0),
            tool_call_failure: AtomicUsize::new(0),
            tool_list_success: AtomicUsize::new(0),
            tool_list_failure: AtomicUsize::new(0),
            tool_call_durations: RwLock::new(Vec::new()),
            tool_list_durations: RwLock::new(Vec::new()),
        }
    }

    fn record_tool_call(&self, duration_ms: f64, success: bool) {
        self.tool_call_requests.fetch_add(1, Ordering::SeqCst);
        if success {
            self.tool_call_success.fetch_add(1, Ordering::SeqCst);
        } else {
            self.tool_call_failure.fetch_add(1, Ordering::SeqCst);
        }
        self.tool_call_durations.write().unwrap().push(duration_ms);
    }

    fn record_tool_list(&self, duration_ms: f64, success: bool) {
        self.tool_list_requests.fetch_add(1, Ordering::SeqCst);
        if success {
            self.tool_list_success.fetch_add(1, Ordering::SeqCst);
        } else {
            self.tool_list_failure.fetch_add(1, Ordering::SeqCst);
        }
        self.tool_list_durations.write().unwrap().push(duration_ms);
    }

    fn print_summary(&self) {
        let tool_call_count = self.tool_call_requests.load(Ordering::SeqCst);
        let tool_list_count = self.tool_list_requests.load(Ordering::SeqCst);
        let total_count = tool_call_count + tool_list_count;

        let test_duration = TEST_START_TIME
            .read()
            .unwrap()
            .map(|start| start.elapsed().as_secs_f64())
            .unwrap_or(60.0);

        let overall_rps = total_count as f64 / test_duration;
        let tool_call_rps = tool_call_count as f64 / test_duration;
        let tool_list_rps = tool_list_count as f64 / test_duration;

        let tool_call_success_rate = if tool_call_count > 0 {
            self.tool_call_success.load(Ordering::SeqCst) as f64 / tool_call_count as f64 * 100.0
        } else {
            0.0
        };

        let tool_list_success_rate = if tool_list_count > 0 {
            self.tool_list_success.load(Ordering::SeqCst) as f64 / tool_list_count as f64 * 100.0
        } else {
            0.0
        };

        let tool_call_durations = self.tool_call_durations.read().unwrap();
        let tool_list_durations = self.tool_list_durations.read().unwrap();

        let tool_call_stats = calculate_duration_stats(&tool_call_durations);
        let tool_list_stats = calculate_duration_stats(&tool_list_durations);

        println!("\n{}", "=".repeat(80));
        println!("MCP TOOLS BENCHMARK SUMMARY (Tokio/Rust)");
        println!("{}", "=".repeat(80));
        println!();
        println!("Total Requests:      {:>12}", total_count);
        println!("Total Tool Calls:    {:>12}", tool_call_count);
        println!("Total Tool Lists:    {:>12}", tool_list_count);
        println!();
        println!("Throughput (RPS):");
        println!("  Overall:           {:>12.2}", overall_rps);
        println!("  Tool Call:         {:>12.2}", tool_call_rps);
        println!("  Tool List:         {:>12.2}", tool_list_rps);
        println!();
        println!("Success Rates:");
        println!("  Tool Call:         {:>11.2}%", tool_call_success_rate);
        println!("  Tool List:         {:>11.2}%", tool_list_success_rate);
        println!();
        println!("Tool Call Duration (ms):");
        println_duration_stats("  ", &tool_call_stats);
        println!();
        println!("Tool List Duration (ms):");
        println_duration_stats("  ", &tool_list_stats);
        println!();
        println!("{}", "=".repeat(80));
    }
}

struct DurationStats {
    avg: f64,
    min: f64,
    max: f64,
    p50: f64,
    p90: f64,
    p95: f64,
    p99: f64,
}

fn calculate_duration_stats(durations: &[f64]) -> DurationStats {
    if durations.is_empty() {
        return DurationStats {
            avg: 0.0, min: 0.0, max: 0.0, p50: 0.0, p90: 0.0, p95: 0.0, p99: 0.0,
        };
    }
    let mut sorted = durations.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let avg: f64 = sorted.iter().sum::<f64>() / sorted.len() as f64;
    let min = sorted.first().copied().unwrap_or(0.0);
    let max = sorted.last().copied().unwrap_or(0.0);
    let percentile = |p: f64| -> f64 {
        let idx = ((sorted.len() as f64 * p) / 100.0).floor() as usize;
        sorted.get(idx.min(sorted.len() - 1)).copied().unwrap_or(0.0)
    };
    DurationStats {
        avg, min, max,
        p50: percentile(50.0),
        p90: percentile(90.0),
        p95: percentile(95.0),
        p99: percentile(99.0),
    }
}

fn println_duration_stats(prefix: &str, stats: &DurationStats) {
    println!("{}Avg:  {:>12.2}", prefix, stats.avg);
    println!("{}Min:  {:>12.2}", prefix, stats.min);
    println!("{}Max:  {:>12.2}", prefix, stats.max);
    println!("{}p50:  {:>12.2}", prefix, stats.p50);
    println!("{}p90:  {:>12.2}", prefix, stats.p90);
    println!("{}p95:  {:>12.2}", prefix, stats.p95);
    println!("{}p99:  {:>12.2}", prefix, stats.p99);
}

#[derive(Debug, Serialize, Deserialize)]
struct JwtClaims {
    sub: String, exp: i64, iat: i64, aud: String, iss: String, jti: String,
    token_use: String, user: JwtUser,
}

#[derive(Debug, Serialize, Deserialize)]
struct JwtUser {
    email: String, full_name: String, is_admin: bool, auth_provider: String,
}

fn generate_jwt_token() -> String {
    let now = Utc::now();
    let exp = now + Duration::hours(8760);
    let claims = JwtClaims {
        sub: CONFIG.jwt_username.clone(),
        exp: exp.timestamp(),
        iat: now.timestamp(),
        aud: CONFIG.jwt_audience.clone(),
        iss: CONFIG.jwt_issuer.clone(),
        jti: Uuid::new_v4().to_string(),
        token_use: "session".to_string(),
        user: JwtUser {
            email: CONFIG.jwt_username.clone(),
            full_name: "Tokio MCP Benchmark".to_string(),
            is_admin: true,
            auth_provider: "local".to_string(),
        },
    };
    let header = r#"{"alg":"HS256","typ":"JWT"}"#;
    let payload = serde_json::to_string(&claims).unwrap();
    let header_b64 = URL_SAFE_NO_PAD.encode(header.as_bytes());
    let payload_b64 = URL_SAFE_NO_PAD.encode(payload.as_bytes());
    let message = format!("{}.{}", header_b64, payload_b64);
    let mut mac = HmacSha256::new_from_slice(CONFIG.jwt_secret_key.as_bytes()).unwrap();
    mac.update(message.as_bytes());
    let signature = mac.finalize().into_bytes();
    let signature_b64 = URL_SAFE_NO_PAD.encode(&signature);
    format!("{}.{}.{}", header_b64, payload_b64, signature_b64)
}

#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String, id: String, method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcResponse {
    jsonrpc: String, id: String,
    #[serde(default)]
    result: Option<serde_json::Value>,
    #[serde(default)]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcError { code: i64, message: String }

#[derive(Debug, Serialize, Deserialize)]
struct ToolsListResult { tools: Vec<ToolInfo> }

#[derive(Debug, Serialize, Deserialize)]
struct ToolInfo { name: String }

struct McpClient {
    client: Client,
    session_id: RwLock<Option<String>>,
}

impl McpClient {
    fn new() -> Self {
        McpClient {
            client: Client::builder().timeout(StdDuration::from_secs(30)).build().unwrap(),
            session_id: RwLock::new(None),
        }
    }

    async fn mcp_request(&self, method: &str, params: Option<serde_json::Value>, server_id: &str) -> Result<(Option<serde_json::Value>, f64)> {
        let start = Instant::now();
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Uuid::new_v4().to_string(),
            method: method.to_string(),
            params,
        };
        let url = format!("{}/servers/{}/mcp", CONFIG.host, server_id);
        let token = SHARED_STATE.get_token();
        let mut req_builder = self.client.post(&url)
            .json(&request)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .header("Authorization", format!("Bearer {}", token));
        if let Some(session_id) = self.session_id.read().unwrap().clone() {
            req_builder = req_builder.header("Mcp-Session-Id", session_id);
        }
        let response = req_builder.send().await?;
        if let Some(sid) = response.headers().get("Mcp-Session-Id") {
            if let Ok(sid_str) = sid.to_str() {
                *self.session_id.write().unwrap() = Some(sid_str.to_string());
            }
        }
        let duration_ms = start.elapsed().as_secs_f64() * 1000.0;
        if !response.status().is_success() {
            return Ok((None, duration_ms));
        }
        let json_response: JsonRpcResponse = response.json().await?;
        if let Some(error) = json_response.error {
            log::warn!("MCP error {}: {}", error.code, error.message);
            return Ok((None, duration_ms));
        }
        Ok((json_response.result, duration_ms))
    }

    async fn initialize(&self, server_id: &str) -> Result<bool> {
        let params = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {"tools": {}, "resources": {}, "prompts": {}},
            "clientInfo": {"name": "tokio-mcp-benchmark", "version": "1.0.0"}
        });
        let (result, _) = self.mcp_request("initialize", Some(params), server_id).await?;
        Ok(result.is_some())
    }

    async fn tools_list(&self, server_id: &str) -> Result<(Vec<String>, f64)> {
        let (result, duration) = self.mcp_request("tools/list", None, server_id).await?;
        let tools = match result {
            Some(val) => {
                let tools_result: ToolsListResult = serde_json::from_value(val)?;
                tools_result.tools.into_iter().map(|t| t.name).collect()
            }
            None => Vec::new(),
        };
        Ok((tools, duration))
    }

    async fn tools_call(&self, tool_name: &str, args: serde_json::Value, server_id: &str) -> Result<(bool, f64)> {
        let params = serde_json::json!({"name": tool_name, "arguments": args});
        let (result, duration) = self.mcp_request("tools/call", Some(params), server_id).await?;
        Ok((result.is_some(), duration))
    }
}

async fn auto_detect() -> Result<()> {
    if SHARED_STATE.is_initialized() {
        return Ok(());
    }
    if !CONFIG.server_id.is_empty() {
        let mcp_client = McpClient::new();
        mcp_client.initialize(&CONFIG.server_id).await.ok();
        let tools = if !CONFIG.tool_names_str.is_empty() {
            CONFIG.tool_names_str.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
        } else {
            let (tools, _) = mcp_client.tools_list(&CONFIG.server_id).await.unwrap_or_default();
            tools
        };
        let server_target = ServerTarget {
            server_id: CONFIG.server_id.clone(),
            server_name: CONFIG.server_id.clone(),
            tool_names: tools,
        };
        log::info!("Server: {} name={} tools={}", server_target.server_id, server_target.server_name, server_target.tool_names.len());
        SHARED_STATE.server_targets.write().unwrap().push(server_target);
    }
    SHARED_STATE.set_initialized();
    Ok(())
}

async fn benchmark_worker(worker_id: usize, run_time: StdDuration) -> Result<()> {
    let server_targets = SHARED_STATE.get_server_targets();
    if server_targets.is_empty() {
        return Ok(());
    }
    let server_target = &server_targets[worker_id % server_targets.len()];
    let tool_names = &server_target.tool_names;
    if tool_names.is_empty() {
        return Ok(());
    }
    let mcp_client = Arc::new(McpClient::new());
    let start = Instant::now();
    while start.elapsed() < run_time {
        let action = rand::random::<f64>();
        if action < 20.0 / 21.0 {
            let tool_name = tool_names.get(worker_id % tool_names.len()).cloned().unwrap_or_else(|| tool_names[0].clone());
            let args = build_tool_args(&tool_name);
            let (success, duration) = mcp_client.tools_call(&tool_name, args, &server_target.server_id).await.unwrap_or((false, 0.0));
            METRICS.record_tool_call(duration, success);
            let wait_ms = 20 + rand::random::<u64>() % 80;
            tokio::time::sleep(StdDuration::from_millis(wait_ms)).await;
        } else {
            let (_, duration) = mcp_client.tools_list(&server_target.server_id).await.unwrap_or((Vec::new(), 0.0));
            METRICS.record_tool_list(duration, true);
            let wait_ms = 20 + rand::random::<u64>() % 80;
            tokio::time::sleep(StdDuration::from_millis(wait_ms)).await;
        }
    }
    Ok(())
}

fn build_tool_args(tool_name: &str) -> serde_json::Value {
    let name_lower = tool_name.to_lowercase();
    let timezones = ["UTC", "America/New_York", "America/Los_Angeles", "Europe/London", "Europe/Paris", "Asia/Tokyo", "Asia/Shanghai", "Australia/Sydney"];
    if name_lower.contains("time") {
        let tz = timezones[rand::random::<usize>() % timezones.len()];
        serde_json::json!({ "timezone": tz })
    } else if name_lower.contains("echo") {
        serde_json::json!({ "message": format!("perf-test-{}", rand::random::<u64>() % 10000) })
    } else if name_lower.contains("convert") {
        let src = timezones[rand::random::<usize>() % timezones.len()];
        let dst = timezones[(rand::random::<usize>() + 1) % timezones.len()];
        serde_json::json!({ "time": "2025-01-01T00:00:00Z", "source_timezone": src, "target_timezone": dst })
    } else {
        serde_json::Value::Object(serde_json::Map::new())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    println!("{}", "=".repeat(80));
    println!("MCP TOOLS BENCHMARK (Tokio/Rust)");
    println!("{}", "=".repeat(80));
    println!("Profile:      {}", CONFIG.profile);
    println!("Users:        {}", CONFIG.users);
    println!("Duration:     {}s", CONFIG.run_time_secs);
    println!("Host:         {}", CONFIG.host);
    println!("Server ID:    {}", CONFIG.server_id);
    println!("{}", "=".repeat(80));
    auto_detect().await?;
    *TEST_START_TIME.write().unwrap() = Some(Instant::now());
    let run_time = StdDuration::from_secs(CONFIG.run_time_secs);
    let mut handles = Vec::new();
    for i in 0..CONFIG.users {
        let handle = tokio::spawn(benchmark_worker(i, run_time));
        handles.push(handle);
    }
    for handle in handles {
        handle.await??;
    }
    METRICS.print_summary();
    Ok(())
}
