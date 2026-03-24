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
import { TenantResolver } from './tenant-resolver';
import { ApprovalStore } from './approval-store';
import { normalizeAlert } from './alert-webhook';
import { createIssue, saveIssueToDB } from './patrol';
import { buildRcaPrompt } from './rca';
import { SessionStore, loadUsers, saveUsers, verifyPassword, hashPassword, parseCookie, setSessionCookie, clearSessionCookie, UserRole } from './auth';

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
const TENANTS_CONFIG = process.env.TENANTS_CONFIG || seedConfig('tenants.yaml');
const USERS_CONFIG = process.env.USERS_CONFIG || seedConfig('users.yaml');

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
  tenantsConfigPath: TENANTS_CONFIG,
});
const auditLogger = new AuditLogger();
const sessionStore = new SessionStore();
const tenantResolver = new TenantResolver(TENANTS_CONFIG);
const approvalStore = new ApprovalStore(path.join(CONFIG_DIR, 'approvals.json'));
claudeClient.approvalStore = approvalStore;

// ── Config validation on startup ──────────────────────────────
function validateConfigs(): void {
  const yaml = require('js-yaml');
  const configs: Array<{ name: string; path: string; required: string }> = [
    { name: 'glossary', path: GLOSSARY_CONFIG, required: 'glossary' },
    { name: 'accounts', path: ACCOUNTS_CONFIG, required: 'accounts' },
    { name: 'platforms', path: PLATFORMS_CONFIG, required: 'platforms' },
    { name: 'tenants', path: TENANTS_CONFIG, required: 'tenants' },
    { name: 'providers', path: PROVIDERS_CONFIG, required: 'provider' },
    { name: 'skills', path: SKILLS_CONFIG, required: 'skills' },
  ];
  for (const cfg of configs) {
    if (!fs.existsSync(cfg.path)) continue;
    try {
      const content = fs.readFileSync(cfg.path, 'utf-8');
      const data = yaml.load(content);
      if (data && typeof data !== 'object') {
        console.error(`[config] WARNING: ${cfg.name}.yaml is not a valid YAML object`);
      }
    } catch (err) {
      console.error(`[config] ERROR: ${cfg.name}.yaml is malformed: ${(err as Error).message}`);
    }
  }
}
validateConfigs();

// Rate limiting: login attempts per username
const loginAttempts = new Map<string, { count: number; first: number }>();

// Rate limiting: max concurrent chat queries per user
const MAX_CONCURRENT_QUERIES = 2;
const activeQueries = new Map<string, number>();
function acquireQuerySlot(userId: string): boolean {
  const current = activeQueries.get(userId) || 0;
  if (current >= MAX_CONCURRENT_QUERIES) return false;
  activeQueries.set(userId, current + 1);
  return true;
}
function releaseQuerySlot(userId: string): void {
  const current = activeQueries.get(userId) || 0;
  if (current <= 1) activeQueries.delete(userId);
  else activeQueries.set(userId, current - 1);
}

function isUserAuthEnabled(): boolean {
  const config = loadUsers(USERS_CONFIG);
  return config !== null && config.users.length > 0;
}
const messageHandler = new MessageHandler(claudeClient, auditLogger, tenantResolver);

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
  tenantsConfigPath: TENANTS_CONFIG,
  usersConfigPath: USERS_CONFIG,
  kubeconfigManager,
  onScheduledJobsChanged: () => scheduler.reload(),
  onTenantsChanged: () => tenantResolver.reload(),
});
adminApi.approvalStore = approvalStore;

