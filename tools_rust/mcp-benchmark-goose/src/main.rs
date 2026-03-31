// -*- coding: utf-8 -*-
/**
 * MCP Tools Benchmark - Goose Load Test Script (Optimized)
 *
 * Optimized version with:
 * - Cached JWT token (generated once)
 * - Cached Authorization header
 * - Reduced allocations in hot paths
 * - Efficient session data handling
 *
 * Usage:
 *   cargo run --release -- --users 125 --run-time 60s
 */

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{Duration, Utc};
use goose::prelude::*;
use hmac::{Hmac, Mac};
use once_cell::sync::Lazy;
use sha2::Sha256;
use std::env;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

static CONFIG: Lazy<Config> = Lazy::new(Config::from_env);
static AUTH_HEADER: Lazy<String> = Lazy::new(|| format!("Bearer {}", generate_jwt_token()));

#[derive(Debug, Clone)]
struct Config {
    host: String,
    server_id: String,
    jwt_secret_key: String,
    jwt_audience: String,
    jwt_issuer: String,
    jwt_username: String,
    bearer_token: String,
}

impl Config {
    fn from_env() -> Self {
        let _ = dotenvy::dotenv();
        Config {
            host: env::var("MCP_BENCHMARK_HOST")
                .unwrap_or_else(|_| "http://localhost:8080".to_string()),
            server_id: env::var("MCP_BENCHMARK_SERVER_ID")
                .or_else(|_| env::var("K6_MCP_SERVER_ID"))
                .unwrap_or_default(),
            jwt_secret_key: env::var("JWT_SECRET_KEY")
                .or_else(|_| env::var("K6_JWT_SECRET_KEY"))
                .unwrap_or_else(|_| "my-test-key-but-now-longer-than-32-bytes".to_string()),
            jwt_audience: env::var("JWT_AUDIENCE").unwrap_or_else(|_| "mcpgateway-api".to_string()),
            jwt_issuer: env::var("JWT_ISSUER").unwrap_or_else(|_| "mcpgateway".to_string()),
            jwt_username: env::var("JWT_USERNAME").unwrap_or_else(|_| "admin@example.com".to_string()),
            bearer_token: env::var("MCPGATEWAY_BEARER_TOKEN")
                .or_else(|_| env::var("K6_BEARER_TOKEN"))
                .unwrap_or_default(),
        }
    }
}

fn generate_jwt_token() -> String {
    if !CONFIG.bearer_token.is_empty() {
        return CONFIG.bearer_token.clone();
    }
    
    let now = Utc::now();
    let exp = now + Duration::hours(8760);
    let claims = serde_json::json!({
        "sub": CONFIG.jwt_username,
        "exp": exp.timestamp(),
        "iat": now.timestamp(),
        "aud": CONFIG.jwt_audience,
        "iss": CONFIG.jwt_issuer,
        "jti": Uuid::new_v4().to_string(),
        "token_use": "session",
        "user": {
            "email": CONFIG.jwt_username,
            "full_name": "Goose MCP Benchmark",
            "is_admin": true,
            "auth_provider": "local"
        }
    });
    let header = r#"{"alg":"HS256","typ":"JWT"}"#;
    let payload = claims.to_string();
    let header_b64 = URL_SAFE_NO_PAD.encode(header.as_bytes());
    let payload_b64 = URL_SAFE_NO_PAD.encode(payload.as_bytes());
    let message = format!("{}.{}", header_b64, payload_b64);
    let mut mac = HmacSha256::new_from_slice(CONFIG.jwt_secret_key.as_bytes()).unwrap();
    mac.update(message.as_bytes());
    let signature = mac.finalize().into_bytes();
    let signature_b64 = URL_SAFE_NO_PAD.encode(&signature);
    format!("{}.{}.{}", header_b64, payload_b64, signature_b64)
}

fn build_tool_args(tool_name: &str) -> serde_json::Value {
    let name_lower = tool_name.to_lowercase();
    const TIMEZONES: [&str; 8] = ["UTC", "America/New_York", "America/Los_Angeles", "Europe/London", "Europe/Paris", "Asia/Tokyo", "Asia/Shanghai", "Australia/Sydney"];
    if name_lower.contains("time") {
        serde_json::json!({ "timezone": TIMEZONES[rand::random::<usize>() % TIMEZONES.len()] })
    } else if name_lower.contains("echo") {
        serde_json::json!({ "message": format!("perf-test-{}", rand::random::<u64>() % 10000) })
    } else if name_lower.contains("convert") {
        let src = TIMEZONES[rand::random::<usize>() % TIMEZONES.len()];
        let dst = TIMEZONES[(rand::random::<usize>() + 1) % TIMEZONES.len()];
        serde_json::json!({ "time": "2025-01-01T00:00:00Z", "source_timezone": src, "target_timezone": dst })
    } else {
        serde_json::Value::Object(serde_json::Map::new())
    }
}

