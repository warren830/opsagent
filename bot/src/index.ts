import 'dotenv/config';
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
import {
  loadGrafanaConfig,
  buildLogqlQuery,
  queryLokiLogs, formatLogsForPrompt,
  searchTempoErrorTrace, getTempoTrace, formatTraceForPrompt,
  queryMimirRange, formatMetricsForPrompt,
} from './grafana-client';
import { loadGithubRepos } from './github-repos-loader';
import { fetchSourceContext, formatSourceForPrompt } from './github-source';
import { SessionStore, loadUsers, saveUsers, verifyPassword, hashPassword, parseCookie, setSessionCookie, clearSessionCookie, UserRole } from './auth';
import { initSchema } from './db';
import { buildFeishuAuthUrl, buildTeamsAuthUrl, findUserByPlatformId, upsertOAuthUser } from './oauth';

const PORT = parseInt(process.env.PORT || '3978', 10);
const WORK_DIR = process.env.WORK_DIR || path.resolve(__dirname, '../..');
const KNOWLEDGE_DIR = process.env.KNOWLEDGE_DIR || path.join(WORK_DIR, 'knowledge');

/** Simple fetch wrapper for OAuth API calls. */
async function fetchJson(url: string, options: { method: string; headers: Record<string, string>; body?: string }): Promise<any> {
  const { default: https } = await import('https');
  return new Promise((resolve, reject) => {
    const parsedUrl = new URL(url);
    const req = https.request({
      hostname: parsedUrl.hostname,
      path: parsedUrl.pathname + parsedUrl.search,
      method: options.method,
      headers: options.headers,
    }, (res) => {
      let data = '';
      res.on('data', (chunk: Buffer) => { data += chunk; });
      res.on('end', () => {
        try { resolve(JSON.parse(data)); }
        catch { reject(new Error(`Invalid JSON response from ${url}`)); }
      });
    });
    req.on('error', reject);
    if (options.body) req.write(options.body);
    req.end();
  });
}
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
const TELEMETRY_CONFIG = process.env.TELEMETRY_CONFIG || seedConfig('telemetry.yaml');
const GITHUB_REPOS_CONFIG = process.env.GITHUB_REPOS_CONFIG || seedConfig('github-repos.yaml');

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
claudeClient.init();

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
const messageHandler = new MessageHandler(claudeClient, auditLogger, tenantResolver, USERS_CONFIG);

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
scheduler = new SchedulerManager(adapters, claudeClient, SCHEDULED_JOBS_CONFIG, KNOWLEDGE_DIR);
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
  telemetryConfigPath: TELEMETRY_CONFIG,
  githubReposConfigPath: GITHUB_REPOS_CONFIG,
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
  const url = (req.url || '').split('?')[0];

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
    res.writeHead(allOk ? 200 : 503, { 'Content-Type': 'application/json', 'Cache-Control': 'no-cache, no-store, must-revalidate' });
    res.end(JSON.stringify({ status: allOk ? 'ok' : 'degraded', platforms: enabledPlatforms, checks }));
    return;
  }

  // Admin UI
  if (req.method === 'GET' && (url === '/admin' || url === '/admin/')) {
    const htmlPath = path.join(STATIC_DIR, 'admin.html');
    if (fs.existsSync(htmlPath)) {
      res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8', 'Cache-Control': 'no-cache' });
      res.end(fs.readFileSync(htmlPath, 'utf-8'));
    } else {
      res.writeHead(404);
      res.end('Admin UI not found');
    }
    return;
  }

  // Admin API — disable caching for all API responses
  if (url.startsWith('/admin/api/')) {
    res.setHeader('Cache-Control', 'no-cache, no-store, must-revalidate');
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

    // ── OAuth login ──────────────────────────────────────────────
    if (url === '/admin/api/oauth/feishu' && req.method === 'GET') {
      const feishuAppId = platformsConfig?.platforms?.feishu?.credentials?.app_id;
      if (!feishuAppId) {
        res.writeHead(400, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: 'Feishu app not configured' }));
        return;
      }
      const state = require('crypto').randomUUID();
      const baseUrl = req.headers['x-forwarded-host'] || req.headers.host || 'localhost:3978';
      const proto = req.headers['x-forwarded-proto'] || 'https';
      const redirectUri = `${proto}://${baseUrl}/admin/api/oauth/feishu/callback`;
      const authUrl = buildFeishuAuthUrl(feishuAppId, redirectUri, state);
      res.writeHead(302, { Location: authUrl });
      res.end();
      return;
    }

    if (url?.startsWith('/admin/api/oauth/feishu/callback') && req.method === 'GET') {
      const urlObj = new URL(url, `http://${req.headers.host}`);
      const code = urlObj.searchParams.get('code');
      if (!code) {
        res.writeHead(400, { 'Content-Type': 'text/html' });
        res.end('<h1>OAuth Error</h1><p>No authorization code</p>');
        return;
      }
      try {
        // Exchange code for user info via Feishu API
        const feishuAdapter = adapters.get('feishu');
        const feishuAppId = platformsConfig?.platforms?.feishu?.credentials?.app_id;
        const feishuAppSecret = platformsConfig?.platforms?.feishu?.credentials?.app_secret;
        if (!feishuAppId || !feishuAppSecret) throw new Error('Feishu app not configured');

        const tokenRes = await fetchJson('https://open.feishu.cn/open-apis/authen/v1/oidc/access_token', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ grant_type: 'authorization_code', code, app_id: feishuAppId, app_secret: feishuAppSecret }),
        });
        if (tokenRes.code !== 0) throw new Error(`Feishu token error: ${tokenRes.msg}`);

        const userInfo = await fetchJson('https://open.feishu.cn/open-apis/authen/v1/user_info', {
          method: 'GET',
          headers: { Authorization: `Bearer ${tokenRes.data.access_token}` },
        });
        if (userInfo.code !== 0) throw new Error(`Feishu user info error: ${userInfo.msg}`);

        const openId = userInfo.data.open_id;
        const name = userInfo.data.name || 'Unknown';
        const email = userInfo.data.email || userInfo.data.enterprise_email || '';

        // Find or create user
        const user = upsertOAuthUser(USERS_CONFIG, {
          platform: 'feishu', platformId: openId, name, email,
        });

        // Create session
        const token = sessionStore.create({ username: user.username, role: user.role, tenant_id: user.tenant_id });
        setSessionCookie(res, token);
        console.log(`[index] OAuth login: "${user.username}" via Feishu (${openId})`);
        res.writeHead(302, { Location: '/admin' });
        res.end();
      } catch (err: any) {
        console.error(`[index] Feishu OAuth error: ${err.message}`);
        res.writeHead(500, { 'Content-Type': 'text/html' });
        res.end(`<h1>OAuth Error</h1><p>${err.message}</p><a href="/admin">Back</a>`);
      }
      return;
    }

    if (url === '/admin/api/oauth/teams' && req.method === 'GET') {
      const teamsClientId = platformsConfig?.platforms?.teams?.credentials?.client_id || process.env.TEAMS_CLIENT_ID;
      const teamsTenantId = platformsConfig?.platforms?.teams?.credentials?.tenant_id || process.env.TEAMS_TENANT_ID || 'common';
      if (!teamsClientId) {
        res.writeHead(400, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: 'Teams OAuth not configured' }));
        return;
      }
      const state = require('crypto').randomUUID();
      const baseUrl = req.headers['x-forwarded-host'] || req.headers.host || 'localhost:3978';
      const proto = req.headers['x-forwarded-proto'] || 'https';
      const redirectUri = `${proto}://${baseUrl}/admin/api/oauth/teams/callback`;
      const authUrl = buildTeamsAuthUrl(teamsClientId, redirectUri, state, teamsTenantId);
      res.writeHead(302, { Location: authUrl });
      res.end();
      return;
    }

    if (url?.startsWith('/admin/api/oauth/teams/callback') && req.method === 'GET') {
      const urlObj = new URL(url, `http://${req.headers.host}`);
      const code = urlObj.searchParams.get('code');
      if (!code) {
        res.writeHead(400, { 'Content-Type': 'text/html' });
        res.end('<h1>OAuth Error</h1><p>No authorization code</p>');
        return;
      }
      try {
        const teamsClientId = platformsConfig?.platforms?.teams?.credentials?.client_id || process.env.TEAMS_CLIENT_ID;
        const teamsClientSecret = platformsConfig?.platforms?.teams?.credentials?.client_secret || process.env.TEAMS_CLIENT_SECRET;
        const teamsTenantId = platformsConfig?.platforms?.teams?.credentials?.tenant_id || process.env.TEAMS_TENANT_ID || 'common';
        if (!teamsClientId || !teamsClientSecret) throw new Error('Teams OAuth not configured');

        const baseUrl = req.headers['x-forwarded-host'] || req.headers.host || 'localhost:3978';
        const proto = req.headers['x-forwarded-proto'] || 'https';
        const redirectUri = `${proto}://${baseUrl}/admin/api/oauth/teams/callback`;

        // Exchange code for tokens
        const tokenRes = await fetchJson(`https://login.microsoftonline.com/${teamsTenantId}/oauth2/v2.0/token`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
          body: new URLSearchParams({
            client_id: teamsClientId, client_secret: teamsClientSecret,
            code, redirect_uri: redirectUri, grant_type: 'authorization_code',
            scope: 'openid profile email User.Read',
          }).toString(),
        });
        if (tokenRes.error) throw new Error(`Teams token error: ${tokenRes.error_description || tokenRes.error}`);

        // Get user info from Microsoft Graph
        const graphRes = await fetchJson('https://graph.microsoft.com/v1.0/me', {
          method: 'GET',
          headers: { Authorization: `Bearer ${tokenRes.access_token}` },
        });

        const aadObjectId = graphRes.id;
        const name = graphRes.displayName || 'Unknown';
        const email = graphRes.mail || graphRes.userPrincipalName || '';

        const user = upsertOAuthUser(USERS_CONFIG, {
          platform: 'teams', platformId: aadObjectId, name, email,
        });

        const token = sessionStore.create({ username: user.username, role: user.role, tenant_id: user.tenant_id });
        setSessionCookie(res, token);
        console.log(`[index] OAuth login: "${user.username}" via Teams (${aadObjectId})`);
        res.writeHead(302, { Location: '/admin' });
        res.end();
      } catch (err: any) {
        console.error(`[index] Teams OAuth error: ${err.message}`);
        res.writeHead(500, { 'Content-Type': 'text/html' });
        res.end(`<h1>OAuth Error</h1><p>${err.message}</p><a href="/admin">Back</a>`);
      }
      return;
    }

    // ── SSE events stream — bypass session check (cookie sent by browser EventSource) ──
    if (url === '/admin/api/events' && req.method === 'GET') {
      const handled = await adminApi.handleRequest(req, res, url, undefined, null);
      if (handled) return;
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
        const queryUser = authUser?.username || 'admin';
        const reply = await claudeClient.query(body.message, 'admin', queryUser, authUser?.tenant_id, queryUser);
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
        for await (const chunk of claudeClient.queryStream(message, 'admin', queryUser, tenantId, queryUser)) {
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
        adminApi.pushEvent('issue_created', { id: saved.id, title: saved.title, severity: saved.severity, status: saved.status, created_at: saved.created_at });

        // ── Background RCA: multi-signal collection + Claude analysis ──
        (async () => {
          const rcaStart = Date.now();
          const grafanaCfg = loadGrafanaConfig(TELEMETRY_CONFIG);
          const alertLabels = alert.metric_data?.labels as Record<string, string> | undefined;
          const triggerType = alertLabels?.trigger_type ?? 'error';
          const serviceName = alertLabels?.service ?? alertLabels?.job ?? '';
          const clusterName = alertLabels?.cluster ?? '';
          const now = Date.now();
          const rangeStart = now - 60 * 60_000;  // last 1 hour

          // Helper: push a progress step to all SSE clients watching this issue
          const progress = (step: string, status: 'running' | 'done' | 'error', detail?: string) => {
            adminApi.pushEvent('rca_progress', { id: saved.id, step, status, detail: detail ?? '' });
          };

          let metricsContext = '';
          let traceContext   = '';
          let logsContext    = '';
          let kubectlContext = '';
          let sourceContext  = '';

          if (grafanaCfg) {
            // 1. Metrics (Mimir)
            if (serviceName && grafanaCfg.mimir_url) {
              progress('metrics', 'running', `查询 Mimir 指标：error rate、memory_usage、db_connections（服务: ${serviceName}）`);
              try {
                const errRateResults = await queryMimirRange(
                  grafanaCfg, `rate(orders_errors_total{service="${serviceName}"}[2m])`, rangeStart, now, 60,
                );
                const memResults = await queryMimirRange(
                  grafanaCfg, `memory_usage_ratio{service="${serviceName}"}`, rangeStart, now, 60,
                );
                const dbResults = await queryMimirRange(
                  grafanaCfg, `db_connections_active{service="${serviceName}"}`, rangeStart, now, 60,
                );
                metricsContext =
                  formatMetricsForPrompt(errRateResults, 'orders_errors_total (rate/2m)') +
                  formatMetricsForPrompt(memResults,    'memory_usage_ratio') +
                  formatMetricsForPrompt(dbResults,     'db_connections_active');

                // Extract key findings for the progress message
                const errPeak = errRateResults[0]?.samples.reduce((m, s) => s.value > m ? s.value : m, 0) ?? 0;
                const dbPeak  = dbResults[0]?.samples.reduce((m, s) => s.value > m ? s.value : m, 0) ?? 0;
                const memLast = memResults[0]?.samples.slice(-1)[0]?.value ?? 0;
                progress('metrics', 'done',
                  `错误率峰值 ${(errPeak * 60).toFixed(2)}/min，DB连接峰值 ${dbPeak.toFixed(0)}，内存用量 ${(memLast * 100).toFixed(1)}%`);
                console.log(`[index] Fetched Mimir metrics for RCA issue #${saved.id}`);
              } catch (e: any) {
                progress('metrics', 'error', e.message);
                console.warn(`[index] Mimir fetch failed: ${e.message}`);
              }
            }

            // 2. Traces (Tempo)
            if (serviceName && grafanaCfg.tempo_url) {
              progress('traces', 'running', `在 Tempo 中搜索 ${serviceName} 最近的错误 Trace`);
              try {
                const traceId = await searchTempoErrorTrace(grafanaCfg, serviceName, rangeStart, now, 3);
                if (traceId) {
                  const traceResult = await getTempoTrace(grafanaCfg, traceId);
                  traceContext = formatTraceForPrompt(traceResult);
                  const errorSpans = traceResult.spans.filter(s => s.status === 'error');
                  progress('traces', 'done',
                    `找到 Trace ${traceId.substring(0, 16)}...，共 ${traceResult.spans.length} 个 Span，${errorSpans.length} 个报错。` +
                    (errorSpans[0] ? `首个错误: ${errorSpans[0].name} (${errorSpans[0].durationMs}ms) — ${errorSpans[0].attributes['error.message'] ?? ''}` : ''));
                  console.log(`[index] Fetched Tempo trace ${traceId} for RCA issue #${saved.id}`);
                } else {
                  progress('traces', 'done', '未找到匹配的错误 Trace（时间窗口内无数据）');
                }
              } catch (e: any) {
                progress('traces', 'error', e.message);
                console.warn(`[index] Tempo fetch failed: ${e.message}`);
              }
            }

            // 3. Logs (Loki)
            if (grafanaCfg.loki_url) {
              const queryLabels: Record<string, string> = {};
              if (alertLabels?.job)     queryLabels.job     = alertLabels.job;
              if (alertLabels?.service) queryLabels.service = alertLabels.service;
              if (clusterName)          queryLabels.cluster = clusterName;
              if (Object.keys(queryLabels).length > 0) {
                progress('logs', 'running', `查询 Loki 日志：${JSON.stringify(queryLabels)}（最近1小时，最多100条）`);
                try {
                  const lokiResult = await queryLokiLogs(
                    grafanaCfg, buildLogqlQuery(queryLabels), rangeStart, now, 100,
                  );
                  logsContext = formatLogsForPrompt(lokiResult);
                  // Find error log lines for summary
                  const errorLogs = lokiResult.logs.filter(l => l.line.includes('"level":"error"') || l.line.includes('"level": "error"'));
                  const lastError = errorLogs[errorLogs.length - 1];
                  let errorSummary = '';
                  if (lastError) {
                    try {
                      const parsed = JSON.parse(lastError.line);
                      errorSummary = `最新错误: [${lastError.timestamp}] ${parsed.message ?? parsed.msg ?? lastError.line.substring(0, 120)}`;
                    } catch { errorSummary = lastError.line.substring(0, 120); }
                  }
                  progress('logs', 'done',
                    `获取 ${lokiResult.total} 条日志，其中 ${errorLogs.length} 条错误。${errorSummary}`);
                  console.log(`[index] Fetched ${lokiResult.total} Loki logs for RCA issue #${saved.id}`);
                } catch (e: any) {
                  progress('logs', 'error', e.message);
                  console.warn(`[index] Loki fetch failed: ${e.message}`);
                }
              }
            }
          }

          // 4. kubectl — Pod/Node resource state
          if (clusterName || serviceName) {
            const namespace = alertLabels?.namespace ?? 'rca';
            progress('kubectl', 'running', `kubectl get pods/top/events -n ${namespace}（集群: ${clusterName || 'current-context'}）`);
            try {
              const { execSync } = require('child_process') as typeof import('child_process');
              const nsFlag = `-n ${namespace}`;
              const lsFlag = serviceName ? `-l app=${serviceName}` : '';

              const podLines   = execSync(`kubectl get pods ${nsFlag} ${lsFlag} -o wide 2>/dev/null`, { timeout: 8000 }).toString().trim();
              const topLines   = execSync(`kubectl top pods ${nsFlag} ${lsFlag} --no-headers 2>/dev/null`, { timeout: 8000 }).toString().trim();
              const eventLines = execSync(`kubectl get events ${nsFlag} --sort-by=.lastTimestamp 2>/dev/null | tail -20`, { timeout: 8000 }).toString().trim();

              kubectlContext = [
                '', `### kubectl Pod 状态（namespace: ${namespace}）`, '',
                '**Pod 列表：**', '```', podLines || '(无数据)', '```', '',
                '**Pod 资源用量（CPU/MEM）：**', '```', topLines || '(无数据)', '```', '',
                '**最近 K8s 事件：**', '```', eventLines || '(无事件)', '```', '',
                '请结合 Pod 资源用量判断是否存在 OOM/CPU throttling，K8s 事件中是否有 OOMKilled/BackOff。', '',
              ].join('\n');

              // Extract running pod count and resource summary for progress
              const runningPods = (podLines.match(/Running/g) || []).length;
              const topSummary  = topLines.split('\n')[0] ?? '';
              progress('kubectl', 'done',
                `${runningPods} 个 Pod Running。资源用量: ${topSummary || '(metrics-server 未响应)'}。K8s 事件: ${eventLines.split('\n').length} 条`);
              console.log(`[index] Fetched kubectl context for RCA issue #${saved.id}`);
            } catch (e: any) {
              progress('kubectl', 'error', e.message);
              console.warn(`[index] kubectl fetch failed: ${e.message}`);
            }
          }

          // 5. GitHub source code
          try {
            const githubCfg = loadGithubRepos(GITHUB_REPOS_CONFIG);
            const enabledRepos = githubCfg.github_repos.filter(r => r.enabled);
            if (enabledRepos.length > 0) {
              const matchedRepo = enabledRepos.find(r =>
                r.name === serviceName || r.id === serviceName || r.repo.includes(serviceName || ''),
              ) ?? enabledRepos[0];

              progress('source', 'running', `读取 GitHub 源码: ${matchedRepo.repo}（服务: ${serviceName || 'all'}）`);
              const srcCtx = await fetchSourceContext(matchedRepo.token, matchedRepo.repo, serviceName);
              sourceContext = formatSourceForPrompt(srcCtx);
              progress('source', 'done',
                `读取 ${srcCtx.files.length} 个文件: ${srcCtx.files.map(f => f.path).join(', ')}`);
              console.log(`[index] Fetched ${srcCtx.files.length} source files from ${matchedRepo.repo} for RCA issue #${saved.id}`);
            }
          } catch (e: any) {
            progress('source', 'error', e.message);
            console.warn(`[index] GitHub source fetch failed: ${e.message}`);
          }

          // 6. Claude analysis
          progress('analysis', 'running', '所有信号采集完毕，Claude 正在综合分析根因...');

          const rcaPrompt =
            buildRcaPrompt({
              issueId:     saved.id,
              title:       saved.title,
              resource_id: saved.resource_id || '',
              severity:    saved.severity,
              description: saved.description || '',
              regions:     [],
              metric_data: alert.metric_data,
            }) +
            (triggerType === 'metrics+error'
              ? '\n\n> ⚠️ 此告警由「指标阈值突破 + 应用错误事件」联合触发，请重点分析 Mimir 指标趋势与源码中的连接池配置。\n'
              : '') +
            metricsContext +
            traceContext +
            logsContext +
            kubectlContext +
            sourceContext;

          try {
            const response = await claudeClient.query(rcaPrompt, 'alert', 'system');
            const jsonMatch = response.match(/```json\s*([\s\S]*?)\s*```/);
            const jsonStr   = jsonMatch?.[1] || response.match(/\{"[\s\S]*?"root_cause"[\s\S]*?\}/)?.[0];
            if (!jsonStr) {
              progress('analysis', 'error', 'Claude 返回格式异常，无 JSON 块');
              console.warn('[index] RCA response has no JSON block — skipping DB save');
              console.warn('[index] RCA raw response:', response);
              return;
            }
            const parsed = JSON.parse(jsonStr);
            const { createRcaResult, saveRcaResultToDB } = await import('./rca');
            const rcaResult = createRcaResult({
              issue_id:             saved.id,
              root_cause:           parsed.root_cause || 'Unknown',
              confidence:           parsed.confidence ?? 0.5,
              contributing_factors: parsed.contributing_factors || [],
              recommendations:      parsed.recommendations || [],
              fix_plan:             parsed.fix_plan || {},
              fix_risk_level:       parsed.fix_risk_level || 'medium',
              evidence:             parsed.evidence || {},
              model_id:             'claude',
              duration_ms:          Date.now() - rcaStart,
            });
            await saveRcaResultToDB(rcaResult);
            console.log(`[index] RCA result saved for issue #${saved.id} (signals: metrics=${!!metricsContext} traces=${!!traceContext} logs=${!!logsContext} kubectl=${!!kubectlContext} source=${!!sourceContext})`);
            progress('analysis', 'done', parsed.root_cause ?? '分析完成');
            adminApi.pushEvent('rca_completed', { id: saved.id, root_cause: parsed.root_cause, confidence: parsed.confidence ?? 0.5 });
          } catch (e: any) {
            progress('analysis', 'error', e.message);
            console.error(`[index] Background RCA failed: ${e.message}`);
          }
        })();

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

// Initialize DB schema before starting server
initSchema().then(() => {
  console.log('[index] DB schema initialized');
}).catch(err => {
  console.error(`[index] DB schema init failed (alerts will not be persisted): ${err.message}`);
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
