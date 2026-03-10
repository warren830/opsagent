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

const PORT = parseInt(process.env.PORT || '3978', 10);
const WORK_DIR = process.env.WORK_DIR || path.resolve(__dirname, '../..');
const PLUGINS_CONFIG = process.env.PLUGINS_CONFIG || path.join(WORK_DIR, 'config/plugins.yaml');
const MCP_CONFIG = process.env.MCP_CONFIG || path.join(WORK_DIR, 'config/mcp.json');
const PLATFORMS_CONFIG = process.env.PLATFORMS_CONFIG || path.join(WORK_DIR, 'config/platforms.yaml');
const GLOSSARY_CONFIG = process.env.GLOSSARY_CONFIG || path.join(WORK_DIR, 'config/glossary.yaml');
const ACCOUNTS_CONFIG = process.env.ACCOUNTS_CONFIG || path.join(WORK_DIR, 'config/accounts.yaml');
const SKILLS_CONFIG = process.env.SKILLS_CONFIG || path.join(WORK_DIR, 'config/skills.yaml');
const KNOWLEDGE_DIR = process.env.KNOWLEDGE_DIR || path.join(WORK_DIR, 'knowledge');
const STATIC_DIR = path.join(__dirname, '../static');

// Core components
const claudeClient = new ClaudeClient({
  workDir: WORK_DIR,
  pluginsConfigPath: PLUGINS_CONFIG,
  mcpConfigPath: MCP_CONFIG,
});
const auditLogger = new AuditLogger();
const messageHandler = new MessageHandler(claudeClient, auditLogger);

// Admin API
const adminApi = new AdminApi({
  glossaryConfigPath: GLOSSARY_CONFIG,
  accountsConfigPath: ACCOUNTS_CONFIG,
  platformsConfigPath: PLATFORMS_CONFIG,
  skillsConfigPath: SKILLS_CONFIG,
  knowledgeDir: KNOWLEDGE_DIR,
});

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
  const adapter = adapters.get(platform);
  if (!adapter) {
    res.writeHead(404);
    res.end(`Platform "${platform}" is not configured`);
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
    // Chat endpoint
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
