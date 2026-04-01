#!/bin/bash
#
# MCP Tools Benchmark with Goose Load Testing Framework - Quick Start Script
#
# Usage:
#   ./run-goose.sh [quick|standard|heavy]
#
# Examples:
#   ./run-goose.sh quick     # 10 users, 10s
#   ./run-goose.sh standard  # 50 users, 30s (default)
#   ./run-goose.sh heavy     # 125 users, 60s
#
# Environment Variables:
#   MCP_BENCHMARK_SERVER_ID  - Virtual server UUID (required)
#   MCP_BENCHMARK_HOST       - Gateway URL (default: http://localhost:8080)
#   JWT_SECRET_KEY           - JWT signing secret
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/../../../tools_rust/mcp-benchmark-goose" && pwd)"
PROFILE="${1:-standard}"

# Configuration (check MCP_BENCHMARK_* vars first, then K6_* vars for consistency, then defaults)
MCP_HOST="${MCP_BENCHMARK_HOST:-${K6_MCP_HOST:-http://localhost:8080}}"
MCP_SERVER_ID="${MCP_BENCHMARK_SERVER_ID:-${K6_MCP_SERVER_ID:-b8e3f1a2c4d5e6f7a1b2c3d4e5f6a7b8}}"
JWT_SECRET="${JWT_SECRET_KEY:-my-test-key-but-now-longer-than-32-bytes}"

# Profile configurations
case "${PROFILE}" in
  quick)
    USERS=10
    DURATION=10
    ;;
  standard)
    USERS=50
    DURATION=30
    ;;
  heavy)
    USERS=125
    DURATION=60
    ;;
  *)
    echo "Unknown profile: ${PROFILE}"
    echo "Valid profiles: quick, standard, heavy"
    exit 1
    ;;
esac

echo "========================================"
echo "  MCP Tools Benchmark (Goose/Rust)"
echo "========================================"
echo "  Profile:    ${PROFILE}"
echo "  Users:      ${USERS}"
echo "  Duration:   ${DURATION}s"
echo "  Host:       ${MCP_HOST}"
echo "  Server ID:  ${MCP_SERVER_ID}"
echo "========================================"
echo ""

export MCP_BENCHMARK_HOST="${MCP_HOST}"
export MCP_BENCHMARK_SERVER_ID="${MCP_SERVER_ID}"
export JWT_SECRET_KEY="${JWT_SECRET}"

# Build if needed
echo "Building mcp-benchmark-goose..."
cd "${PROJECT_DIR}"
cargo build --release --quiet 2>/dev/null || cargo build --release

echo ""
echo "Running benchmark..."
echo ""

# Run the benchmark with goose options
"${PROJECT_DIR}/target/release/mcp-benchmark-goose" \
  --users "${USERS}" \
  --run-time "${DURATION}s"
