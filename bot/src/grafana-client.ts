/**
 * Grafana Cloud client for OpsAgent RCA enrichment.
 *
 * Supports three signal sources:
 *   - Loki  (Logs)    — query_range API
 *   - Tempo (Traces)  — search + span lookup API
 *   - Mimir (Metrics) — Prometheus-compatible query_range API
 *
 * All calls use Basic Auth with the Grafana Cloud instance ID + API token.
 * Uses only Node.js built-ins — no extra dependencies.
 */
import * as fs from 'fs';
import * as https from 'https';
import * as http from 'http';
import * as url from 'url';
import * as yaml from 'js-yaml';

// ── Config types ──────────────────────────────────────────────

export interface GrafanaConfig {
  loki_url:   string;
  tempo_url:  string;
  mimir_url:  string;
  user_id:    string;       // legacy fallback for all endpoints
  loki_user_id:  string;   // Loki instance ID  (Basic Auth username)
  tempo_user_id: string;   // Tempo instance ID
  mimir_user_id: string;   // Prometheus/Mimir instance ID
  api_token:  string;
}

// ── Loki types ────────────────────────────────────────────────

export interface LokiLogEntry {
  timestamp: string;
  line: string;
}

export interface LokiQueryResult {
  logs: LokiLogEntry[];
  total: number;
}

// ── Tempo types ───────────────────────────────────────────────

export interface TempoSpan {
  traceId:   string;
  spanId:    string;
  name:      string;
  service:   string;
  startMs:   number;
  durationMs: number;
  status:    string;       // 'ok' | 'error' | 'unset'
  attributes: Record<string, string>;
}

export interface TempoTraceResult {
  traceId: string;
  spans:   TempoSpan[];
  rootSpan?: TempoSpan;
}

// ── Mimir types ───────────────────────────────────────────────

export interface MimirSample {
  timestamp: string;
  value: number;
}

export interface MimirSeriesResult {
  metric: Record<string, string>;
  samples: MimirSample[];
}

// ── Config loader ─────────────────────────────────────────────

/**
 * Load Grafana Cloud credentials.
 * Priority: environment variables > telemetry.yaml > null (not configured).
 *
 * Env vars:
 *   GRAFANA_LOKI_URL, GRAFANA_TEMPO_URL, GRAFANA_MIMIR_URL
 *   GRAFANA_USER_ID (instance ID, same for all endpoints)
 *   GRAFANA_API_TOKEN
 */
export function loadGrafanaConfig(telemetryConfigPath?: string): GrafanaConfig | null {
  const envLoki  = process.env.GRAFANA_LOKI_URL;
  const envTempo = process.env.GRAFANA_TEMPO_URL;
  const envMimir = process.env.GRAFANA_MIMIR_URL;
  const envUser  = process.env.GRAFANA_USER_ID;
  const envToken = process.env.GRAFANA_API_TOKEN;

  if (envUser && envToken) {
    return {
      loki_url:      envLoki  || '',
      tempo_url:     envTempo || '',
      mimir_url:     envMimir || '',
      user_id:       envUser,
      loki_user_id:  process.env.GRAFANA_LOKI_USER_ID  || envUser,
      tempo_user_id: process.env.GRAFANA_TEMPO_USER_ID || envUser,
      mimir_user_id: process.env.GRAFANA_MIMIR_USER_ID || envUser,
      api_token:     envToken,
    };
  }

  if (telemetryConfigPath && fs.existsSync(telemetryConfigPath)) {
    try {
      const content = fs.readFileSync(telemetryConfigPath, 'utf-8');
      const cfg = yaml.load(content) as any;
      const g = cfg?.telemetry?.grafana ?? cfg?.grafana;
      const baseUserId = g?.user_id ? String(g.user_id) : '';
      if (g?.enabled !== false && (baseUserId || g?.loki_user_id) && g?.api_token) {
        return {
          loki_url:      g.loki_url  || '',
          tempo_url:     g.tempo_url || '',
          mimir_url:     g.mimir_url || '',
          user_id:       baseUserId,
          loki_user_id:  String(g.loki_user_id  || baseUserId),
          tempo_user_id: String(g.tempo_user_id || baseUserId),
          mimir_user_id: String(g.mimir_user_id || baseUserId),
          api_token:     g.api_token,
        };
      }
    } catch {
      // ignore parse errors
    }
  }

  return null;
}

// ── Shared HTTP helper ────────────────────────────────────────

