#!/bin/bash
#
# MCP Tools Benchmark with k6 - Quick Start Script
#
# Usage:
#   ./run.sh [quick|standard|heavy]
#
# Examples:
#   ./run.sh quick     # 10 VUs, 10s
#   ./run.sh standard  # 50 VUs, 30s (default)
#   ./run.sh heavy     # 125 VUs, 60s
#
# Environment Variables (same as make benchmark-mcp-tools):
#   MCP_BENCHMARK_SERVER_ID  - Virtual server UUID
#   MCP_BENCHMARK_HOST       - Gateway URL (default: http://localhost:8080)
#   K6_JWT_SECRET_KEY        - JWT signing secret
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROFILE="${1:-standard}"

# Configuration (check MCP_BENCHMARK_* vars first for consistency with make targets, then K6_* vars, then defaults)
MCP_HOST="${MCP_BENCHMARK_HOST:-${K6_MCP_HOST:-http://localhost:8080}}"
MCP_SERVER_ID="${MCP_BENCHMARK_SERVER_ID:-${K6_MCP_SERVER_ID:-b8e3f1a2c4d5e6f7a1b2c3d4e5f6a7b8}}"
JWT_SECRET="${K6_JWT_SECRET_KEY:-my-test-key-but-now-longer-than-32-bytes}"

# Profile configurations
case "${PROFILE}" in
  quick)
    VUS=10
    DURATION=10s
    ;;
  standard)
    VUS=50
    DURATION=30s
    ;;
  heavy)
    VUS=125
    DURATION=60s
    ;;
  *)
    echo "Unknown profile: ${PROFILE}"
    echo "Valid profiles: quick, standard, heavy"
    exit 1
    ;;
esac

echo "========================================"
echo "  MCP Tools Benchmark (k6)"
echo "========================================"
echo "  Profile:    ${PROFILE}"
echo "  VUs:        ${VUS}"
echo "  Duration:   ${DURATION}"
echo "  Host:       ${MCP_HOST}"
echo "  Server ID:  ${MCP_SERVER_ID}"
echo "========================================"
echo ""

export K6_MCP_HOST="${MCP_HOST}"
export K6_MCP_SERVER_ID="${MCP_SERVER_ID}"
export K6_JWT_SECRET_KEY="${JWT_SECRET}"

k6 run \
  --vus "${VUS}" \
  --duration "${DURATION}" \
  "${SCRIPT_DIR}/mcp-tools-benchmark.ts"
