import * as http from 'http';
import * as fs from 'fs';
import * as path from 'path';
import * as yaml from 'js-yaml';

import { KubeconfigManager } from './kubeconfig-manager';
import { UserRole, loadUsers, saveUsers, hashPassword } from './auth';
import { getUserKnowledgeDir, getUserConfigPath, sanitizeUsername } from './user-config-loader';

export interface AdminApiOptions {
  glossaryConfigPath: string;
  accountsConfigPath: string;
  platformsConfigPath: string;
  skillsConfigPath: string;
  knowledgeDir: string;
  scheduledJobsConfigPath?: string;
  pluginsConfigPath?: string;
  providerConfigPath?: string;
  telemetryConfigPath?: string;
  clustersConfigPath?: string;
  tenantsConfigPath?: string;
  usersConfigPath?: string;
  kubeconfigManager?: KubeconfigManager;
  onScheduledJobsChanged?: () => void;
  onTenantsChanged?: () => void;
}

export class AdminApi {
  private readonly glossaryPath: string;
  private readonly accountsPath: string;
  private readonly platformsPath: string;
  private readonly skillsPath: string;
  private readonly knowledgeDir: string;
  private readonly scheduledJobsPath?: string;
  private readonly pluginsPath?: string;
  private readonly providerPath?: string;
  private readonly telemetryPath?: string;
  private readonly clustersPath?: string;
  private readonly tenantsPath?: string;
  private readonly usersPath?: string;
  private readonly kubeconfigManager?: KubeconfigManager;
  private readonly onScheduledJobsChanged?: () => void;
  private readonly onTenantsChanged?: () => void;
  private authWarningLogged = false;
  public approvalStore?: import('./approval-store').ApprovalStore;
  public onApprovalApproved?: (approvalId: string) => void;
  public onApprovalRejected?: (approvalId: string) => void;

  constructor(options: AdminApiOptions) {
    this.glossaryPath = path.resolve(options.glossaryConfigPath);
    this.accountsPath = path.resolve(options.accountsConfigPath);
    this.platformsPath = path.resolve(options.platformsConfigPath);
    this.skillsPath = path.resolve(options.skillsConfigPath);
    this.knowledgeDir = path.resolve(options.knowledgeDir);
    if (options.scheduledJobsConfigPath) this.scheduledJobsPath = path.resolve(options.scheduledJobsConfigPath);
    if (options.pluginsConfigPath) this.pluginsPath = path.resolve(options.pluginsConfigPath);
    if (options.providerConfigPath) this.providerPath = path.resolve(options.providerConfigPath);
    if (options.telemetryConfigPath) this.telemetryPath = path.resolve(options.telemetryConfigPath);
    if (options.clustersConfigPath) this.clustersPath = path.resolve(options.clustersConfigPath);
    if (options.tenantsConfigPath) this.tenantsPath = path.resolve(options.tenantsConfigPath);
    if (options.usersConfigPath) this.usersPath = path.resolve(options.usersConfigPath);
    this.kubeconfigManager = options.kubeconfigManager;
    this.onScheduledJobsChanged = options.onScheduledJobsChanged;
    this.onTenantsChanged = options.onTenantsChanged;
  }