function grafanaGet(
  baseUrl: string,
  path: string,
  userId: string,
  apiToken: string,
  timeoutMs = 10_000,
): Promise<any> {
  const credentials = Buffer.from(`${userId}:${apiToken}`).toString('base64');
  const parsedUrl = new url.URL(baseUrl);
  const isHttps = parsedUrl.protocol === 'https:';
  const fullPath = parsedUrl.pathname.replace(/\/$/, '') + path;

  return new Promise((resolve, reject) => {
    const options = {
      hostname: parsedUrl.hostname,
      port:     parsedUrl.port || (isHttps ? 443 : 80),
      path:     fullPath,
      method:   'GET',
      headers: {
        Authorization: `Basic ${credentials}`,
        Accept:        'application/json',
      },
    };
    const transport: typeof https = isHttps ? https : (http as any);
    const req = transport.request(options, (res) => {
      let data = '';
      res.on('data', (chunk: Buffer) => { data += chunk; });
      res.on('end', () => {
        if ((res.statusCode ?? 0) >= 400) {
          reject(new Error(`HTTP ${res.statusCode}: ${data.substring(0, 200)}`));
          return;
        }
        try { resolve(JSON.parse(data)); }
        catch (e: any) { reject(new Error(`JSON parse error: ${e.message}`)); }
      });
    });
    req.on('error', reject);
    req.setTimeout(timeoutMs, () => req.destroy(new Error(`Timeout after ${timeoutMs}ms`)));
    req.end();
  });
}

// ── LogQL helper ──────────────────────────────────────────────

/**
 * Build a LogQL stream selector from a label dict.
 * { job: "rca-demo", cluster: "prod" } → '{job="rca-demo",cluster="prod"}'
 */
export function buildLogqlQuery(labels: Record<string, string>): string {
  const selectors = Object.entries(labels)
    .map(([k, v]) => `${k}="${v}"`)
    .join(',');
  return `{${selectors}}`;
}

// ── Loki: query logs ──────────────────────────────────────────

export async function queryLokiLogs(
  config: GrafanaConfig,
  logql: string,
  startMs: number,
  endMs: number,
  limit = 50,
): Promise<LokiQueryResult> {
  if (!config.loki_url) throw new Error('Loki URL not configured');

  const params = new URLSearchParams({
    query:     logql,
    start:     String(startMs * 1_000_000),  // Loki expects nanoseconds
    end:       String(endMs   * 1_000_000),
    limit:     String(limit),
    direction: 'backward',
  });

  const parsed = await grafanaGet(
    config.loki_url,
    `/loki/api/v1/query_range?${params}`,
    config.loki_user_id || config.user_id,
    config.api_token,
  );

  const logs: LokiLogEntry[] = [];
  for (const stream of parsed.data?.result ?? []) {
    for (const [tsNs, line] of stream.values ?? []) {
      const tsMs = Math.floor(parseInt(tsNs, 10) / 1_000_000);
      logs.push({ timestamp: new Date(tsMs).toISOString(), line });
    }
  }
  logs.sort((a, b) => a.timestamp.localeCompare(b.timestamp));
  return { logs, total: logs.length };
}

export function formatLogsForPrompt(result: LokiQueryResult, maxLines = 50): string {
  if (result.total === 0) return '';
  const lines = result.logs.slice(-maxLines)
    .map(l => `[${l.timestamp}] ${l.line}`)
    .join('\n');
  return [
    '',
    `### Grafana Cloud Loki 相关日志（最近1小时，共 ${result.total} 条）`,
    '',
    '```',
    lines,
    '```',
    '',
    '请优先分析上述日志中的 ERROR / Exception / timeout 信息，结合日志时序确定根因。',
    '',
  ].join('\n');
}

// ── Tempo: query traces ───────────────────────────────────────

/**
 * Search Tempo for recent error traces by service name.
 * Returns the most recent error trace ID, or null if none found.
 */
export async function searchTempoErrorTrace(
  config: GrafanaConfig,
  serviceName: string,
  startMs: number,
  endMs: number,
  limit = 5,
): Promise<string | null> {
  if (!config.tempo_url) throw new Error('Tempo URL not configured');

  // Don't filter by error=true — many spans don't have this tag set even when status=ERROR.
  // Instead fetch recent traces and inspect span status in getTempoTrace().
  const params = new URLSearchParams({
    service: serviceName,
    start:   String(Math.floor(startMs / 1000)),  // Tempo: Unix seconds
    end:     String(Math.floor(endMs   / 1000)),
    limit:   String(limit),
  });

  const parsed = await grafanaGet(
    config.tempo_url,
    `/api/search?${params}`,
    config.tempo_user_id || config.user_id,
    config.api_token,
  );

  const traces = parsed.traces ?? [];
  if (traces.length === 0) return null;
  // Return most recent
  return traces[0].traceID as string;
}

/**
 * Fetch all spans for a given trace ID from Tempo.
 */
