# MCP Benchmark Tools (Rust)

High-performance MCP (Model Context Protocol) tools benchmarks implemented in Rust.

## Available Tools

### 1. mcp-benchmark (Tokio-based)

**Location:** `tools_rust/mcp-benchmark/`

A lightweight, Tokio-based load test tool with custom metrics collection.

**Best for:**
- Controlled, predictable load testing
- Lower resource usage
- Simpler deployment (single binary)

**Usage:**
```bash
cd tools_rust/mcp-benchmark

# Quick test (10 users, 10s)
MCP_BENCHMARK_SERVER_ID=<server-id> cargo run --release -- --profile quick

# Standard test (50 users, 30s)
MCP_BENCHMARK_SERVER_ID=<server-id> cargo run --release -- --profile standard

# Heavy test (125 users, 60s)
MCP_BENCHMARK_SERVER_ID=<server-id> cargo run --release -- --profile heavy

# Custom configuration
MCP_BENCHMARK_SERVER_ID=<server-id> MCP_BENCHMARK_HOST=http://localhost:8080 \
  cargo run --release -- --users 100 --run-time 120
```

### 2. mcp-benchmark-goose (Goose-based)

**Location:** `tools_rust/mcp-benchmark-goose/`

Built on the [Goose](https://github.com/tag1consulting/goose) load testing framework - a Rust equivalent of Locust.

**Best for:**
- Integration with existing Goose workflows
- Distributed load testing (Gaggle mode)
- HTML/JSON report generation
- More realistic user simulation

**Usage:**
```bash
cd tools_rust/mcp-benchmark-goose

# Quick test (10 users, 10s)
MCP_BENCHMARK_SERVER_ID=<server-id> cargo run --release -- --users 10 --run-time 10s

# Standard test (50 users, 30s)
MCP_BENCHMARK_SERVER_ID=<server-id> cargo run --release -- --users 50 --run-time 30s

# Heavy test (125 users, 60s)
MCP_BENCHMARK_SERVER_ID=<server-id> cargo run --release -- --users 125 --run-time 60s

# With reports
MCP_BENCHMARK_SERVER_ID=<server-id> cargo run --release -- --users 50 --run-time 30s \
  --report-file report.html --report-markdown report.md
```

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `MCP_BENCHMARK_HOST` | Gateway URL | `http://localhost:8080` |
| `MCP_BENCHMARK_SERVER_ID` | Virtual server UUID (required) | - |
| `JWT_SECRET_KEY` | JWT signing secret | `my-test-key-but-now-longer-than-32-bytes` |

## Test Scenario

Both tools implement the same test scenario matching the k6/Locust benchmarks:

1. **Initialization**: Discover tools via MCP `tools/list`
2. **Tool Calling**: 20:1 ratio of `tools/call` to `tools/list`
3. **Think Time**: 20-80ms random delay between requests
4. **Session Management**: Maintains MCP session IDs across requests

## Performance Comparison

| Tool | RPS (50 users) | Memory | CPU | Best Use |
|------|----------------|--------|-----|----------|
| **Tokio** | ~500 | Low | Low | Quick tests, CI/CD |
| **Goose** | ~450 | Medium | Medium | Production load tests |
| **k6** | ~480 | Medium | Medium | JS ecosystem |
| **Locust** | ~400 | High | High | Python ecosystem |

## Building

```bash
# Build Tokio version
cd tools_rust/mcp-benchmark
cargo build --release

# Build Goose version
cd tools_rust/mcp-benchmark-goose
cargo build --release
```

## Example Output (Tokio)

```
================================================================================
MCP TOOLS BENCHMARK SUMMARY (Tokio/Rust)
================================================================================

Total Requests:             15465
Total Tool Calls:           14727
Total Tool Lists:             738

Throughput (RPS):
  Overall:                 511.18
  Tool Call:               486.78
  Tool List:                24.39

Success Rates:
  Tool Call:              100.00%
  Tool List:              100.00%

Tool Call Duration (ms):
  Avg:         37.68
  Min:         12.38
  Max:       3430.06
  p50:         26.73
  p90:         46.03
  p95:         55.79
  p99:        120.82
```

## License

Apache-2.0