/// Initialize MCP session and discover tools
async fn mcp_initialize(user: &mut GooseUser) -> TransactionResult {
    if CONFIG.server_id.is_empty() {
        log::error!("MCP_BENCHMARK_SERVER_ID is required");
        return Ok(());
    }

    let mcp_url = format!("/servers/{}/mcp", CONFIG.server_id);
    let auth_header = &*AUTH_HEADER;

    // Initialize MCP session
    let init_payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": Uuid::new_v4().to_string(),
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {"tools": {}, "resources": {}, "prompts": {}},
            "clientInfo": {"name": "goose-mcp-benchmark", "version": "1.0.0"}
        }
    });

    let request_builder = user
        .get_request_builder(&GooseMethod::Post, &mcp_url)?
        .json(&init_payload)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("Authorization", auth_header);

    let goose_request = GooseRequest::builder()
        .set_request_builder(request_builder)
        .build();
    let _ = user.request(goose_request).await?;

    // Get tools list
    let tools_payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": Uuid::new_v4().to_string(),
        "method": "tools/list"
    });

    let request_builder = user
        .get_request_builder(&GooseMethod::Post, &mcp_url)?
        .json(&tools_payload)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("Authorization", auth_header);

    let goose_request = GooseRequest::builder()
        .set_request_builder(request_builder)
        .build();
    let goose_response = user.request(goose_request).await?;

    // Parse tools
    if let Ok(response) = goose_response.response {
        if let Ok(json) = response.json::<serde_json::Value>().await {
            if let Some(tools) = json.get("result").and_then(|r: &serde_json::Value| r.get("tools")).and_then(|t: &serde_json::Value| t.as_array()) {
                let tool_names: Vec<String> = tools
                    .iter()
                    .filter_map(|t: &serde_json::Value| t.get("name").and_then(|n| n.as_str()).map(String::from))
                    .collect();
                
                if !tool_names.is_empty() {
                    log::info!("Server: {} tools={}", CONFIG.server_id, tool_names.len());
                    user.set_session_data(tool_names);
                }
            }
        }
    }

    Ok(())
}

/// Call a tool with proper arguments (optimized)
async fn tool_call(user: &mut GooseUser) -> TransactionResult {
    let tool_names: Vec<String> = match user.get_session_data::<Vec<String>>() {
        Some(names) => names.clone(),
        None => return Ok(()),
    };

    if tool_names.is_empty() {
        return Ok(());
    }

    let tool_name = &tool_names[rand::random::<usize>() % tool_names.len()];
    let args = build_tool_args(tool_name);
    let mcp_url = format!("/servers/{}/mcp", CONFIG.server_id);
    let auth_header = &*AUTH_HEADER;

    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": Uuid::new_v4().to_string(),
        "method": "tools/call",
        "params": {
            "name": tool_name,
            "arguments": args
        }
    });

    let request_builder = user
        .get_request_builder(&GooseMethod::Post, &mcp_url)?
        .json(&payload)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("Authorization", auth_header);

    let goose_request = GooseRequest::builder()
        .set_request_builder(request_builder)
        .build();
    let _ = user.request(goose_request).await?;

    Ok(())
}

/// List available tools (optimized)
async fn tools_list(user: &mut GooseUser) -> TransactionResult {
    let mcp_url = format!("/servers/{}/mcp", CONFIG.server_id);
    let auth_header = &*AUTH_HEADER;

    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": Uuid::new_v4().to_string(),
        "method": "tools/list"
    });

    let request_builder = user
        .get_request_builder(&GooseMethod::Post, &mcp_url)?
        .json(&payload)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("Authorization", auth_header);

    let goose_request = GooseRequest::builder()
        .set_request_builder(request_builder)
        .build();
    let _ = user.request(goose_request).await?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), GooseError> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    println!("{}", "=".repeat(80));
    println!("MCP TOOLS BENCHMARK (Goose/Rust - Optimized)");
    println!("{}", "=".repeat(80));
    println!("Host:         {}", CONFIG.host);
    println!("Server ID:    {}", CONFIG.server_id);
    println!("Auth:         {}", if CONFIG.bearer_token.is_empty() { "Auto-generated JWT" } else { "Pre-generated token" });
    println!("{}", "=".repeat(80));

    let mut scenario = scenario!("McpBenchmark").set_host(&CONFIG.host);
    scenario = scenario
        .register_transaction(transaction!(mcp_initialize).set_on_start())
        .register_transaction(transaction!(tool_call).set_weight(20)?)
        .register_transaction(transaction!(tools_list).set_weight(1)?);

    GooseAttack::initialize()?
        .register_scenario(scenario)
        .execute()
        .await?;

    Ok(())
}
