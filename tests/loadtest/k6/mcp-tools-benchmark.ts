// -*- coding: utf-8 -*-
/**
 * MCP Tools Benchmark - k6 Load Test Script
 *
 * Equivalent to `make benchmark-mcp-tools` but using k6 instead of Locust.
 * Tests ONLY the MCP Streamable HTTP transport path through the gateway.
 *
 * Test Scenario:
 *   - MCPToolCallerUser: Heavy tool/call in tight loop (weight 5)
 *   - Discovers tools via MCP tools/list during initialization
 *   - Calls tools/call rapidly with proper arguments
 *
 * Usage:
 *   # Quick test (10 VUs, 30s)
 *   k6 run tests/loadtest/k6/mcp-tools-benchmark.ts
 *
 *   # Production benchmark (125 VUs, 60s)
 *   k6 run --vus 125 --duration 60s tests/loadtest/k6/mcp-tools-benchmark.ts
 *
 *   # With custom server ID
 *   K6_MCP_SERVER_ID=7acd24c38b784628bce6e0bf96256008 k6 run tests/loadtest/k6/mcp-tools-benchmark.ts
 *
 *   # Stressed test (300 VUs, 5min)
 *   k6 run --vus 300 --duration 300s tests/loadtest/k6/mcp-tools-benchmark.ts
 *
 * Environment Variables:
 *   K6_MCP_HOST:              Gateway URL (default: http://localhost:8080)
 *   K6_MCP_SERVER_ID:         Virtual server UUID (required, no default)
 *   K6_MCP_TOOL_NAMES:        Comma-separated tool names (optional, auto-detected if empty)
 *   K6_JWT_SECRET_KEY:        JWT signing secret (default: my-test-key-but-now-longer-than-32-bytes)
 *   K6_JWT_ALGORITHM:         JWT algorithm (default: HS256)
 *   K6_JWT_AUDIENCE:          JWT audience (default: mcpgateway-api)
 *   K6_JWT_ISSUER:            JWT issuer (default: mcpgateway)
 *   K6_JWT_USERNAME:          Admin email (default: admin@example.com)
 *   K6_BEARER_TOKEN:          Pre-generated token (optional, overrides JWT generation)
 *   K6_LOG_LEVEL:             Log level (default: info)
 *
 * Copyright 2025
 * SPDX-License-Identifier: Apache-2.0
 */

import { check, sleep } from 'k6';
import http from 'k6/http';
import { Rate, Trend, Counter } from 'k6/metrics';
import { b64encode } from 'k6/encoding';
import crypto from 'k6/crypto';

// =============================================================================
// Configuration
// =============================================================================

const config = {
  // MCP Target
  host: __ENV.K6_MCP_HOST || 'http://localhost:8080',
  serverId: __ENV.K6_MCP_SERVER_ID || '',
  toolNamesStr: __ENV.K6_MCP_TOOL_NAMES || '',

  // JWT / Auth
  jwtSecretKey: __ENV.K6_JWT_SECRET_KEY || 'my-test-key-but-now-longer-than-32-bytes',
  jwtAlgorithm: __ENV.K6_JWT_ALGORITHM || 'HS256',
  jwtAudience: __ENV.K6_JWT_AUDIENCE || 'mcpgateway-api',
  jwtIssuer: __ENV.K6_JWT_ISSUER || 'mcpgateway',
  jwtUsername: __ENV.K6_JWT_USERNAME || 'admin@example.com',
  bearerToken: __ENV.K6_BEARER_TOKEN || '',

  // Timing
  minWait: 0.02,
  maxWait: 0.1,

  // Timezones for time tool
  timezones: [
    'UTC',
    'America/New_York',
    'America/Los_Angeles',
    'Europe/London',
    'Europe/Paris',
    'Asia/Tokyo',
    'Asia/Shanghai',
    'Australia/Sydney',
  ],
};

// =============================================================================
// Custom Metrics
// =============================================================================

// Success/failure rates
const toolCallSuccessRate = new Rate('tool_call_success');
const toolListSuccessRate = new Rate('tool_list_success');

// Timing metrics
const toolCallDuration = new Trend('tool_call_duration_ms');
const toolListDuration = new Trend('tool_list_duration_ms');

// Counters
const toolCallsTotal = new Counter('tool_calls_total');
const toolListsTotal = new Counter('tool_lists_total');
const toolsDiscovered = new Counter('tools_discovered');

