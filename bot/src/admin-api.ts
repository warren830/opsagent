import * as http from 'http';
import * as fs from 'fs';
import * as path from 'path';
import * as yaml from 'js-yaml';

export interface AdminApiOptions {
  glossaryConfigPath: string;
  accountsConfigPath: string;
  platformsConfigPath: string;
  skillsConfigPath: string;
  knowledgeDir: string;
  scheduledJobsConfigPath?: string;
  pluginsConfigPath?: string;
  onScheduledJobsChanged?: () => void;
}

export class AdminApi {
  private readonly glossaryPath: string;
  private readonly accountsPath: string;
  private readonly platformsPath: string;
  private readonly skillsPath: string;
  private readonly knowledgeDir: string;
  private readonly scheduledJobsPath?: string;
  private readonly pluginsPath?: string;
  private readonly onScheduledJobsChanged?: () => void;
  private authWarningLogged = false;

  constructor(options: AdminApiOptions) {
    this.glossaryPath = path.resolve(options.glossaryConfigPath);
    this.accountsPath = path.resolve(options.accountsConfigPath);
    this.platformsPath = path.resolve(options.platformsConfigPath);
    this.skillsPath = path.resolve(options.skillsConfigPath);
    this.knowledgeDir = path.resolve(options.knowledgeDir);
    if (options.scheduledJobsConfigPath) this.scheduledJobsPath = path.resolve(options.scheduledJobsConfigPath);
    if (options.pluginsConfigPath) this.pluginsPath = path.resolve(options.pluginsConfigPath);
    this.onScheduledJobsChanged = options.onScheduledJobsChanged;
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

  // ── Knowledge base file management ──────────────────────────────

  private listKnowledgeFiles(res: http.ServerResponse): boolean {
    if (!fs.existsSync(this.knowledgeDir)) {
      fs.mkdirSync(this.knowledgeDir, { recursive: true });
    }
    const files = fs.readdirSync(this.knowledgeDir)
      .filter(f => {
        const stat = fs.statSync(path.join(this.knowledgeDir, f));
        return stat.isFile();
      })
      .map(f => {
        const stat = fs.statSync(path.join(this.knowledgeDir, f));
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

  private getKnowledgeFile(res: http.ServerResponse, filename: string): boolean {
    if (filename.includes('..') || filename.includes('/')) {
      this.jsonResponse(res, 400, { error: 'Invalid filename' });
      return true;
    }
    const filePath = path.join(this.knowledgeDir, filename);
    if (!fs.existsSync(filePath)) {
      this.jsonResponse(res, 404, { error: 'File not found' });
      return true;
    }
    const content = fs.readFileSync(filePath, 'utf-8');
    this.jsonResponse(res, 200, { name: filename, content });
    return true;
  }

  private putKnowledgeFile(res: http.ServerResponse, filename: string, body: any): boolean {
    if (filename.includes('..') || filename.includes('/')) {
      this.jsonResponse(res, 400, { error: 'Invalid filename' });
      return true;
    }
    if (!body || typeof body.content !== 'string') {
      this.jsonResponse(res, 400, { error: 'Request body must contain a "content" string' });
      return true;
    }
    if (!fs.existsSync(this.knowledgeDir)) {
      fs.mkdirSync(this.knowledgeDir, { recursive: true });
    }
    const filePath = path.join(this.knowledgeDir, filename);
    fs.writeFileSync(filePath, body.content, 'utf-8');
    this.jsonResponse(res, 200, { ok: true, name: filename, size: Buffer.byteLength(body.content) });
    return true;
  }

  private deleteKnowledgeFile(res: http.ServerResponse, filename: string): boolean {
    if (filename.includes('..') || filename.includes('/')) {
      this.jsonResponse(res, 400, { error: 'Invalid filename' });
      return true;
    }
    const filePath = path.join(this.knowledgeDir, filename);
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