// ── Approval execution callbacks ──────────────────────────────
adminApi.onApprovalApproved = async (approvalId: string) => {
  const approval = approvalStore.get(approvalId);
  if (!approval) return;
  console.log(`[index] Executing approved command #${approvalId}: ${approval.command}`);
  try {
    const { execFile } = require('child_process');
    const result: string = await new Promise((resolve, reject) => {
      execFile('/bin/sh', ['-c', approval.command], {
        cwd: WORK_DIR, timeout: 180_000, maxBuffer: 100 * 1024,
        env: process.env,
      }, (err: any, stdout: string, stderr: string) => {
        if (err) reject(new Error(stderr || err.message));
        else resolve(stdout || '(no output)');
      });
    });
    approvalStore.markExecuted(approvalId, result);
    // Notify user via IM if possible
    const adapter = adapters.get(approval.platform);
    if (adapter?.sendToChannel && approval.channelId) {
      adapter.sendToChannel(approval.channelId,
        `✅ 审批 #${approvalId} 已通过并执行完成:\n命令: ${approval.command}\n结果: ${result.substring(0, 500)}\n批准人: ${approval.resolvedBy}`,
      );
    }
  } catch (err: any) {
    approvalStore.markFailed(approvalId, err.message);
    const adapter = adapters.get(approval.platform);
    if (adapter?.sendToChannel && approval.channelId) {
      adapter.sendToChannel(approval.channelId,
        `❌ 审批 #${approvalId} 已批准但执行失败:\n命令: ${approval.command}\n错误: ${err.message}`,
      );
    }
  }
};

adminApi.onApprovalRejected = (approvalId: string) => {
  const approval = approvalStore.get(approvalId);
  if (!approval) return;
  const adapter = adapters.get(approval.platform);
  if (adapter?.sendToChannel && approval.channelId) {
    adapter.sendToChannel(approval.channelId,
      `🚫 审批 #${approvalId} 已被拒绝:\n命令: ${approval.command}\n拒绝人: ${approval.resolvedBy}${approval.rejectReason ? '\n原因: ' + approval.rejectReason : ''}`,
    );
  }
};