export async function getTempoTrace(
  config: GrafanaConfig,
  traceId: string,
): Promise<TempoTraceResult> {
  if (!config.tempo_url) throw new Error('Tempo URL not configured');

  const parsed = await grafanaGet(
    config.tempo_url,
    `/api/traces/${traceId}`,
    config.tempo_user_id || config.user_id,
    config.api_token,
  );

  const spans: TempoSpan[] = [];
  for (const batch of parsed.batches ?? []) {
    const serviceName = batch.resource?.attributes
      ?.find((a: any) => a.key === 'service.name')?.value?.stringValue ?? 'unknown';

    for (const scopeSpan of batch.scopeSpans ?? []) {
      for (const s of scopeSpan.spans ?? []) {
        const durationMs = Math.round(
          (parseInt(s.endTimeUnixNano, 10) - parseInt(s.startTimeUnixNano, 10)) / 1_000_000,
        );
        const attrs: Record<string, string> = {};
        for (const a of s.attributes ?? []) {
          attrs[a.key] = a.value?.stringValue ?? String(a.value?.intValue ?? a.value?.boolValue ?? '');
        }
        const statusCode = s.status?.code ?? 0; // 2 = ERROR in OTel
        spans.push({
          traceId:    traceId,
          spanId:     s.spanId,
          name:       s.name,
          service:    serviceName,
          startMs:    Math.floor(parseInt(s.startTimeUnixNano, 10) / 1_000_000),
          durationMs,
          status:     statusCode === 2 ? 'error' : statusCode === 1 ? 'ok' : 'unset',
          attributes: attrs,
        });
      }
    }
  }

  spans.sort((a, b) => a.startMs - b.startMs);
  const rootSpan = spans.find(s => !spans.some(p => p.spanId === (s as any).parentSpanId));

  return { traceId, spans, rootSpan };
}

export function formatTraceForPrompt(result: TempoTraceResult): string {
  if (result.spans.length === 0) return '';

  const errorSpans = result.spans.filter(s => s.status === 'error');
  const lines: string[] = [
    '',
    `### Grafana Cloud Tempo 错误 Trace（traceId: ${result.traceId}）`,
    '',
    `共 ${result.spans.length} 个 Span，其中 ${errorSpans.length} 个报错：`,
    '',
  ];

  for (const s of result.spans) {
    const flag   = s.status === 'error' ? '❌' : '✅';
    const errMsg = s.attributes['error.message'] ?? s.attributes['exception.message'] ?? '';
    lines.push(
      `${flag} [${new Date(s.startMs).toISOString()}] ${s.service} / ${s.name}` +
      ` (${s.durationMs}ms)` +
      (errMsg ? ` — ${errMsg}` : ''),
    );
  }

  lines.push('', '请结合 Trace 中的调用链和耗时，分析哪个服务/Span 是根因。', '');
  return lines.join('\n');
}

// ── Mimir: query metrics ──────────────────────────────────────

/**
 * Query a PromQL expression over a time range from Grafana Cloud Mimir.
 */
export async function queryMimirRange(
  config: GrafanaConfig,
  promql: string,
  startMs: number,
  endMs: number,
  stepSeconds = 60,
): Promise<MimirSeriesResult[]> {
  if (!config.mimir_url) throw new Error('Mimir URL not configured');

  const params = new URLSearchParams({
    query: promql,
    start: String(Math.floor(startMs / 1000)),
    end:   String(Math.floor(endMs   / 1000)),
    step:  String(stepSeconds),
  });

  // mimir_url already ends with /api/prom — just append /api/v1/query_range
  const parsed = await grafanaGet(
    config.mimir_url,
    `/api/v1/query_range?${params}`,
    config.mimir_user_id || config.user_id,
    config.api_token,
  );

  const results: MimirSeriesResult[] = [];
  for (const r of parsed.data?.result ?? []) {
    results.push({
      metric:  r.metric,
      samples: (r.values ?? []).map(([ts, v]: [number, string]) => ({
        timestamp: new Date(ts * 1000).toISOString(),
        value:     parseFloat(v),
      })),
    });
  }
  return results;
}

export function formatMetricsForPrompt(
  results: MimirSeriesResult[],
  metricName: string,
): string {
  if (results.length === 0) return '';

  const lines: string[] = [
    '',
    `### Grafana Cloud Mimir 指标：${metricName}`,
    '',
  ];

  for (const r of results) {
    const labelStr = Object.entries(r.metric)
      .filter(([k]) => k !== '__name__')
      .map(([k, v]) => `${k}="${v}"`)
      .join(', ');
    lines.push(`**{${labelStr}}**`);

    if (r.samples.length > 0) {
      const values   = r.samples.map(s => s.value);
      const maxVal   = Math.max(...values);
      const lastVal  = values[values.length - 1];
      const trend    = values.length > 1
        ? (values[values.length - 1] > values[0] ? '↑ 上升' : '↓ 下降')
        : '—';

      lines.push(`- 最近值: ${lastVal.toFixed(4)}, 峰值: ${maxVal.toFixed(4)}, 趋势: ${trend}`);

      // Show last 5 data points
      const recent = r.samples.slice(-5);
      lines.push('- 最近5个采样点:');
      for (const s of recent) {
        lines.push(`  [${s.timestamp}] ${s.value.toFixed(4)}`);
      }
    }
    lines.push('');
  }

  lines.push('请结合指标趋势判断故障是突发还是渐进，并与日志/Trace 时间对齐做综合分析。', '');
  return lines.join('\n');
}
