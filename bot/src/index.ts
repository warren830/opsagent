import * as http from 'http';
import * as fs from 'fs';
import * as path from 'path';
import { PlatformAdapter } from './adapters/types';
import { TeamsAdapter } from './adapters/teams';
import { SlackAdapter } from './adapters/slack';
import { FeishuAdapter } from './adapters/feishu';
import { MessageHandler } from './message-handler';
import { ClaudeClient } from './claude-client';
import { AuditLogger } from './audit-logger';
import { loadPlatforms, isPlatformEnabled, getPlatformSettings, getPlatformCredentials } from './platform-loader';
import { AdminApi } from './admin-api';
import { SchedulerManager } from './scheduler';
import { KubeconfigManager } from './kubeconfig-manager';

const PORT = parseInt(process.env.PORT || '3978', 10);
const WORK_DIR = process.env.WORK_DIR || path.resolve(__dirname, '../..');
const KNOWLEDGE_DIR = process.env.KNOWLEDGE_DIR || path.join(WORK_DIR, 'knowledge');
const STATIC_DIR = path.join(__dirname, '../static');

// Admin-editable configs are stored on EFS (knowledge/_config/) so they persist across deploys.
// On first boot, seed from bundled config/ in the image.
const CONFIG_DIR = path.join(KNOWLEDGE_DIR, '_config');
const BUNDLED_CONFIG_DIR = path.join(WORK_DIR, 'config');

function seedConfig(filename: string): string {
  const efsPath = path.join(CONFIG_DIR, filename);
  if (!fs.existsSync(CONFIG_DIR)) {
    fs.mkdirSync(CONFIG_DIR, { recursive: true });
  }
  if (!fs.existsSync(efsPath)) {
    const bundled = path.join(BUNDLED_CONFIG_DIR, filename);
    if (fs.existsSync(bundled)) {
      fs.copyFileSync(bundled, efsPath);
      console.log(`[index] Seeded config: ${filename}`);
    }
  }
  return efsPath;
}

const PLUGINS_CONFIG = process.env.PLUGINS_CONFIG || seedConfig('plugins.yaml');
const MCP_CONFIG = process.env.MCP_CONFIG || path.join(WORK_DIR, 'config/mcp.json');
const PLATFORMS_CONFIG = process.env.PLATFORMS_CONFIG || seedConfig('platforms.yaml');
const GLOSSARY_CONFIG = process.env.GLOSSARY_CONFIG || seedConfig('glossary.yaml');
const ACCOUNTS_CONFIG = process.env.ACCOUNTS_CONFIG || seedConfig('accounts.yaml');
const SKILLS_CONFIG = process.env.SKILLS_CONFIG || seedConfig('skills.yaml');
const SCHEDULED_JOBS_CONFIG = process.env.SCHEDULED_JOBS_CONFIG || seedConfig('scheduled-jobs.yaml');
const PROVIDERS_CONFIG = process.env.PROVIDERS_CONFIG || seedConfig('providers.yaml');
const CLUSTERS_CONFIG = process.env.CLUSTERS_CONFIG || seedConfig('clusters.yaml');

// Core components
const claudeClient = new ClaudeClient({
  workDir: WORK_DIR,
  pluginsConfigPath: PLUGINS_CONFIG,
  mcpConfigPath: MCP_CONFIG,
  glossaryConfigPath: GLOSSARY_CONFIG,
  accountsConfigPath: ACCOUNTS_CONFIG,
  skillsConfigPath: SKILLS_CONFIG,
  knowledgeDir: KNOWLEDGE_DIR,
  providerConfigPath: PROVIDERS_CONFIG,
});
const auditLogger = new AuditLogger();
const messageHandler = new MessageHandler(claudeClient, auditLogger);

// Scheduler (initialized after adapters are created below)
let scheduler: SchedulerManager;

// Admin API (initialized after adapters are created below)
let adminApi: AdminApi;

// Load platform configuration
const platformsConfig = loadPlatforms(PLATFORMS_CONFIG);

// Register platform adapters based on config + available env vars
const adapters = new Map<string, PlatformAdapter>();

// Teams
if (isPlatformEnabled(platformsConfig, 'teams')) {
  const creds = getPlatformCredentials(platformsConfig, 'teams');
  adapters.set('teams', new TeamsAdapter({ appId: creds.app_id || '', appPassword: creds.app_password || '' }));
  console.log(`[index] Teams adapter enabled${creds.app_id ? '' : ' (no credentials)'}`);
}

