/**
 * Grafana Cloud Loki client for OpsAgent.
 * Queries logs from Grafana Cloud Loki to enrich RCA prompts.
 * Uses only Node.js built-ins (https/http) — no extra dependencies.
 */
import * as fs from 'fs';
import * as https from 'https';
import * as http from 'http';
import * as url from 'url';
import * as yaml from 'js-yaml';

export interface GrafanaConfig {
  loki_url: string;
  user_id: string;
  api_token: string;
}

export interface LokiLogEntry {
  timestamp: string;
  line: string;
}

export interface LokiQueryResult {
  logs: LokiLogEntry[];
  total: number;
}

// ── Config loader ─────────────────────────────────────────────

/**
 * Load Grafana Cloud credentials.
 * Priority: environment variables > telemetry.yaml > null (not configured).
 */
export function loadGrafanaConfig(telemetryConfigPath?: string): GrafanaConfig | null {
  const envUrl = process.env.GRAFANA_LOKI_URL;
  const envUser = process.env.GRAFANA_USER_ID;
  const envToken = process.env.GRAFANA_API_TOKEN;
  if (envUrl && envUser && envToken) {
    return { loki_url: envUrl, user_id: envUser, api_token: envToken };
  }

  if (telemetryConfigPath && fs.existsSync(telemetryConfigPath)) {
    try {
      const content = fs.readFileSync(telemetryConfigPath, 'utf-8');
      const cfg = yaml.load(content) as any;
      // telemetry.yaml nests under telemetry.grafana
      const g = cfg?.telemetry?.grafana ?? cfg?.grafana;
      if (g?.enabled !== false && g?.loki_url && g?.user_id && g?.api_token) {
        return { loki_url: g.loki_url, user_id: g.user_id, api_token: g.api_token };
      }
    } catch {
      // ignore parse errors
    }
  }

  return null;
}

// ── LogQL helpers ─────────────────────────────────────────────

/**
 * Build a simple LogQL stream selector from a label dict.
 * e.g. { job: "rca-demo", cluster: "prod" } -> '{job="rca-demo", cluster="prod"}'
 */
export function buildLogqlQuery(labels: Record<string, string>): string {
  const selectors = Object.entries(labels)
    .map(([k, v]) => `${k}="${v}"`)
    .join(', ');
  return `{${selectors}}`;
}

// ── Loki query ────────────────────────────────────────────────

/**
 * Query Grafana Cloud Loki using the query_range endpoint.
 *
 * @param config    Grafana credentials
 * @param logql     LogQL query string
 * @param startMs   Range start in milliseconds (epoch)
 * @param endMs     Range end in milliseconds (epoch)
 * @param limit     Max number of log lines to return (default 100)
 */
export function queryLokiLogs(
  config: GrafanaConfig,
  logql: string,
  startMs: number,
  endMs: number,
  limit = 100,
): Promise<LokiQueryResult> {
  const credentials = Buffer.from(`${config.user_id}:${config.api_token}`).toString('base64');
  const parsedUrl = new url.URL(config.loki_url);
  const isHttps = parsedUrl.protocol === 'https:';

  const params = new URLSearchParams({
    query: logql,
    start: String(startMs * 1_000_000),   // Loki expects nanoseconds
    end:   String(endMs   * 1_000_000),
    limit: String(limit),
    direction: 'backward',
  });

  const reqPath =
    `${parsedUrl.pathname.replace(/\/$/, '')}/loki/api/v1/query_range?${params}`;

  return new Promise((resolve, reject) => {
    const options = {
      hostname: parsedUrl.hostname,
      port: parsedUrl.port || (isHttps ? 443 : 80),
      path: reqPath,
      method: 'GET',
      headers: {
        Authorization: `Basic ${credentials}`,
        Accept: 'application/json',
      },
    };

    const transport: typeof https = isHttps ? https : (http as any);
    const req = transport.request(options, (res) => {
      let data = '';
      res.on('data', (chunk: Buffer) => { data += chunk; });
      res.on('end', () => {
        if (res.statusCode !== 200) {
          reject(new Error(`Loki query HTTP ${res.statusCode}: ${data.substring(0, 200)}`));
          return;
        }
        try {
          const parsed = JSON.parse(data);
          const logs: LokiLogEntry[] = [];
          for (const stream of parsed.data?.result ?? []) {
            for (const [tsNs, line] of stream.values ?? []) {
              const tsMs = Math.floor(parseInt(tsNs, 10) / 1_000_000);
              logs.push({ timestamp: new Date(tsMs).toISOString(), line });
            }
          }
          logs.sort((a, b) => a.timestamp.localeCompare(b.timestamp));
          resolve({ logs, total: logs.length });
        } catch (e: any) {
          reject(new Error(`Failed to parse Loki response: ${e.message}`));
        }
      });
    });

    req.on('error', reject);
    req.setTimeout(10_000, () => {
      req.destroy(new Error('Loki query timed out after 10s'));
    });
    req.end();
  });
}

/**
 * Format LokiQueryResult as a markdown code block for inclusion in RCA prompts.
 */
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