  async handleRequest(
    req: http.IncomingMessage,
    res: http.ServerResponse,
    urlPath: string,
    body?: any,
    authUser?: { username: string; role: UserRole; tenant_id?: string } | null,
  ): Promise<boolean> {
    // CORS headers — default to same-origin (no CORS) unless explicitly configured
    const corsOrigin = process.env.ADMIN_CORS_ORIGIN;
    if (corsOrigin) res.setHeader('Access-Control-Allow-Origin', corsOrigin);
    res.setHeader('Access-Control-Allow-Methods', 'GET, PUT, POST, DELETE, OPTIONS');
    res.setHeader('Access-Control-Allow-Headers', 'Content-Type, X-Admin-Key');

    if (req.method === 'OPTIONS') {
      res.writeHead(204);
      res.end();
      return true;
    }

    // API key authentication (backward compat when no user auth)
    if (!authUser) {
      const adminApiKey = process.env.ADMIN_API_KEY;
      if (adminApiKey) {
        const providedKey = req.headers['x-admin-key'];
        if (providedKey !== adminApiKey) {
          this.jsonResponse(res, 401, { error: 'Unauthorized: invalid or missing X-Admin-Key header' });
          return true;
        }
      } else {
        if (!this.authWarningLogged) {
          console.warn('[admin-api] WARNING: ADMIN_API_KEY is not set. Admin API is accessible without authentication.');
          this.authWarningLogged = true;
        }
      }
    }

    // ── Role-based access control ──────────────────────────────
    if (authUser?.role === 'tenant_admin') {
      const allowed = ['/admin/api/glossary', '/admin/api/accounts', '/admin/api/skills',
        '/admin/api/knowledge', '/admin/api/clusters', '/admin/api/chat', '/admin/api/upload',
        '/admin/api/me/'];
      const isAllowed = allowed.some(prefix => urlPath.startsWith(prefix));
      if (!isAllowed) {
        this.jsonResponse(res, 403, { error: 'Forbidden: insufficient permissions' });
        return true;
      }

      // Transparent tenant rewriting: redirect to tenant-scoped endpoints
      const tid = authUser.tenant_id!;
      if (urlPath === '/admin/api/glossary') {
        const tenantGlossaryPath = path.join(this.knowledgeDir, '_tenants', tid, '_config', 'glossary.yaml');
        if (req.method === 'GET') return this.getTenantYaml(res, tenantGlossaryPath, 'glossary', {});
        if (req.method === 'PUT') return this.putTenantYaml(res, tenantGlossaryPath, 'glossary', body);
      }
      if (urlPath === '/admin/api/skills') {
        const tenantSkillsPath = path.join(this.knowledgeDir, '_tenants', tid, '_config', 'skills.yaml');
        if (req.method === 'GET') return this.getTenantYaml(res, tenantSkillsPath, 'skills', []);
        if (req.method === 'PUT') return this.putTenantYaml(res, tenantSkillsPath, 'skills', body);
      }
      if (urlPath === '/admin/api/knowledge') {
        const tenantDir = path.join(this.knowledgeDir, '_tenants', tid);
        return this.listKnowledgeFiles(res, tenantDir);
      }
      const knMatch = urlPath.match(/^\/admin\/api\/knowledge\/(.+)$/);
      if (knMatch) {
        const tenantDir = path.join(this.knowledgeDir, '_tenants', tid);
        const filename = decodeURIComponent(knMatch[1]);
        if (filename.includes('..') || filename.includes('/')) {
          this.jsonResponse(res, 400, { error: 'Invalid filename' });
          return true;
        }
        if (req.method === 'GET') return this.getKnowledgeFile(res, filename, tenantDir);
        if (req.method === 'PUT') return this.putKnowledgeFile(res, filename, body, tenantDir);
        if (req.method === 'DELETE') return this.deleteKnowledgeFile(res, filename, tenantDir);
      }
      if (urlPath === '/admin/api/accounts') {
        return this.getTenantAccounts(res, tid);
      }
    }

    if (urlPath === '/admin/api/glossary') {
      if (req.method === 'GET') return this.getGlossary(res);
      if (req.method === 'PUT') return this.putGlossary(res, body);
    }

    if (urlPath === '/admin/api/accounts') {
      if (req.method === 'GET') return this.getAccounts(res);
      if (req.method === 'PUT') return this.putAccounts(res, body);
    }

    if (urlPath === '/admin/api/platforms') {
      if (req.method === 'GET') return this.getPlatforms(res);
      if (req.method === 'PUT') return this.putPlatforms(res, body);
    }

    if (urlPath === '/admin/api/skills') {
      if (req.method === 'GET') return this.getSkills(res);
      if (req.method === 'PUT') return this.putSkills(res, body);
    }

    if (urlPath === '/admin/api/scheduled-jobs' && this.scheduledJobsPath) {
      if (req.method === 'GET') return this.getScheduledJobs(res);
      if (req.method === 'PUT') return this.putScheduledJobs(res, body);
    }

    if (urlPath === '/admin/api/plugins' && this.pluginsPath) {
      if (req.method === 'GET') return this.getPlugins(res);
      if (req.method === 'PUT') return this.putPlugins(res, body);
    }

    if (urlPath === '/admin/api/telemetry') {
      if (req.method === 'GET') {
        const data = this.telemetryPath ? this.readYaml(this.telemetryPath) : null;
        this.jsonResponse(res, 200, data || { telemetry: {} });
        return true;
      }
      if (req.method === 'PUT') {
        if (!this.telemetryPath) { this.jsonResponse(res, 404, { error: 'telemetry config not configured' }); return true; }
        if (!body?.telemetry) { this.jsonResponse(res, 400, { error: 'body.telemetry required' }); return true; }
        this.writeYaml(this.telemetryPath, { telemetry: body.telemetry });
        this.jsonResponse(res, 200, { ok: true });
        return true;
      }
    }

    if (urlPath === '/admin/api/provider' && this.providerPath) {
      if (req.method === 'GET') return this.getProvider(res);
      if (req.method === 'PUT') return this.putProvider(res, body);
    }

    // Tenants management
    if (urlPath === '/admin/api/tenants' && this.tenantsPath) {
      if (req.method === 'GET') return this.getTenants(res);
      if (req.method === 'PUT') return this.putTenants(res, body);
    }

    // Per-tenant resource management: /admin/api/tenants/:id/(glossary|skills|knowledge[/:file])
    const tenantResMatch = urlPath.match(/^\/admin\/api\/tenants\/([^/]+)\/(glossary|skills|knowledge)(?:\/(.+))?$/);
    if (tenantResMatch) {
      const tenantId = decodeURIComponent(tenantResMatch[1]);
      const resource = tenantResMatch[2];
      const subPath = tenantResMatch[3] ? decodeURIComponent(tenantResMatch[3]) : null;

      if (tenantId.includes('..') || tenantId.includes('/')) {
        this.jsonResponse(res, 400, { error: 'Invalid tenant ID' });
        return true;
      }

      const tenantDir = path.join(this.knowledgeDir, '_tenants', tenantId);
      const tenantConfigDir = path.join(tenantDir, '_config');

      if (resource === 'glossary') {
        const glossaryPath = path.join(tenantConfigDir, 'glossary.yaml');
        if (req.method === 'GET') return this.getTenantYaml(res, glossaryPath, 'glossary', {});
        if (req.method === 'PUT') return this.putTenantYaml(res, glossaryPath, 'glossary', body);
      }

      if (resource === 'skills') {
        const skillsPath = path.join(tenantConfigDir, 'skills.yaml');
        if (req.method === 'GET') return this.getTenantYaml(res, skillsPath, 'skills', []);
        if (req.method === 'PUT') return this.putTenantYaml(res, skillsPath, 'skills', body);
      }

      if (resource === 'knowledge') {
        if (!subPath) {
          if (req.method === 'GET') return this.listKnowledgeFiles(res, tenantDir);
        } else {
          if (subPath.includes('..') || subPath.includes('/')) {
            this.jsonResponse(res, 400, { error: 'Invalid filename' });
            return true;
          }
          if (req.method === 'GET') return this.getKnowledgeFile(res, subPath, tenantDir);
          if (req.method === 'PUT') return this.putKnowledgeFile(res, subPath, body, tenantDir);
          if (req.method === 'DELETE') return this.deleteKnowledgeFile(res, subPath, tenantDir);
        }
      }
    }

    // ── Issues API ─────────────────────────────────────────────────
    if (urlPath === '/admin/api/issues' && req.method === 'GET') {
      try {
        const db = require('./db') as typeof import('./db');
        const tenantFilter = authUser?.role === 'tenant_admin' ? authUser.tenant_id : undefined;
        const params: any[] = [];
        let where = '';
        if (tenantFilter) { params.push(tenantFilter); where = `WHERE tenant_id = $${params.length}`; }
        const issues = await db.query(
          `SELECT id, resource_id, resource_type, severity, status, source, title, occurrence_count, account_name, tenant_id, created_at, updated_at, resolved_at FROM issues ${where} ORDER BY created_at DESC LIMIT 100`,
          params,
        );
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ issues }));
        return true;
      } catch (err: any) {
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ issues: [], db_error: err.message }));
        return true;
      }
    }

    const issueDetailMatch = urlPath.match(/^\/admin\/api\/issues\/(\d+)$/);
    if (issueDetailMatch && req.method === 'GET') {
      try {
        const db = require('./db') as typeof import('./db');
        const issueId = issueDetailMatch[1];
        const issue = await db.queryOne('SELECT * FROM issues WHERE id = $1', [issueId]);
        const rcaResults = await db.query('SELECT * FROM rca_results WHERE issue_id = $1 ORDER BY created_at DESC', [issueId]);
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ issue, rca_results: rcaResults }));
        return true;
      } catch (err: any) {
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ issue: null, rca_results: [], db_error: err.message }));
        return true;
      }
    }

    const issueStatusMatch = urlPath.match(/^\/admin\/api\/issues\/(\d+)\/status$/);
    if (issueStatusMatch && req.method === 'PUT') {
      try {
        const db = require('./db') as typeof import('./db');
        const issueId = issueStatusMatch[1];
        const newStatus = body?.status;
        if (!newStatus) {
          res.writeHead(400, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ error: 'status is required' }));
          return true;
        }
        const resolvedAt = newStatus === 'resolved' ? ', resolved_at = NOW()' : '';
        await db.query(`UPDATE issues SET status = $1, updated_at = NOW()${resolvedAt} WHERE id = $2`, [newStatus, issueId]);
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ ok: true }));
        return true;
      } catch (err: any) {
        res.writeHead(500, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: err.message }));
        return true;
      }
    }

    // ── Resources API ─────────────────────────────────────────────
    if (urlPath === '/admin/api/resources' && req.method === 'GET') {
      try {
        const db = require('./db') as typeof import('./db');
        const resources = await db.query(
          `SELECT resource_id, resource_type, name, provider, region, status, account_name, tenant_id, last_seen_at FROM resources ORDER BY last_seen_at DESC LIMIT 200`,
        );
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ resources }));
        return true;
      } catch (err: any) {
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ resources: [], db_error: err.message }));
        return true;
      }
    }

    if (urlPath === '/admin/api/resources/summary' && req.method === 'GET') {
      try {
        const db = require('./db') as typeof import('./db');
        const typeCounts = await db.query(`SELECT resource_type, COUNT(*) as count FROM resources GROUP BY resource_type ORDER BY count DESC`);
        const providerCounts = await db.query(`SELECT provider, COUNT(*) as count FROM resources GROUP BY provider ORDER BY count DESC`);
        const total = await db.queryOne(`SELECT COUNT(*) as count FROM resources`);
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ total: total?.count || 0, by_type: typeCounts, by_provider: providerCounts }));
        return true;
      } catch (err: any) {
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ total: 0, by_type: [], by_provider: [], db_error: err.message }));
        return true;
      }
    }

    // ── Approvals ─────────────────────────────────────────────────
    const approvalBasePath = urlPath.split('?')[0];
    if (approvalBasePath === '/admin/api/approvals' && req.method === 'POST' && this.approvalStore) {
      if (!body?.command) {
        this.jsonResponse(res, 400, { error: 'command is required' });
        return true;
      }
      const approval = this.approvalStore.create({
        command: body.command,
        requestedBy: body.requestedBy || authUser?.username || 'unknown',
        requestedByName: body.requestedByName || body.requestedBy || 'Unknown',
        platform: body.platform || 'admin',
        channelId: body.channelId,
        tenantId: body.tenantId || authUser?.tenant_id,
      });
      this.jsonResponse(res, 201, approval);
      return true;
    }

    if (approvalBasePath === '/admin/api/approvals' && req.method === 'GET' && this.approvalStore) {
      const tenantFilter = authUser?.role === 'tenant_admin' ? authUser.tenant_id : undefined;
      const status = (new URL(req.url || '', 'http://localhost').searchParams.get('status')) || undefined;
      const list = this.approvalStore.list({ status, tenantId: tenantFilter });
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ approvals: list }));
      return true;
    }

    if (urlPath === '/admin/api/approvals/pending-count' && req.method === 'GET' && this.approvalStore) {
      const tenantFilter = authUser?.role === 'tenant_admin' ? authUser.tenant_id : undefined;
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ count: this.approvalStore.pendingCount(tenantFilter) }));
      return true;
    }

    const approvalMatch = urlPath.match(/^\/admin\/api\/approvals\/(\d+)\/(approve|reject)$/);
    if (approvalMatch && req.method === 'POST' && this.approvalStore) {
      const id = approvalMatch[1];
      const action = approvalMatch[2];
      const approval = this.approvalStore.get(id);
      if (!approval) {
        res.writeHead(404, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: 'Approval not found' }));
        return true;
      }
      // Tenant admin can only approve within their tenant
      if (authUser?.role === 'tenant_admin' && authUser.tenant_id !== approval.tenantId) {
        res.writeHead(403, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: 'Cannot approve requests from other tenants' }));
        return true;
      }
      if (action === 'approve') {
        const result = this.approvalStore.approve(id, authUser?.username || 'admin');
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ approval: result }));
        if (this.onApprovalApproved) this.onApprovalApproved(id);
      } else {
        const reason = body?.reason || '';
        const result = this.approvalStore.reject(id, authUser?.username || 'admin', reason);
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ approval: result }));
        if (this.onApprovalRejected) this.onApprovalRejected(id);
      }
      return true;
    }

    // Users management (super_admin only)
    if (urlPath === '/admin/api/users' && this.usersPath) {
      if (req.method === 'GET') return this.getUsers(res);
      if (req.method === 'POST') return await this.createUser(res, body);
    }
    const userMatch = urlPath.match(/^\/admin\/api\/users\/([^/]+)(?:\/password)?$/);
    if (userMatch && this.usersPath) {
      const username = decodeURIComponent(userMatch[1]);
      if (urlPath.endsWith('/password') && req.method === 'PUT') {
        return await this.resetUserPassword(res, username, body);
      }
      if (req.method === 'DELETE') {
        return this.deleteUser(res, username, authUser?.username);
      }
    }

    // Clusters management
    if (urlPath === '/admin/api/clusters' && this.clustersPath) {
      if (req.method === 'GET') return this.getClusters(res);
      if (req.method === 'PUT') return this.putClusters(res, body);
    }

    if (urlPath === '/admin/api/clusters/status' && this.kubeconfigManager) {
      if (req.method === 'GET') return this.getClustersStatus(res);
    }

    if (urlPath === '/admin/api/clusters/refresh' && this.kubeconfigManager) {
      if (req.method === 'POST') return await this.refreshClusters(res);
    }

    // Knowledge base file management
    if (urlPath === '/admin/api/knowledge') {
      if (req.method === 'GET') return this.listKnowledgeFiles(res);
    }

    const knowledgeMatch = urlPath.match(/^\/admin\/api\/knowledge\/(.+)$/);
    if (knowledgeMatch) {
      const filename = decodeURIComponent(knowledgeMatch[1]);
      if (req.method === 'GET') return this.getKnowledgeFile(res, filename);
      if (req.method === 'PUT') return this.putKnowledgeFile(res, filename, body);
      if (req.method === 'DELETE') return this.deleteKnowledgeFile(res, filename);
    }

    // ── Personal config: /admin/api/me/* ──────────────────────────
    if (urlPath.startsWith('/admin/api/me/') && authUser?.username) {
      const username = authUser.username;

      // Personal knowledge files
      if (urlPath === '/admin/api/me/knowledge') {
        const userDir = getUserKnowledgeDir(this.knowledgeDir, username);
        if (req.method === 'GET') return this.listKnowledgeFiles(res, userDir);
      }
      const meKnMatch = urlPath.match(/^\/admin\/api\/me\/knowledge\/(.+)$/);
      if (meKnMatch) {
        const userDir = getUserKnowledgeDir(this.knowledgeDir, username);
        const filename = decodeURIComponent(meKnMatch[1]);
        if (filename.includes('..') || filename.includes('/')) {
          this.jsonResponse(res, 400, { error: 'Invalid filename' });
          return true;
        }
        if (req.method === 'GET') return this.getKnowledgeFile(res, filename, userDir);
        if (req.method === 'PUT') return this.putKnowledgeFile(res, filename, body, userDir);
        if (req.method === 'DELETE') return this.deleteKnowledgeFile(res, filename, userDir);
      }

      // Personal skills config
      if (urlPath === '/admin/api/me/skills') {
        const configPath = getUserConfigPath(this.knowledgeDir, username, 'skills.yaml');
        if (req.method === 'GET') return this.getPersonalYaml(res, configPath);
        if (req.method === 'PUT') return this.putPersonalYaml(res, configPath, body);
      }

      // Personal scheduled jobs
      if (urlPath === '/admin/api/me/scheduled-jobs') {
        const configPath = getUserConfigPath(this.knowledgeDir, username, 'scheduled-jobs.yaml');
        if (req.method === 'GET') return this.getPersonalYaml(res, configPath);
        if (req.method === 'PUT') return this.putPersonalYaml(res, configPath, body);
      }

      // Personal job results
      if (urlPath === '/admin/api/me/job-results') {
        if (req.method === 'GET') return this.listJobResults(res, username);
      }
    }

    return false;
  }

  private getGlossary(res: http.ServerResponse): boolean {
    const data = this.readYaml(this.glossaryPath);
    const glossary = data?.glossary || {};
    this.jsonResponse(res, 200, { glossary });
    return true;
  }

  private putGlossary(res: http.ServerResponse, body: any): boolean {
    if (!body || typeof body.glossary !== 'object') {
      this.jsonResponse(res, 400, { error: 'Request body must contain a "glossary" object' });
      return true;
    }
    this.writeYaml(this.glossaryPath, { glossary: body.glossary });
    this.jsonResponse(res, 200, { ok: true, count: Object.keys(body.glossary).length });
    return true;
  }

  private getAccounts(res: http.ServerResponse): boolean {
    const data = this.readYaml(this.accountsPath);
    const accounts = data?.accounts || { extra: [], overrides: {}, defaults: {} };
    this.jsonResponse(res, 200, { accounts });
    return true;
  }

  private putAccounts(res: http.ServerResponse, body: any): boolean {
    if (!body || typeof body.accounts !== 'object') {
      this.jsonResponse(res, 400, { error: 'Request body must contain an "accounts" object' });
      return true;
    }
    this.writeYaml(this.accountsPath, { accounts: body.accounts });
    this.jsonResponse(res, 200, { ok: true });
    return true;
  }

  private getPlatforms(res: http.ServerResponse): boolean {
    const data = this.readYaml(this.platformsPath);
    const platforms = data?.platforms || {};
    this.jsonResponse(res, 200, { platforms });
    return true;
  }

  private putPlatforms(res: http.ServerResponse, body: any): boolean {
    if (!body || typeof body.platforms !== 'object') {
      this.jsonResponse(res, 400, { error: 'Request body must contain a "platforms" object' });
      return true;
    }
    this.writeYaml(this.platformsPath, { platforms: body.platforms });
    this.jsonResponse(res, 200, { ok: true });
    return true;
  }

  // ── Skills ──────────────────────────────────────────────────────

  private getSkills(res: http.ServerResponse): boolean {
    const data = this.readYaml(this.skillsPath);
    const skills = data?.skills || [];
    this.jsonResponse(res, 200, { skills });
    return true;
  }

  private putSkills(res: http.ServerResponse, body: any): boolean {
    if (!body || !Array.isArray(body.skills)) {
      this.jsonResponse(res, 400, { error: 'Request body must contain a "skills" array' });
      return true;
    }
    this.writeYaml(this.skillsPath, { skills: body.skills });
    this.jsonResponse(res, 200, { ok: true, count: body.skills.length });
    return true;
  }

  // ── Per-tenant YAML helpers ────────────────────────────────────

  private getTenantYaml(res: http.ServerResponse, filePath: string, rootKey: string, defaultValue: any): boolean {
    const data = this.readYaml(filePath);
    const value = data?.[rootKey] ?? defaultValue;
    this.jsonResponse(res, 200, { [rootKey]: value });
    return true;
  }

  private putTenantYaml(res: http.ServerResponse, filePath: string, rootKey: string, body: any): boolean {
    if (!body || body[rootKey] === undefined) {
      this.jsonResponse(res, 400, { error: `Request body must contain a "${rootKey}" field` });
      return true;
    }
    this.writeYaml(filePath, { [rootKey]: body[rootKey] });
    const count = Array.isArray(body[rootKey]) ? body[rootKey].length : Object.keys(body[rootKey]).length;
    this.jsonResponse(res, 200, { ok: true, count });
    return true;
  }

  // ── Knowledge base file management ──────────────────────────────

  private listKnowledgeFiles(res: http.ServerResponse, baseDir?: string): boolean {
    const dir = baseDir || this.knowledgeDir;
    if (!fs.existsSync(dir)) {
      fs.mkdirSync(dir, { recursive: true });
    }
    const autoGenerated = new Set(['glossary.md', 'accounts.md', 'clusters.md', 'index.md']);
    const hiddenDirs = new Set(['_config', '_tenants']);
    const files = fs.readdirSync(dir)
      .filter(f => {
        if (hiddenDirs.has(f)) return false;
        const stat = fs.statSync(path.join(dir, f));
        return stat.isFile() && !autoGenerated.has(f);
      })
      .map(f => {
        const stat = fs.statSync(path.join(dir, f));
        return {
          name: f,
          size: stat.size,
          modified: stat.mtime.toISOString(),
        };
      })
      .sort((a, b) => a.name.localeCompare(b.name));
    this.jsonResponse(res, 200, { files });
    return true;
  }

  private getKnowledgeFile(res: http.ServerResponse, filename: string, baseDir?: string): boolean {
    if (filename.includes('..') || filename.includes('/')) {
      this.jsonResponse(res, 400, { error: 'Invalid filename' });
      return true;
    }
    const dir = baseDir || this.knowledgeDir;
    const filePath = path.join(dir, filename);
    if (!fs.existsSync(filePath)) {
      this.jsonResponse(res, 404, { error: 'File not found' });
      return true;
    }
    const content = fs.readFileSync(filePath, 'utf-8');
    this.jsonResponse(res, 200, { name: filename, content });
    return true;
  }

  private putKnowledgeFile(res: http.ServerResponse, filename: string, body: any, baseDir?: string): boolean {
    if (filename.includes('..') || filename.includes('/')) {
      this.jsonResponse(res, 400, { error: 'Invalid filename' });
      return true;
    }
    if (!body || typeof body.content !== 'string') {
      this.jsonResponse(res, 400, { error: 'Request body must contain a "content" string' });
      return true;
    }
    const dir = baseDir || this.knowledgeDir;
    if (!fs.existsSync(dir)) {
      fs.mkdirSync(dir, { recursive: true });
    }
    const filePath = path.join(dir, filename);
    fs.writeFileSync(filePath, body.content, 'utf-8');
    this.jsonResponse(res, 200, { ok: true, name: filename, size: Buffer.byteLength(body.content) });
    return true;
  }

  private deleteKnowledgeFile(res: http.ServerResponse, filename: string, baseDir?: string): boolean {
    if (filename.includes('..') || filename.includes('/')) {
      this.jsonResponse(res, 400, { error: 'Invalid filename' });
      return true;
    }
    const dir = baseDir || this.knowledgeDir;
    const filePath = path.join(dir, filename);
    if (!fs.existsSync(filePath)) {
      this.jsonResponse(res, 404, { error: 'File not found' });
      return true;
    }
    fs.unlinkSync(filePath);
    this.jsonResponse(res, 200, { ok: true });
    return true;
  }

  // ── Scheduled Jobs ──────────────────────────────────────────────

  private getScheduledJobs(res: http.ServerResponse): boolean {
    const data = this.readYaml(this.scheduledJobsPath!);
    const scheduled_jobs = data?.scheduled_jobs || [];
    this.jsonResponse(res, 200, { scheduled_jobs });
    return true;
  }

  private putScheduledJobs(res: http.ServerResponse, body: any): boolean {
    if (!body || !Array.isArray(body.scheduled_jobs)) {
      this.jsonResponse(res, 400, { error: 'Request body must contain a "scheduled_jobs" array' });
      return true;
    }
    this.writeYaml(this.scheduledJobsPath!, { scheduled_jobs: body.scheduled_jobs });
    this.jsonResponse(res, 200, { ok: true, count: body.scheduled_jobs.length });
    if (this.onScheduledJobsChanged) this.onScheduledJobsChanged();
    return true;
  }

  // ── Plugins ────────────────────────────────────────────────────

  private getPlugins(res: http.ServerResponse): boolean {
    const data = this.readYaml(this.pluginsPath!);
    const plugins = data?.plugins || {};
    this.jsonResponse(res, 200, { plugins });
    return true;
  }

  private putPlugins(res: http.ServerResponse, body: any): boolean {
    if (!body || typeof body.plugins !== 'object') {
      this.jsonResponse(res, 400, { error: 'Request body must contain a "plugins" object' });
      return true;
    }
    this.writeYaml(this.pluginsPath!, { plugins: body.plugins });
    this.jsonResponse(res, 200, { ok: true });
    return true;
  }

  // ── Provider ──────────────────────────────────────────────────

  private getProvider(res: http.ServerResponse): boolean {
    const data = this.readYaml(this.providerPath!);
    const provider = data?.provider || { type: 'bedrock', model: 'opus' };
    this.jsonResponse(res, 200, { provider });
    return true;
  }

  private putProvider(res: http.ServerResponse, body: any): boolean {
    if (!body || typeof body.provider !== 'object' || !body.provider.type) {
      this.jsonResponse(res, 400, { error: 'Request body must contain a "provider" object with a "type" field' });
      return true;
    }
    this.writeYaml(this.providerPath!, { provider: body.provider });
    console.log(`[admin-api] Provider updated: type=${body.provider.type}`);
    this.jsonResponse(res, 200, { ok: true, type: body.provider.type });
    return true;
  }

  // ── Clusters ──────────────────────────────────────────────────

  private getClusters(res: http.ServerResponse): boolean {
    const data = this.readYaml(this.clustersPath!);
    const clusters = data?.clusters || { auto_discovery: { aws: { enabled: true } }, static: [], refresh: { on_startup: true, interval_minutes: 60 } };
    this.jsonResponse(res, 200, { clusters });
    return true;
  }

  private putClusters(res: http.ServerResponse, body: any): boolean {
    if (!body || typeof body.clusters !== 'object') {
      this.jsonResponse(res, 400, { error: 'Request body must contain a "clusters" object' });
      return true;
    }
    this.writeYaml(this.clustersPath!, { clusters: body.clusters });
    this.jsonResponse(res, 200, { ok: true });
    return true;
  }

  private getClustersStatus(res: http.ServerResponse): boolean {
    const clusters = this.kubeconfigManager!.getDiscoveredClusters();
    const lastRefresh = this.kubeconfigManager!.getLastRefreshTime();
    this.jsonResponse(res, 200, {
      clusters,
      lastRefresh: lastRefresh?.toISOString() || null,
      total: clusters.length,
      active: clusters.filter(c => !c.configError).length,
      failed: clusters.filter(c => c.configError).length,
    });
    return true;
  }

  private async refreshClusters(res: http.ServerResponse): Promise<boolean> {
    try {
      const clusters = await this.kubeconfigManager!.refresh();
      this.jsonResponse(res, 200, {
        ok: true,
        total: clusters.length,
        active: clusters.filter(c => !c.configError).length,
        failed: clusters.filter(c => c.configError).length,
      });
    } catch (err) {
      this.jsonResponse(res, 500, { error: (err as Error).message });
    }
    return true;
  }

  // ── Tenants ──────────────────────────────────────────────────

  private getTenants(res: http.ServerResponse): boolean {
    const data = this.readYaml(this.tenantsPath!);
    const tenants = data?.tenants || [];
    this.jsonResponse(res, 200, { tenants });
    return true;
  }

  private putTenants(res: http.ServerResponse, body: any): boolean {
    if (!body || !Array.isArray(body.tenants)) {
      this.jsonResponse(res, 400, { error: 'Request body must contain a "tenants" array' });
      return true;
    }

    // Validate: no duplicate channel_id across tenants
    const seen = new Map<string, string>();
    for (const tenant of body.tenants) {
      if (!tenant.channels) continue;
      for (const ch of tenant.channels) {
        const key = `${ch.platform}:${ch.channel_id}`;
        if (seen.has(key)) {
          this.jsonResponse(res, 400, {
            error: `Duplicate channel mapping: ${key} is assigned to both "${seen.get(key)}" and "${tenant.id}"`,
          });
          return true;
        }
        seen.set(key, tenant.id);
      }
    }

    this.writeYaml(this.tenantsPath!, { tenants: body.tenants });
    this.jsonResponse(res, 200, { ok: true, count: body.tenants.length });
    if (this.onTenantsChanged) this.onTenantsChanged();
    return true;
  }

  // ── Users ────────────────────────────────────────────────────

  private getUsers(res: http.ServerResponse): boolean {
    const config = loadUsers(this.usersPath!);
    const users = (config?.users || []).map(u => ({
      username: u.username,
      role: u.role,
      tenant_id: u.tenant_id,
    }));
    this.jsonResponse(res, 200, { users });
    return true;
  }

  private async createUser(res: http.ServerResponse, body: any): Promise<boolean> {
    if (!body?.username || !body?.password || !body?.role) {
      this.jsonResponse(res, 400, { error: 'username, password, and role are required' });
      return true;
    }
    if (body.role === 'tenant_admin' && !body.tenant_id) {
      this.jsonResponse(res, 400, { error: 'tenant_id is required for tenant_admin role' });
      return true;
    }
    const config = loadUsers(this.usersPath!) || { users: [] };
    if (config.users.some(u => u.username === body.username)) {
      this.jsonResponse(res, 400, { error: `User "${body.username}" already exists` });
      return true;
    }
    const hash = await hashPassword(body.password);
    const user: any = { username: body.username, password_hash: hash, role: body.role };
    if (body.tenant_id) user.tenant_id = body.tenant_id;
    config.users.push(user);
    saveUsers(this.usersPath!, config);
    console.log(`[admin-api] User "${body.username}" created (${body.role})`);
    this.jsonResponse(res, 200, { ok: true, username: body.username });
    return true;
  }

  private deleteUser(res: http.ServerResponse, username: string, currentUsername?: string): boolean {
    if (username === currentUsername) {
      this.jsonResponse(res, 400, { error: 'Cannot delete your own account' });
      return true;
    }
    const config = loadUsers(this.usersPath!);
    if (!config) {
      this.jsonResponse(res, 404, { error: 'User not found' });
      return true;
    }
    const idx = config.users.findIndex(u => u.username === username);
    if (idx === -1) {
      this.jsonResponse(res, 404, { error: 'User not found' });
      return true;
    }
    config.users.splice(idx, 1);
    saveUsers(this.usersPath!, config);
    console.log(`[admin-api] User "${username}" deleted`);
    this.jsonResponse(res, 200, { ok: true });
    return true;
  }

  private async resetUserPassword(res: http.ServerResponse, username: string, body: any): Promise<boolean> {
    if (!body?.password) {
      this.jsonResponse(res, 400, { error: 'password is required' });
      return true;
    }
    const config = loadUsers(this.usersPath!);
    if (!config) {
      this.jsonResponse(res, 404, { error: 'User not found' });
      return true;
    }
    const user = config.users.find(u => u.username === username);
    if (!user) {
      this.jsonResponse(res, 404, { error: 'User not found' });
      return true;
    }
    user.password_hash = await hashPassword(body.password);
    saveUsers(this.usersPath!, config);
    console.log(`[admin-api] Password reset for "${username}"`);
    this.jsonResponse(res, 200, { ok: true });
    return true;
  }

  // ── Tenant-scoped accounts (for tenant_admin) ─────────────────

  private getTenantAccounts(res: http.ServerResponse, tenantId: string): boolean {
    // Load tenant config to get aws_account_ids and alicloud
    const tenantsData = this.readYaml(this.tenantsPath!);
    const tenant = (tenantsData?.tenants || []).find((t: any) => t.id === tenantId);
    if (!tenant) {
      this.jsonResponse(res, 404, { error: 'Tenant not found' });
      return true;
    }
    this.jsonResponse(res, 200, {
      accounts: {
        aws_account_ids: tenant.aws_account_ids || [],
        alicloud: tenant.alicloud || [],
      },
      tenant_id: tenantId,
    });
    return true;
  }

  // ── Personal config helpers ────────────────────────────────────

  private getPersonalYaml(res: http.ServerResponse, configPath: string): boolean {
    const data = this.readYaml(configPath);
    this.jsonResponse(res, 200, data || {});
    return true;
  }

  private putPersonalYaml(res: http.ServerResponse, configPath: string, body: any): boolean {
    if (!body || typeof body !== 'object') {
      this.jsonResponse(res, 400, { error: 'Request body must be a JSON object' });
      return true;
    }
    this.writeYaml(configPath, body);
    this.jsonResponse(res, 200, { ok: true });
    return true;
  }

  private listJobResults(res: http.ServerResponse, username: string): boolean {
    const resultsDir = path.join(this.knowledgeDir, '_users', username, '_job_results');
    if (!fs.existsSync(resultsDir)) {
      this.jsonResponse(res, 200, { results: [] });
      return true;
    }
    const files = fs.readdirSync(resultsDir)
      .filter(f => f.endsWith('.md'))
      .sort()
      .reverse()
      .slice(0, 50); // Limit to 50 most recent
    const results = files.map(f => {
      const content = fs.readFileSync(path.join(resultsDir, f), 'utf-8');
      return { filename: f, content, size: content.length };
    });
    this.jsonResponse(res, 200, { results });
    return true;
  }

  // ── YAML helpers ───────────────────────────────────────────────

  private readYaml(filePath: string): any {
    if (!fs.existsSync(filePath)) return null;
    const content = fs.readFileSync(filePath, 'utf-8');
    return yaml.load(content) || null;
  }

  private writeYaml(filePath: string, data: any): void {
    const dir = path.dirname(filePath);
    if (!fs.existsSync(dir)) {
      fs.mkdirSync(dir, { recursive: true });
    }
    // Backup current file before overwrite
    if (fs.existsSync(filePath)) {
      const backupPath = filePath + '.bak';
      try { fs.copyFileSync(filePath, backupPath); } catch { /* best effort */ }
    }
    // Atomic write: write to temp file, then rename (prevents corruption on crash)
    const tmpPath = filePath + '.tmp.' + Date.now();
    fs.writeFileSync(tmpPath, yaml.dump(data, { lineWidth: 120, noRefs: true }), 'utf-8');
    fs.renameSync(tmpPath, filePath);
  }

  private jsonResponse(res: http.ServerResponse, status: number, data: any): void {
    res.writeHead(status, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify(data));
  }
}