// Slack
if (isPlatformEnabled(platformsConfig, 'slack')) {
  const creds = getPlatformCredentials(platformsConfig, 'slack');
  const settings = getPlatformSettings(platformsConfig, 'slack');
  if (creds.bot_token && creds.signing_secret) {
    const allowedChannels: string[] = settings?.allowed_channels || [];
    adapters.set('slack', new SlackAdapter({
      botToken: creds.bot_token,
      signingSecret: creds.signing_secret,
      allowedChannels: allowedChannels.length > 0 ? allowedChannels : undefined,
    }));
    console.log('[index] Slack adapter enabled');
  } else {
    console.log('[index] Slack platform enabled but missing credentials (bot_token / signing_secret)');
  }
}

// Feishu
if (isPlatformEnabled(platformsConfig, 'feishu')) {
  const creds = getPlatformCredentials(platformsConfig, 'feishu');
  if (creds.app_id && creds.app_secret) {
    adapters.set('feishu', new FeishuAdapter({
      appId: creds.app_id,
      appSecret: creds.app_secret,
      verificationToken: creds.verification_token || '',
    }));
    console.log('[index] Feishu adapter enabled');
  } else {
    console.log('[index] Feishu platform enabled but missing credentials (app_id / app_secret)');
  }
}

// Initialize scheduler
scheduler = new SchedulerManager(adapters, claudeClient, SCHEDULED_JOBS_CONFIG);
scheduler.start();

// Initialize KubeconfigManager (async — runs in background, does not block startup)
const kubeconfigManager = new KubeconfigManager(CLUSTERS_CONFIG, ACCOUNTS_CONFIG, KNOWLEDGE_DIR);
kubeconfigManager.initialize().catch(err => {
  console.error(`[index] KubeconfigManager initialization failed: ${err}`);
});

// Initialize Admin API
adminApi = new AdminApi({
  glossaryConfigPath: GLOSSARY_CONFIG,
  accountsConfigPath: ACCOUNTS_CONFIG,
  platformsConfigPath: PLATFORMS_CONFIG,
  skillsConfigPath: SKILLS_CONFIG,
  knowledgeDir: KNOWLEDGE_DIR,
  scheduledJobsConfigPath: SCHEDULED_JOBS_CONFIG,
  pluginsConfigPath: PLUGINS_CONFIG,
  providerConfigPath: PROVIDERS_CONFIG,
  clustersConfigPath: CLUSTERS_CONFIG,
  kubeconfigManager,
  onScheduledJobsChanged: () => scheduler.reload(),
});

// HTTP request body parser
function parseBody(req: http.IncomingMessage): Promise<string> {
  return new Promise((resolve) => {
    let data = '';
    req.on('data', (chunk: Buffer) => { data += chunk.toString(); });
    req.on('end', () => resolve(data));
  });
}

// Route: POST /api/messages/:platform
async function handlePlatformMessage(
  platform: string,
  req: http.IncomingMessage,
  res: http.ServerResponse,
  body: any,
): Promise<void> {
  // Handle Feishu/Slack URL verification regardless of adapter registration,
  // so webhook URL can be configured before credentials are fully set up.
  if (body?.type === 'url_verification' && body.challenge) {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ challenge: body.challenge }));
    return;
  }

  const adapter = adapters.get(platform);
  if (!adapter) {
    res.writeHead(404, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ error: `Platform "${platform}" is not configured` }));
    return;
  }

  try {
    const msg = await adapter.handleWebhook(req, res, body);
    if (msg) {
      // Process asynchronously so we don't block the webhook response
      messageHandler.handleMessage(adapter, msg).catch((err) => {
        console.error(`[index] Error handling ${platform} message: ${err}`);
      });
    }
  } catch (error) {
    console.error(`[index] Error processing ${platform} activity: ${error}`);
    if (!res.writableEnded) {
      res.writeHead(500);
      res.end('Internal Server Error');
    }
  }
}

