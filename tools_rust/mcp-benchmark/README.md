# MCP Benchmark (Rust/Tokio)

High-performance MCP (Model Context Protocol) tools benchmark implemented in Rust using Tokio async runtime.

## Overview

This benchmark tool tests the MCP Streamable HTTP transport path through the ContextForge gateway, equivalent to the k6 and Locust versions but implemented in Rust for maximum performance.

## Features

- **High Performance**: Native Rust implementation with Tokio async runtime
- **MCP Protocol Support**: Full JSON-RPC 2.0 compliance with session management
- **Auto-Detection**: Automatically discovers server ID and tool names
- **JWT Authentication**: Built-in JWT token generation
- **Comprehensive Metrics**: RPS, success rates, and detailed latency percentiles
- **Configurable**: Multiple profiles (quick, standard, heavy) or custom settings

## Quick Start

```bash
# Quick test (10 users, 10s)
./run.sh quick

# Standard test (50 users, 30s)
./run.sh standard

# Heavy test (125 users, 60s)
./run.sh heavy
```

## Usage

### From run.sh script

```bash
cd tests/loadtest/goose
./run.sh [quick|standard|heavy]
```

### Direct execution

```bash
cd tools_rust/mcp-benchmark
cargo run --release -- --profile heavy
```

### Custom configuration

```bash
cargo run --release -- \
  --users 100 \
  --run-time 120 \
  --host http://localhost:8080 \
  --server-id b8e3f1a2c4d5e6f7a1b2c3d4e5f6a7b8
```

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `MCP_BENCHMARK_HOST` | Gateway URL | `http://localhost:8080` |
| `MCP_BENCHMARK_SERVER_ID` | Virtual server UUID (required) | - |
| `MCP_BENCHMARK_TOOL_NAMES` | Comma-separated tool names | Auto-detected |
| `JWT_SECRET_KEY` | JWT signing secret | `my-test-key-but-now-longer-than-32-bytes` |
| `JWT_ALGORITHM` | JWT algorithm | `HS256` |
| `JWT_AUDIENCE` | JWT audience | `mcpgateway-api` |
| `JWT_ISSUER` | JWT issuer | `mcpgateway` |
| `JWT_USERNAME` | Admin email | `admin@example.com` |

## Test Scenario

The benchmark simulates realistic MCP client behavior:

1. **Initialization**: Auto-detects server and tools via MCP `tools/list`
2. **Tool Calling**: 20:1 ratio of `tools/call` to `tools/list` (matching k6/Locust)
3. **Think Time**: 20-80ms random delay between requests
4. **Session Management**: Maintains MCP session IDs across requests

## Output Example

```
================================================================================
MCP TOOLS BENCHMARK SUMMARY (Tokio/Rust)
================================================================================

Total Requests:           35548
Total Tool Calls:         33904
Total Tool Lists:          1644

Throughput (RPS):
  Overall:                592.47
  Tool Call:              565.07
  Tool List:               27.40

Success Rates:
  Tool Call:               100.0%
  Tool List:               100.0%

Tool Call Duration (ms):
  Avg:          154.67
  Min:           22.00
  Max:          1250.00
  p50:          133.00
  p90:          310.00
  p95:          380.00
  p99:          549.97

Tool List Duration (ms):
  Avg:           66.72
  Min:            9.00
  Max:           450.00
  p50:           53.00
  p90:          154.00
  p95:          190.00
  p99:          232.00

================================================================================
```

## Building

```bash
cd tools_rust/mcp-benchmark
cargo build --release
```

The binary will be at `target/release/mcp-benchmark`.

## Comparison with k6 and Locust

| Feature | k6 | Locust | Rust/Tokio |
|---------|----|--------|------------|
| Language | JavaScript | Python | Rust |
| Concurrency | Goroutines | gevent | Tokio async |
| Memory Usage | Medium | High | Low |
| Performance | High | Medium | Very High |
| Ease of Use | Easy | Easy | Medium |
| Customization | Medium | High | High |

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    MCP Benchmark (Rust)                     │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────┐  ┌─────────┐  ┌─────────┐  ...  ┌─────────┐  │
│  │ Worker  │  │ Worker  │  │ Worker  │       │ Worker  │  │
│  │   #1    │  │   #2    │  │   #3    │       │   #N    │  │
│  └────┬────┘  └────┬────┘  └────┬────┘       └────┬────┘  │
│       │           │           │                   │       │
│       └───────────┴───────────┴───────────────────┘       │
│                           │                                 │
│                  ┌────────▼────────┐                       │
│                  │  Shared State   │                       │
│                  │  - JWT Token    │                       │
│                  │  - Server Info  │                       │
│                  └────────┬────────┘                       │
│                           │                                 │
│                  ┌────────▼────────┐                       │
│                  │     Metrics     │                       │
│                  │  - Counters     │                       │
│                  │  - Durations    │                       │
│                  └─────────────────┘                       │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
                   ┌─────────────────┐
                   │  MCP Gateway    │
                   │  (HTTP/SSE)     │
                   └─────────────────┘
```

## License

Apache-2.0