// RPS tracking
const toolCallRequests = new Counter('tool_call_requests');
const toolListRequests = new Counter('tool_list_requests');

// =============================================================================
// JWT Token Generation
// =============================================================================

/**
 * Generate a JWT token matching gateway expectations.
 * Uses k6 crypto module to create HS256 signed tokens.
 */
function generateJwtToken(): string {
  const now = Math.floor(Date.now() / 1000);
  const exp = now + 8760 * 3600; // 1 year
  const jtiBytes = crypto.randomBytes(16);
  const jti = Array.from(jtiBytes).map(b => b.toString(16).padStart(2, '0')).join('');

  const payload = {
    sub: config.jwtUsername,
    exp: exp,
    iat: now,
    aud: config.jwtAudience,
    iss: config.jwtIssuer,
    jti: jti,
    token_use: 'session',
    user: {
      email: config.jwtUsername,
      full_name: 'K6 MCP Load Test',
      is_admin: true,
      auth_provider: 'local',
    },
  };

  // Create JWT header
  const header = {
    alg: config.jwtAlgorithm,
    typ: 'JWT',
  };

  // Base64url encode header and payload
  const headerEncoded = b64encode(JSON.stringify(header), 'utf-8')
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
    .replace(/=+$/, '');

  const payloadEncoded = b64encode(JSON.stringify(payload), 'utf-8')
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
    .replace(/=+$/, '');

  // Create signature
  const message = `${headerEncoded}.${payloadEncoded}`;
  const signature = crypto.hmac('sha256', config.jwtSecretKey, message, 'base64')
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
    .replace(/=+$/, '');

  return `${headerEncoded}.${payloadEncoded}.${signature}`;
}

/**
 * Get or generate JWT token (cached for test duration).
 */
let cachedToken: string | null = null;

function getToken(): string {
  if (config.bearerToken) {
    return config.bearerToken;
  }
  if (!cachedToken) {
    cachedToken = generateJwtToken();
  }
  return cachedToken;
}

// =============================================================================
// Auto-Detection: Server ID, Tool Names
// =============================================================================

interface ServerTarget {
  serverId: string;
  serverName: string;
  toolNames: string[];
}

let serverTarget: ServerTarget | null = null;

/**
 * Initialize server target from environment variables.
 * No auto-detection - requires K6_MCP_SERVER_ID to be set.
 */
function initializeTarget(): void {
  const token = getToken();
  const headers = {
    Authorization: `Bearer ${token}`,
    Accept: 'application/json',
    'Content-Type': 'application/json',
  };

  // Require server ID to be set
  if (!config.serverId) {
    console.error(
      'K6_MCP_SERVER_ID is required. Set it via environment variable.'
    );
    return;
  }

  const mcpUrl = `${config.host}/servers/${config.serverId}/mcp`;
  let sessionId: string | null = null;
  let serverName = config.serverId;

  // Helper for MCP JSON-RPC calls
  function mcpCall(method: string, params?: Record<string, unknown>): any | null {
    const idBytes = crypto.randomBytes(16);
    const id = Array.from(idBytes).map(b => b.toString(16).padStart(2, '0')).join('');
    
    const payload: any = {
      jsonrpc: '2.0',
      id: id,
      method: method,
    };
    if (params) {
      payload.params = params;
    }

    const callHeaders: any = { ...headers };
    if (sessionId) {
      callHeaders['Mcp-Session-Id'] = sessionId;
    }

    const resp = http.post(mcpUrl, JSON.stringify(payload), {
      headers: callHeaders,
      timeout: '15s',
    });

    // Capture session ID from response headers
    if (resp.headers && resp.headers['Mcp-Session-Id']) {
      sessionId = resp.headers['Mcp-Session-Id'] as string;
    }

    if (resp.status !== 200) {
      console.warn(`MCP ${method} failed with status ${resp.status}`);
      return null;
    }

    const data = resp.json();
    if (data.error) {
      console.warn(`MCP error for ${method}:`, data.error);
      return null;
    }

    return data.result || null;
  }

  // Initialize session
  const initResult = mcpCall('initialize', {
    protocolVersion: '2024-11-05',
    capabilities: {},
    clientInfo: { name: 'k6-mcp-benchmark', version: '1.0' },
  });

  if (initResult?.serverInfo?.name) {
    serverName = initResult.serverInfo.name;
  }

  // Get tool names
  let toolNames: string[] = [];
  if (config.toolNamesStr) {
    toolNames = config.toolNamesStr.split(',').map((t) => t.trim()).filter((t) => t);
  } else {
    const toolsResult = mcpCall('tools/list');
    if (toolsResult?.tools) {
      toolNames = toolsResult.tools
        .filter((t: { name: string }) => t.name)
        .map((t: { name: string }) => t.name);
    }
  }

  serverTarget = {
    serverId: config.serverId,
    serverName: serverName,
    toolNames: toolNames,
  };

  if (serverTarget.toolNames.length > 0) {
    toolsDiscovered.add(serverTarget.toolNames.length);
    console.info(
      `Server: ${serverTarget.serverId} name=${serverName} tools=${toolNames.length}`
    );
  } else {
    console.warn(`No tools found for server ${serverTarget.serverId}`);
  }
}