// HTTP server
const server = http.createServer(async (req, res) => {
  const url = req.url || '';

  if (req.method === 'GET' && url === '/health') {
    const enabledPlatforms = Array.from(adapters.keys());
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ status: 'ok', platforms: enabledPlatforms }));
    return;
  }

  // Admin UI
  if (req.method === 'GET' && (url === '/admin' || url === '/admin/')) {
    const htmlPath = path.join(STATIC_DIR, 'admin.html');
    if (fs.existsSync(htmlPath)) {
      res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
      res.end(fs.readFileSync(htmlPath, 'utf-8'));
    } else {
      res.writeHead(404);
      res.end('Admin UI not found');
    }
    return;
  }

  // Admin API
  if (url.startsWith('/admin/api/')) {
    let body: any;
    if (req.method === 'PUT' || req.method === 'POST') {
      const rawBody = await parseBody(req);
      try {
        body = JSON.parse(rawBody);
      } catch {
        res.writeHead(400, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: 'Invalid JSON' }));
        return;
      }
    }
    // Handle DELETE with optional body
    if (req.method === 'DELETE') {
      const rawBody = await parseBody(req);
      if (rawBody) {
        try { body = JSON.parse(rawBody); } catch { /* no body needed for DELETE */ }
      }
    }
    // File upload endpoint for admin chat (multipart-like: raw binary with headers)
    if (url === '/admin/api/upload' && req.method === 'POST') {
      const fileName = decodeURIComponent(req.headers['x-file-name'] as string || 'upload');
      const dir = path.join('/tmp/opsagent-uploads', `admin-${Date.now()}`);
      fs.mkdirSync(dir, { recursive: true });
      const filePath = path.join(dir, fileName);

      const chunks: Buffer[] = [];
      req.on('data', (chunk: Buffer) => chunks.push(chunk));
      req.on('end', () => {
        fs.writeFileSync(filePath, Buffer.concat(chunks));
        console.log(`[index] Admin upload: ${fileName} -> ${filePath}`);
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ filePath, fileName }));
      });
      return;
    }

    // Chat endpoint (non-streaming, kept for backward compatibility)
    if (url === '/admin/api/chat' && req.method === 'POST') {
      if (!body?.message) {
        res.writeHead(400, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: '"message" field is required' }));
        return;
      }
      try {
        console.log(`[index] Admin chat query: ${body.message.substring(0, 100)}`);
        const reply = await claudeClient.query(body.message, 'admin', 'admin');
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ reply: reply || '' }));
      } catch (err: any) {
        console.error(`[index] Admin chat error: ${err.message}`);
        res.writeHead(500, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: err.message }));
      }
      return;
    }

    // Chat endpoint (streaming via SSE) — supports both POST and GET
    if (url.startsWith('/admin/api/chat/stream') && (req.method === 'POST' || req.method === 'GET')) {
      let message: string | undefined;

      let files: string | undefined;

      if (req.method === 'GET') {
        // GET: message in query param (for EventSource)
        const parsedUrl = new URL(url, `http://${req.headers.host}`);
        message = parsedUrl.searchParams.get('message') || undefined;
        files = parsedUrl.searchParams.get('files') || undefined;
      } else {
        message = body?.message;
        files = body?.files;
      }

      // Inject file paths into query if present
      if (files && message) {
        try {
          const fileList = JSON.parse(files) as Array<{ filePath: string; fileName: string }>;
          if (fileList.length > 0) {
            const desc = fileList.map(f => `- ${f.fileName}: ${f.filePath}`).join('\n');
            message += `\n\n用户附带了以下文件，请用 Read 工具查看并分析:\n${desc}`;
          }
        } catch { /* ignore bad JSON */ }
      }

      if (!message) {
        res.writeHead(400, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: '"message" parameter is required' }));
        return;
      }

      res.writeHead(200, {
        'Content-Type': 'text/event-stream',
        'Cache-Control': 'no-cache',
        'Connection': 'keep-alive',
        'X-Accel-Buffering': 'no',
      });

      const sendSSE = (event: string, data: unknown) => {
        res.write(`event: ${event}\ndata: ${JSON.stringify(data)}\n\n`);
      };

      console.log(`[index] Admin chat stream: ${message.substring(0, 100)}`);

      try {
        for await (const chunk of claudeClient.queryStream(message, 'admin', 'admin')) {
          sendSSE(chunk.type, chunk);
        }
      } catch (err: any) {
        console.error(`[index] Admin chat stream error: ${err.message}`);
        sendSSE('error', { error: err.message });
      }
      res.end();
      return;
    }

    const handled = await adminApi.handleRequest(req, res, url, body);
    if (handled) return;
  }

  if (req.method === 'POST') {
    const rawBody = await parseBody(req);
    let body: any;
    try {
      body = JSON.parse(rawBody);
    } catch {
      res.writeHead(400);
      res.end('Invalid JSON');
      return;
    }

    // Platform-specific routes
    const routeMatch = url.match(/^\/api\/messages\/(\w+)$/);
    if (routeMatch) {
      await handlePlatformMessage(routeMatch[1], req, res, body);
      return;
    }

    // Legacy route: /api/messages -> teams (backward compatible)
    if (url === '/api/messages') {
      await handlePlatformMessage('teams', req, res, body);
      return;
    }
  }

  res.writeHead(404);
  res.end('Not Found');
});

server.listen(PORT, () => {
  console.log(`[index] OpsAgent Bot listening on port ${PORT}`);
  console.log(`[index] Endpoints:`);
  console.log(`[index]   Health:  GET  http://localhost:${PORT}/health`);
  console.log(`[index]   Teams:   POST http://localhost:${PORT}/api/messages/teams`);
  console.log(`[index]   Slack:   POST http://localhost:${PORT}/api/messages/slack`);
  console.log(`[index]   Feishu:  POST http://localhost:${PORT}/api/messages/feishu`);
  console.log(`[index]   Legacy:  POST http://localhost:${PORT}/api/messages`);
  console.log(`[index]   Admin:   GET  http://localhost:${PORT}/admin`);
});
