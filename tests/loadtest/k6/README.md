# MCP Tools Benchmark with k6

Load test script for benchmarking MCP tools/call performance using k6.

## Overview

This script is the k6 equivalent of `make benchmark-mcp-tools` (Locust-based).
It tests ONLY the MCP Streamable HTTP transport path through the gateway virtual server.

**Test Scenario:**
- Heavy `tools/call` in a tight loop (similar to Locust's `MCPToolCallerUser`)
- Occasional `tools/list` for discovery
- Auto-discovers tools from the MCP server or uses provided tool names

## Prerequisites

1. **Install k6**: https://k6.io/docs/getting-started/installation/

   ```bash
   # macOS
   brew install k6

   # Ubuntu/Debian
   sudo apt-get install k6

   # Windows (Chocolatey)
   choco install k6

   # Docker
   docker run --rm grafana/k6
   ```

2. **Running Gateway**: Ensure the ContextForge gateway is running with at least one MCP server connected.

3. **Server ID**: Know the UUID of the virtual server to test.

## Quick Start

### 1. Find Your Server ID

```bash
# Query the gateway REST API
curl -s http://localhost:4444/api/servers \
  -H "Authorization: Bearer $(python -m mcpgateway.utils.create_jwt_token --username admin@example.com --exp 3600 --secret my-test-key-but-now-longer-than-32-bytes)" \
  | jq '.[] | {name: .name, id: .id, tools: (.associatedTools | length)}'
```

### 2. Run the Benchmark

```bash
# Quick test (default: 10 VUs, 10s)
k6 run tests/loadtest/k6/mcp-tools-benchmark.ts

# Production benchmark (125 VUs, 60s) - equivalent to make benchmark-mcp-tools
k6 run --vus 125 --duration 60s tests/loadtest/k6/mcp-tools-benchmark.ts

# High-load test (300 VUs, 5min) - equivalent to make benchmark-mcp-tools-300
k6 run --vus 300 --duration 300s tests/loadtest/k6/mcp-tools-benchmark.ts
```

### 3. Set Required Environment Variables

```bash
# Required: Server ID to test
export K6_MCP_SERVER_ID=7acd24c38b784628bce6e0bf96256008

# Optional: Custom gateway URL (default: http://localhost:8080)
export K6_MCP_HOST=http://localhost:4444

# Optional: Specific tool names (comma-separated, auto-detected if empty)
export K6_MCP_TOOL_NAMES=get_system_time,echo,get_stats

# Optional: Pre-generated JWT token (overrides JWT generation)
export K6_BEARER_TOKEN=eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...
```

## Usage Examples

### Basic Benchmark (125 VUs, 60s)

```bash
export K6_MCP_SERVER_ID=7acd24c38b784628bce6e0bf96256008
k6 run --vus 125 --duration 60s tests/loadtest/k6/mcp-tools-benchmark.ts
```

### Docker Execution

```bash
docker run --rm --network host \
  -e K6_MCP_SERVER_ID=7acd24c38b784628bce6e0bf96256008 \
  -e K6_MCP_HOST=http://host.docker.internal:4444 \
  grafana/k6 run /tests/loadtest/k6/mcp-tools-benchmark.ts \
  --vus 125 --duration 60s
```

### With Custom JWT Settings

```bash
export K6_MCP_SERVER_ID=7acd24c38b784628bce6e0bf96256008
export K6_JWT_SECRET_KEY=my-custom-secret-key
export K6_JWT_USERNAME=admin@mycompany.com
k6 run --vus 125 --duration 60s tests/loadtest/k6/mcp-tools-benchmark.ts
```

### Stressed Test with Thresholds

```bash
export K6_MCP_SERVER_ID=7acd24c38b784628bce6e0bf96256008
k6 run --vus 300 --duration 300s \
  --tag test_type=stress \
  tests/loadtest/k6/mcp-tools-benchmark.ts
```

## Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `K6_MCP_HOST` | `http://localhost:8080` | Gateway URL |
| `K6_MCP_SERVER_ID` | *(required)* | Virtual server UUID to test |
| `K6_MCP_TOOL_NAMES` | *(auto-detect)* | Comma-separated tool names |
| `K6_JWT_SECRET_KEY` | `my-test-key-but-now-longer-than-32-bytes` | JWT signing secret |
| `K6_JWT_ALGORITHM` | `HS256` | JWT algorithm |
| `K6_JWT_AUDIENCE` | `mcpgateway-api` | JWT audience |
| `K6_JWT_ISSUER` | `mcpgateway` | JWT issuer |
| `K6_JWT_USERNAME` | `admin@example.com` | Admin email for JWT |
| `K6_BEARER_TOKEN` | *(generated)* | Pre-generated JWT token |
| `K6_LOG_LEVEL` | `info` | Log level (debug, info, warn, error) |

### k6 Options

The script includes built-in thresholds:

```typescript
export const options = {
  thresholds: {
    tool_call_success: ['rate>0.95'],  // 95% success rate required
    tool_list_success: ['rate>0.95'],
    tool_call_duration_ms: ['p(50)<100', 'p(95)<500', 'p(99)<1000'],
    tool_list_duration_ms: ['p(50)<200', 'p(95)<1000', 'p(99)<2000'],
  },
};
```

Override thresholds via command line:

```bash
k6 run --vus 125 --duration 60s \
  --no-thresholds \
  tests/loadtest/k6/mcp-tools-benchmark.ts
```

## Metrics

### Custom Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `tool_call_success` | Rate | Percentage of successful tool calls |
| `tool_list_success` | Rate | Percentage of successful tool lists |
| `tool_call_duration_ms` | Trend | Tool call response time (ms) |
| `tool_list_duration_ms` | Trend | Tool list response time (ms) |
| `tool_calls_total` | Counter | Total number of tool calls |
| `tool_lists_total` | Counter | Total number of tool lists |
| `tools_discovered` | Counter | Number of tools discovered during setup |

### Output Example

```
========================================
MCP Tools Benchmark Summary
========================================

Total Tool Calls: 45678
Total Tool Lists: 2284
Tools Discovered: 3

Success Rates:
  Tool Call: 99.8%
  Tool List: 100%

Tool Call Duration (ms):
  Avg: 12.45
  Min: 2.10
  Max: 234.56
  p95: 45.67
  p99: 89.12

========================================
```

## Comparison with Locust

| Feature | Locust (`make benchmark-mcp-tools`) | k6 |
|---------|-------------------------------------|----|
| **Engine** | Python/Gevent | Go/JavaScript |
| **VU Model** | Greenlets | Isolates (VUs) |
| **Script** | Python | TypeScript/JavaScript |
| **Metrics** | Custom Python | Built-in + custom |
| **Thresholds** | Manual checks | Built-in threshold system |
| **Output** | Console + HTML | Console + JSON + JUnit |
| **Resource Usage** | Higher (Python) | Lower (Go runtime) |
| **Max VUs** | ~500-1000 per worker | ~5000+ per instance |

## Troubleshooting

### No Tools Discovered

```
ERRO[0001] No tools available to call. Check K6_MCP_SERVER_ID or tool discovery.
```

**Solution:**
1. Verify the server ID is correct: `export K6_MCP_SERVER_ID=<correct-id>`
2. Check gateway connectivity: `curl http://localhost:4444/health`
3. Verify tools exist: Query `/api/tools` endpoint

### Authentication Failures

```
tool_call_success: 0%
```

**Solution:**
1. Check JWT settings match your gateway configuration
2. Use a pre-generated token: `export K6_BEARER_TOKEN=<token>`
3. Verify `K6_JWT_SECRET_KEY` matches gateway's `JWT_SECRET_KEY`

### High Latency

```
tool_call_duration_ms: p(95)=2500 (threshold: p(95)<500)
```

**Solution:**
1. Check gateway logs for bottlenecks
2. Verify database connection pool settings
3. Check if Redis cache is enabled and working
4. Reduce VU count to identify saturation point

## Advanced Usage

### Distributed Testing

```bash
# Master node
k6 run --vus 0 --duration 60s \
  --no-thresholds \
  --out json=results.json \
  tests/loadtest/k6/mcp-tools-benchmark.ts

# Worker nodes (on separate machines)
k6 run --vus 100 --duration 60s \
  --no-thresholds \
  --out json=worker1.json \
  tests/loadtest/k6/mcp-tools-benchmark.ts
```

### Cloud Execution (Grafana k6 Cloud)

```bash
npm install -g k6
k6 cloud tests/loadtest/k6/mcp-tools-benchmark.ts \
  -e K6_MCP_SERVER_ID=7acd24c38b784628bce6e0bf96256008 \
  -e K6_MCP_HOST=https://my-gateway.example.com
```

### Output to InfluxDB + Grafana

```bash
k6 run --vus 125 --duration 60s \
  --out influxdb=http://localhost:8086/k6 \
  tests/loadtest/k6/mcp-tools-benchmark.ts
```

## License

Copyright 2025
SPDX-License-Identifier: Apache-2.0