// HTTP request body parser (max 10MB)
const MAX_BODY_SIZE = 10 * 1024 * 1024;
function parseBody(req: http.IncomingMessage): Promise<string> {
  return new Promise((resolve, reject) => {
    let data = '';
    let size = 0;
    req.on('data', (chunk: Buffer) => {
      size += chunk.length;
      if (size > MAX_BODY_SIZE) {
        req.destroy();
        reject(new Error('Request body too large'));
        return;
      }
      data += chunk.toString();
    });
    req.on('end', () => resolve(data));
    req.on('error', () => resolve(''));
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
    const checks: Record<string, string> = {};
    // Check CLAUDE.md exists
    checks.claude_md = fs.existsSync(path.join(WORK_DIR, 'CLAUDE.md')) ? 'ok' : 'missing';
    // Check config dir
    checks.config = fs.existsSync(CONFIG_DIR) ? 'ok' : 'missing';
    // Check tenants config
    checks.tenants = fs.existsSync(TENANTS_CONFIG) ? 'ok' : 'missing';
    const allOk = Object.values(checks).every(v => v === 'ok');
    res.writeHead(allOk ? 200 : 503, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ status: allOk ? 'ok' : 'degraded', platforms: enabledPlatforms, checks }));
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
    // File upload endpoint for admin chat — must be handled BEFORE parseBody consumes the stream
    if (url === '/admin/api/upload' && req.method === 'POST') {
      let fileName = decodeURIComponent(req.headers['x-file-name'] as string || 'upload');
      // Sanitize filename: strip path components to prevent directory traversal
      fileName = path.basename(fileName).replace(/[^a-zA-Z0-9._-]/g, '_');
      if (!fileName || fileName.startsWith('.')) fileName = 'upload';
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

    let body: any;
    if (req.method === 'PUT' || req.method === 'POST') {
      const rawBody = await parseBody(req);
      if (rawBody) {
        try {
          body = JSON.parse(rawBody);
        } catch {
          res.writeHead(400, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ error: 'Invalid JSON' }));
          return;
        }
      }
    }
    // Handle DELETE with optional body
    if (req.method === 'DELETE') {
      const rawBody = await parseBody(req);
      if (rawBody) {
        try { body = JSON.parse(rawBody); } catch { /* no body needed for DELETE */ }
      }
    }

    // ── Auth endpoints (always accessible) ──────────────────────
    if (url === '/admin/api/auth/login' && req.method === 'POST') {
      if (!body?.username || !body?.password) {
        res.writeHead(400, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: 'username and password are required' }));
        return;
      }
      // Rate limiting: max 5 failed attempts per username per 15 minutes
      const loginKey = `login:${body.username}`;
      const attempts = (loginAttempts.get(loginKey) || { count: 0, first: Date.now() });
      if (Date.now() - attempts.first > 15 * 60 * 1000) {
        attempts.count = 0;
        attempts.first = Date.now();
      }
      if (attempts.count >= 5) {
        res.writeHead(429, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: 'Too many login attempts. Try again in 15 minutes.' }));
        return;
      }
      const usersConfig = loadUsers(USERS_CONFIG);
      if (!usersConfig) {
        res.writeHead(500, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: 'User authentication not configured' }));
        return;
      }
      const user = usersConfig.users.find(u => u.username === body.username);
      if (!user || !(await verifyPassword(body.password, user.password_hash))) {
        attempts.count++;
        loginAttempts.set(loginKey, attempts);
        res.writeHead(401, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: 'Invalid username or password' }));
        return;
      }
      // Reset on successful login
      loginAttempts.delete(loginKey);
      const token = sessionStore.create({ username: user.username, role: user.role, tenant_id: user.tenant_id });
      setSessionCookie(res, token);
      console.log(`[index] User "${user.username}" logged in (${user.role})`);
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ ok: true, user: { username: user.username, role: user.role, tenant_id: user.tenant_id } }));
      return;
    }

    if (url === '/admin/api/auth/logout' && req.method === 'POST') {
      const token = parseCookie(req.headers.cookie || '');
      if (token) sessionStore.destroy(token);
      clearSessionCookie(res);
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ ok: true }));
      return;
    }

    if (url === '/admin/api/auth/me' && req.method === 'GET') {
      if (!isUserAuthEnabled()) {
        // No user auth → backward compat mode, no user info
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ mode: 'api_key' }));
        return;
      }
      const token = parseCookie(req.headers.cookie || '');
      const session = token ? sessionStore.get(token) : null;
      if (!session) {
        res.writeHead(401, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: 'Not authenticated' }));
        return;
      }
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ user: { username: session.username, role: session.role, tenant_id: session.tenant_id } }));
      return;
    }

    // Change own password (any authenticated user)
    if (url === '/admin/api/auth/change-password' && req.method === 'PUT') {
      const token = parseCookie(req.headers.cookie || '');
      const session = token ? sessionStore.get(token) : null;
      if (!session) {
        res.writeHead(401, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: 'Not authenticated' }));
        return;
      }
      if (!body?.current_password || !body?.new_password) {
        res.writeHead(400, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: 'current_password and new_password are required' }));
        return;
      }
      const usersConfig = loadUsers(USERS_CONFIG);
      const user = usersConfig?.users.find(u => u.username === session.username);
      if (!user || !(await verifyPassword(body.current_password, user.password_hash))) {
        res.writeHead(401, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: 'Current password is incorrect' }));
        return;
      }
      user.password_hash = await hashPassword(body.new_password);
      saveUsers(USERS_CONFIG, usersConfig!);
      console.log(`[index] User "${session.username}" changed their password`);
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ ok: true }));
      return;
    }

    // ── Session enforcement (when user auth is enabled) ──────────
    let authUser: { username: string; role: UserRole; tenant_id?: string } | null = null;
    if (isUserAuthEnabled()) {
      const token = parseCookie(req.headers.cookie || '');
      const session = token ? sessionStore.get(token) : null;
      if (!session) {
        res.writeHead(401, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: 'Unauthorized' }));
        return;
      }
      authUser = { username: session.username, role: session.role, tenant_id: session.tenant_id };
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
      let tenantId: string | undefined;

      if (req.method === 'GET') {
        // GET: message in query param (for EventSource)
        const parsedUrl = new URL(url, `http://${req.headers.host}`);
        message = parsedUrl.searchParams.get('message') || undefined;
        files = parsedUrl.searchParams.get('files') || undefined;
        tenantId = parsedUrl.searchParams.get('tenantId') || undefined;
      } else {
        message = body?.message;
        files = body?.files;
        tenantId = body?.tenantId;
      }

      // Enforce tenant scope for tenant_admin users (cannot override)
      if (authUser?.role === 'tenant_admin' && authUser.tenant_id) {
        tenantId = authUser.tenant_id;
      }

      // Inject file paths into query if present
      if (files && message) {
        try {
          const fileList = JSON.parse(files) as Array<{ filePath: string; fileName: string }>;
          if (fileList.length > 0) {
            // Sanitize: only allow paths under /tmp/opsagent-uploads, strip any control chars
            const safeList = fileList.filter(f =>
              f.filePath && f.filePath.startsWith('/tmp/opsagent-uploads/') && !f.filePath.includes('..')
            );
            if (safeList.length > 0) {
              const desc = safeList.map(f => `- ${f.fileName.replace(/[\n\r]/g, '')}: ${f.filePath}`).join('\n');
              message += `\n\n用户附带了以下文件，请用 Read 工具查看并分析:\n${desc}`;
            }
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

      const queryUser = authUser?.username || 'admin';
      if (!acquireQuerySlot(queryUser)) {
        sendSSE('error', { error: 'Too many concurrent queries. Please wait for the current query to finish.' });
        res.end();
        return;
      }

      console.log(`[index] Admin chat stream${tenantId ? ` [tenant=${tenantId}]` : ''}: ${message.substring(0, 100)}`);

      try {
        for await (const chunk of claudeClient.queryStream(message, 'admin', 'admin', tenantId)) {
          sendSSE(chunk.type, chunk);
        }
      } catch (err: any) {
        console.error(`[index] Admin chat stream error: ${err.message}`);
        sendSSE('error', { error: err.message });
      } finally {
        releaseQuerySlot(queryUser);
      }
      res.end();
      return;
    }

    const handled = await adminApi.handleRequest(req, res, url, body, authUser);
    if (handled) return;
  }

  if (req.method === 'POST') {
    let rawBody: string;
    try {
      rawBody = await parseBody(req);
    } catch {
      res.writeHead(413);
      res.end('Request body too large');
      return;
    }
    // Store raw body on request for Slack signature verification
    (req as any)._rawBody = rawBody;
    let body: any;
    try {
      body = JSON.parse(rawBody);
    } catch {
      res.writeHead(400);
      res.end('Invalid JSON');
      return;
    }

    // Alert webhook: POST /api/alerts
    if (url === '/api/alerts') {
      const alert = normalizeAlert(body);
      if (!alert) {
        res.writeHead(400, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: 'Unrecognized alert format' }));
        return;
      }
      console.log(`[index] Alert received: ${alert.source} — ${alert.title} (${alert.severity})`);

      // Create issue from alert
      const issue = createIssue({
        resource_id: alert.resource_id,
        resource_type: alert.resource_type,
        severity: alert.severity,
        source: `alert:${alert.source}`,
        title: alert.title,
        description: alert.description,
        metric_data: alert.metric_data,
      });

      // Try to save to DB (fire-and-forget if DB not available)
      try {
        const saved = await saveIssueToDB(issue);
        console.log(`[index] Issue #${saved.id} created from alert`);

        // Trigger RCA in background
        const rcaPrompt = buildRcaPrompt({
          issueId: saved.id, title: saved.title, resource_id: saved.resource_id || '',
          severity: saved.severity, description: saved.description || '',
          regions: [],
        });
        claudeClient.query(rcaPrompt, 'alert', 'system').catch(err => {
          console.error(`[index] Background RCA failed: ${err.message}`);
        });

        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ ok: true, issue_id: saved.id, source: alert.source }));
      } catch (dbErr: any) {
        console.warn(`[index] DB unavailable, alert logged but not persisted: ${dbErr.message}`);
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ ok: true, persisted: false, source: alert.source }));
      }
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

// ── Graceful shutdown ─────────────────────────────────────────
let shuttingDown = false;
function gracefulShutdown(signal: string) {
  if (shuttingDown) return;
  shuttingDown = true;
  console.log(`[index] ${signal} received, shutting down gracefully...`);

  server.close(() => {
    console.log('[index] HTTP server closed');
    scheduler.stop();
    process.exit(0);
  });

  // Force exit after 30s if connections don't drain
  setTimeout(() => {
    console.error('[index] Graceful shutdown timeout, forcing exit');
    process.exit(1);
  }, 30000);
}

process.on('SIGTERM', () => gracefulShutdown('SIGTERM'));
process.on('SIGINT', () => gracefulShutdown('SIGINT'));

// ── Global error handlers ─────────────────────────────────────
process.on('unhandledRejection', (reason) => {
  console.error('[index] Unhandled promise rejection:', reason);
});

process.on('uncaughtException', (err) => {
  console.error('[index] Uncaught exception:', err);
  // Exit on uncaught exception — process is in unknown state
  gracefulShutdown('uncaughtException');
});