// =============================================================================
// Test Execution
// =============================================================================

/**
 * Get default arguments for a tool based on its name.
 */
function getDefaultToolArgs(toolName: string): Record<string, string> {
  const nameLower = toolName.toLowerCase();

  if (nameLower.includes('time')) {
    const tz = config.timezones[Math.floor(Math.random() * config.timezones.length)];
    return { timezone: tz };
  }

  if (nameLower.includes('echo')) {
    return { message: 'perf-test' };
  }

  if (nameLower.includes('convert')) {
    return {
      time: '2025-01-01T00:00:00Z',
      source_timezone: 'UTC',
      target_timezone: 'Europe/London',
    };
  }

  return {};
}

/**
 * MCP JSON-RPC request helper.
 */
function mcpRequest(
  method: string,
  params?: Record<string, unknown>,
  sessionId?: string | null,
  targetServerId?: string
): { result: any | null; sessionId: string | null; success: boolean; duration: number; rawBody?: string } {
  const token = getToken();
  const idBytes = crypto.randomBytes(16);
  const id = Array.from(idBytes).map(b => b.toString(16).padStart(2, '0')).join('');

  const payload: any = {
    jsonrpc: '2.0',
    id: id,
    method: method,
  };
  if (params) {
    payload.params = params;
  }

  const headers: any = {
    Authorization: `Bearer ${token}`,
    'Content-Type': 'application/json',
    'Accept': 'application/json',
  };
  if (sessionId) {
    headers['Mcp-Session-Id'] = sessionId;
  }

  const serverId = targetServerId || serverTarget?.serverId || config.serverId;
  const mcpUrl = `${config.host}/servers/${serverId}/mcp`;

  const startTime = Date.now();
  const resp = http.post(mcpUrl, JSON.stringify(payload), {
    headers,
    timeout: '15s',
  });
  const duration = Date.now() - startTime;

  let newSessionId: string | null = sessionId;
  if (resp.headers && resp.headers['Mcp-Session-Id']) {
    newSessionId = resp.headers['Mcp-Session-Id'] as string;
  }

  if (resp.status !== 200) {
    return { result: null, sessionId: newSessionId, success: false, duration, rawBody: resp.body };
  }

  const data = resp.json();
  if (data.error) {
    return { result: null, sessionId: newSessionId, success: false, duration, rawBody: resp.body };
  }

  return { result: data.result || null, sessionId: newSessionId, success: true, duration, rawBody: resp.body };
}

// =============================================================================
// VU Code
// =============================================================================

let vuSessionId: string | null = null;

export function setup() {
  initializeTarget();
  return { 
    initialized: true,
    toolNames: serverTarget?.toolNames || [],
    serverId: serverTarget?.serverId || '',
  };
}

export default function (data: { toolNames: string[]; serverId: string }) {
  const toolNames = data?.toolNames || serverTarget?.toolNames || [];
  const serverId = data?.serverId || serverTarget?.serverId || '';
  
  if (toolNames.length === 0) {
    console.error('No tools available to call. Check K6_MCP_SERVER_ID.');
    sleep(1);
    return;
  }

  // MCPToolCallerUser behavior: 20x call_tool, 1x list_tools
  const action = Math.random();

  if (action < 20 / 21) {
    // Call a tool (weight 20)
    const toolName =
      toolNames.length <= 6
        ? toolNames[Math.floor(Math.random() * toolNames.length)]
        : toolNames[Math.floor(Math.random() * 6)];

    const args = getDefaultToolArgs(toolName);

    const { success, duration, rawBody } = mcpRequest('tools/call', {
      name: toolName,
      arguments: args,
    }, vuSessionId, serverId);

    toolCallsTotal.add(1);
    toolCallRequests.add(1);
    toolCallSuccessRate.add(success ? 1 : 0);
    toolCallDuration.add(duration);

    if (!success) {
      console.debug(`Tool call failed: ${toolName}`);
    }
  } else {
    // List tools (weight 1)
    const { success, duration, sessionId } = mcpRequest('tools/list', {}, null, serverId);

    toolListsTotal.add(1);
    toolListRequests.add(1);
    toolListSuccessRate.add(success ? 1 : 0);
    toolListDuration.add(duration);

    // Capture session ID if returned
    if (success && sessionId && !vuSessionId) {
      vuSessionId = sessionId;
    }

    if (!success) {
      console.debug('Tool list failed');
    }
  }

  // Wait between requests (simulates user think time)
  sleep(config.minWait + Math.random() * (config.maxWait - config.minWait));
}

