import * as http from 'http';
import * as fs from 'fs';
import * as path from 'path';
import * as yaml from 'js-yaml';

import { KubeconfigManager } from './kubeconfig-manager';

export interface AdminApiOptions {
  glossaryConfigPath: string;
  accountsConfigPath: string;
  platformsConfigPath: string;
  skillsConfigPath: string;
  knowledgeDir: string;
  scheduledJobsConfigPath?: string;
  pluginsConfigPath?: string;
  providerConfigPath?: string;
  clustersConfigPath?: string;
  tenantsConfigPath?: string;
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
  private readonly clustersPath?: string;
  private readonly tenantsPath?: string;
  private readonly kubeconfigManager?: KubeconfigManager;
  private readonly onScheduledJobsChanged?: () => void;
  private readonly onTenantsChanged?: () => void;
  private authWarningLogged = false;

  constructor(options: AdminApiOptions) {
    this.glossaryPath = path.resolve(options.glossaryConfigPath);
    this.accountsPath = path.resolve(options.accountsConfigPath);
    this.platformsPath = path.resolve(options.platformsConfigPath);
    this.skillsPath = path.resolve(options.skillsConfigPath);
    this.knowledgeDir = path.resolve(options.knowledgeDir);
    if (options.scheduledJobsConfigPath) this.scheduledJobsPath = path.resolve(options.scheduledJobsConfigPath);
    if (options.pluginsConfigPath) this.pluginsPath = path.resolve(options.pluginsConfigPath);
    if (options.providerConfigPath) this.providerPath = path.resolve(options.providerConfigPath);
    if (options.clustersConfigPath) this.clustersPath = path.resolve(options.clustersConfigPath);
    if (options.tenantsConfigPath) this.tenantsPath = path.resolve(options.tenantsConfigPath);
    this.kubeconfigManager = options.kubeconfigManager;
    this.onScheduledJobsChanged = options.onScheduledJobsChanged;
    this.onTenantsChanged = options.onTenantsChanged;
  }

  async handleRequest(
    req: http.IncomingMessage,
    res: http.ServerResponse,
    urlPath: string,
    body?: any,
  ): Promise<boolean> {
    // CORS headers for admin API
    const corsOrigin = process.env.ADMIN_CORS_ORIGIN || '*';
    res.setHeader('Access-Control-Allow-Origin', corsOrigin);
    res.setHeader('Access-Control-Allow-Methods', 'GET, PUT, POST, DELETE, OPTIONS');
    res.setHeader('Access-Control-Allow-Headers', 'Content-Type, X-Admin-Key');

    if (req.method === 'OPTIONS') {
      res.writeHead(204);
      res.end();
      return true;
    }

    // API key authentication
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
    fs.writeFileSync(filePath, yaml.dump(data, { lineWidth: 120, noRefs: true }), 'utf-8');
  }

  private jsonResponse(res: http.ServerResponse, status: number, data: any): void {
    res.writeHead(status, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify(data));
  }
}
