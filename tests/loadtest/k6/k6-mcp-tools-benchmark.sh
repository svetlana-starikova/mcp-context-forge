#!/bin/bash
# -*- coding: utf-8 -*-
#
# MCP Tools Benchmark with k6 - Helper Script
#
# Equivalent to `make benchmark-mcp-tools` but using k6.
#
# Usage:
#   ./k6-mcp-tools-benchmark.sh [quick|standard|heavy|stress]
#
# Examples:
#   ./k6-mcp-tools-benchmark.sh quick     # 10 VUs, 30s
#   ./k6-mcp-tools-benchmark.sh standard  # 125 VUs, 60s (default)
#   ./k6-mcp-tools-benchmark.sh heavy     # 300 VUs, 5min
#   ./k6-mcp-tools-benchmark.sh stress    # 500 VUs, 10min
#

set -euo pipefail

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"

# Default configuration
PROFILE="${1:-standard}"
MCP_SERVER_ID="${K6_MCP_SERVER_ID:-}"
MCP_HOST="${K6_MCP_HOST:-http://localhost:4444}"
JWT_SECRET="${K6_JWT_SECRET_KEY:-my-test-key-but-now-longer-than-32-bytes}"
JWT_USERNAME="${K6_JWT_USERNAME:-admin@example.com}"

# Profile configurations
declare -A PROFILES=(
  ["quick"]="10:30s"
  ["standard"]="125:60s"
  ["heavy"]="300:300s"
  ["stress"]="500:600s"
)

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Helper functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Show usage
usage() {
    cat << EOF
MCP Tools Benchmark with k6

Usage: $(basename "$0") [PROFILE]

Profiles:
  quick     - 10 VUs, 30s (fast sanity check)
  standard  - 125 VUs, 60s (default, equivalent to make benchmark-mcp-tools)
  heavy     - 300 VUs, 5min (equivalent to make benchmark-mcp-tools-300)
  stress    - 500 VUs, 10min (extreme load test)

Environment Variables:
  K6_MCP_SERVER_ID     - MCP virtual server UUID (required if not auto-detected)
  K6_MCP_HOST          - Gateway URL (default: http://localhost:4444)
  K6_JWT_SECRET_KEY    - JWT signing secret
  K6_JWT_USERNAME      - Admin email for JWT generation

Examples:
  $(basename "$0") quick
  $(basename "$0") standard
  $(basename "$0") heavy
  K6_MCP_SERVER_ID=abc123 $(basename "$0") standard

Prerequisites:
  - k6 installed (https://k6.io/docs/getting-started/installation/)
  - Gateway running with at least one MCP server connected

EOF
    exit 1
}

# Check prerequisites
check_prereqs() {
    if ! command -v k6 &> /dev/null; then
        log_error "k6 is not installed"
        echo ""
        echo "Install k6: https://k6.io/docs/getting-started/installation/"
        echo ""
        echo "  macOS:      brew install k6"
        echo "  Ubuntu:     sudo apt-get install k6"
        echo "  Docker:     docker run --rm grafana/k6"
        echo ""
        exit 1
    fi

    log_info "k6 version: $(k6 version | head -1)"
}

# Auto-detect server ID if not provided
detect_server_id() {
    if [[ -n "${MCP_SERVER_ID}" ]]; then
        log_info "Using provided server ID: ${MCP_SERVER_ID}"
        return 0
    fi

    log_warn "K6_MCP_SERVER_ID not set, attempting auto-detection..."

    # Try to generate JWT token
    local jwt_token=""
    if command -v python &> /dev/null; then
        jwt_token=$(python -m mcpgateway.utils.create_jwt_token \
            --username "${JWT_USERNAME}" \
            --exp 3600 \
            --secret "${JWT_SECRET}" 2>/dev/null || echo "")
    fi

    if [[ -z "${jwt_token}" ]]; then
        log_warn "Could not generate JWT token, auto-detection may fail"
        return 1
    fi

    # Query /servers endpoint
    local servers_response
    servers_response=$(curl -s \
        -H "Authorization: Bearer ${jwt_token}" \
        -H "Accept: application/json" \
        "${MCP_HOST}/servers" 2>/dev/null || echo "")

    if [[ -z "${servers_response}" ]]; then
        log_warn "Failed to query /servers endpoint"
        return 1
    fi

    # Try to extract server ID (simple parsing, requires jq for robust parsing)
    local server_id=""
    if command -v jq &> /dev/null; then
        server_id=$(echo "${servers_response}" | \
            jq -r 'if type=="array" then .[0].id elif .items then .items[0].id elif .servers then .servers[0].id else empty end' 2>/dev/null || echo "")
    fi

    if [[ -n "${server_id}" ]]; then
        MCP_SERVER_ID="${server_id}"
        log_success "Auto-detected server ID: ${MCP_SERVER_ID}"
        return 0
    else
        log_warn "Auto-detection failed, please set K6_MCP_SERVER_ID"
        return 1
    fi
}

# Check gateway health
check_gateway_health() {
    log_info "Checking gateway health at ${MCP_HOST}..."

    local health_response
    health_response=$(curl -s \
        -w "\n%{http_code}" \
        "${MCP_HOST}/health" 2>/dev/null || echo "")

    local http_code
    http_code=$(echo "${health_response}" | tail -1)
    local body
    body=$(echo "${health_response}" | head -n -1)

    if [[ "${http_code}" == "200" ]]; then
        log_success "Gateway is healthy (HTTP ${http_code})"
        return 0
    else
        log_error "Gateway health check failed (HTTP ${http_code})"
        echo "Response: ${body}"
        return 1
    fi
}

# Run the benchmark
run_benchmark() {
    local profile="$1"
    local config="${PROFILES[$profile]}"
    local vus="${config%%:*}"
    local duration="${config##*:}"

    echo ""
    echo "========================================"
    echo "  MCP Tools Benchmark (k6)"
    echo "========================================"
    echo "  Profile:    ${profile}"
    echo "  VUs:        ${vus}"
    echo "  Duration:   ${duration}"
    echo "  Host:       ${MCP_HOST}"
    echo "  Server ID:  ${MCP_SERVER_ID:-auto-detect}"
    echo "========================================"
    echo ""

    # Set environment variables
    export K6_MCP_HOST="${MCP_HOST}"
    [[ -n "${MCP_SERVER_ID}" ]] && export K6_MCP_SERVER_ID="${MCP_SERVER_ID}"
    export K6_JWT_SECRET_KEY="${JWT_SECRET}"
    export K6_JWT_USERNAME="${JWT_USERNAME}"

    # Run k6
    k6 run \
        --vus "${vus}" \
        --duration "${duration}" \
        "${SCRIPT_DIR}/mcp-tools-benchmark.ts"

    local exit_code=$?

    echo ""
    if [[ ${exit_code} -eq 0 ]]; then
        log_success "Benchmark completed successfully!"
    else
        log_error "Benchmark failed with exit code ${exit_code}"
    fi

    return ${exit_code}
}

# Main
main() {
    # Check for help flag
    if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
        usage
    fi

    # Validate profile
    if [[ ! -v "PROFILES[$PROFILE]" ]]; then
        log_error "Unknown profile: ${PROFILE}"
        echo ""
        echo "Valid profiles: ${!PROFILES[*]}"
        echo ""
        usage
    fi

    # Run checks
    check_prereqs

    if ! check_gateway_health; then
        log_error "Gateway is not healthy, aborting benchmark"
        exit 1
    fi

    if ! detect_server_id; then
        log_warn "Proceeding without server ID, test may fail"
    fi

    # Run benchmark
    run_benchmark "${PROFILE}"
}

main "$@"