// =============================================================================
// Options
// =============================================================================

export const options = {
  // No thresholds - just collect metrics
  summaryTrendStats: ['avg', 'min', 'med', 'max', 'p(50)', 'p(90)', 'p(95)', 'p(99)'],
};

// =============================================================================
// Handle Summary
// =============================================================================

export function handleSummary(data: any) {
  return {
    stdout: formatSummary(data),
  };
}

function formatSummary(data: any): string {
  // Calculate test duration from k6 metrics
  // Use the duration from the test execution timestamp
  const testStart = data.root_group?.start ? new Date(data.root_group.start).getTime() : Date.now();
  const testEnd = data.root_group?.end ? new Date(data.root_group.end).getTime() : Date.now();
  const testDurationSec = (testEnd - testStart) / 1000 || 60;

  // Calculate RPS
  const toolCallCount = data.metrics.tool_calls_total?.values.count || 0;
  const toolListCount = data.metrics.tool_lists_total?.values.count || 0;
  const totalCount = toolCallCount + toolListCount;
  const overallRps = totalCount / testDurationSec;
  const toolCallRps = toolCallCount / testDurationSec;
  const toolListRps = toolListCount / testDurationSec;

  const summary = [
    '\n========================================',
    'MCP Tools Benchmark Summary',
    '========================================',
    '',
    `Total Requests: ${totalCount}`,
    `Total Tool Calls: ${toolCallCount}`,
    `Total Tool Lists: ${toolListCount}`,
    `Tools Discovered: ${data.metrics.tools_discovered?.values.count || 0}`,
    '',
    'Throughput (RPS):',
    `  Overall: ${overallRps.toFixed(2)}`,
    `  Tool Call: ${toolCallRps.toFixed(2)}`,
    `  Tool List: ${toolListRps.toFixed(2)}`,
    '',
    'Success Rates:',
    `  Tool Call: ${(data.metrics.tool_call_success?.values.rate || 0) * 100}%`,
    `  Tool List: ${(data.metrics.tool_list_success?.values.rate || 0) * 100}%`,
    '',
    'Tool Call Duration (ms):',
    `  Avg: ${data.metrics.tool_call_duration_ms?.values.avg?.toFixed(2) || 'N/A'}`,
    `  Min: ${data.metrics.tool_call_duration_ms?.values.min?.toFixed(2) || 'N/A'}`,
    `  p50: ${data.metrics.tool_call_duration_ms?.values['p(50)']?.toFixed(2) || 'N/A'}`,
    `  p95: ${data.metrics.tool_call_duration_ms?.values['p(95)']?.toFixed(2) || 'N/A'}`,
    `  p99: ${data.metrics.tool_call_duration_ms?.values['p(99)']?.toFixed(2) || 'N/A'}`,
    '',
    'Tool List Duration (ms):',
    `  Avg: ${data.metrics.tool_list_duration_ms?.values.avg?.toFixed(2) || 'N/A'}`,
    `  Min: ${data.metrics.tool_list_duration_ms?.values.min?.toFixed(2) || 'N/A'}`,
    `  p50: ${data.metrics.tool_list_duration_ms?.values['p(50)']?.toFixed(2) || 'N/A'}`,
    `  p95: ${data.metrics.tool_list_duration_ms?.values['p(95)']?.toFixed(2) || 'N/A'}`,
    `  p99: ${data.metrics.tool_list_duration_ms?.values['p(99)']?.toFixed(2) || 'N/A'}`,
    '',
    '========================================',
  ];

  return summary.join('\n');
}
