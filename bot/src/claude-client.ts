import { ChildProcess, spawn } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';
import { loadPlugins, generateMcpConfig } from './plugin-loader';
import { loadGlossary, generateGlossaryKnowledge } from './glossary-loader';
import { loadAccounts, generateAccountsKnowledge, generateAlicloudPromptSection } from './accounts-loader';
import { loadSkills, generateSkillsPrompt } from './skills-loader';
import { scanKnowledgeFiles, generateKnowledgeIndex, generateKnowledgeIndexFile } from './knowledge-loader';
import { loadProvider, buildProviderEnv, ProviderConfig } from './provider-loader';
import { loadTenants, TenantConfig, TenantAlicloudAccount } from './tenant-loader';
import { ApprovalStore } from './approval-store';

const CLAUDE_TIMEOUT_MS = 5 * 60 * 1000; // 5 minutes
const SESSION_TTL_MS = 30 * 60 * 1000; // 30 minutes idle -> expire session

const BASE_SYSTEM_PROMPT = `你是 OpsAgent，一个多云基础设施查询助手。
支持 AWS、阿里云、Azure、GCP 等多云平台。
使用 aws cli 查询 AWS 资源，使用 aliyun cli 查询阿里云资源，使用 az cli 查询 Azure 资源，使用 gcloud 查询 GCP 资源。
使用 kubectl 查询 Kubernetes 集群。
使用 MCP 工具查询 Confluence 文档、Jira 工单等外部数据源。
AWS 跨账号查询时使用 ./scripts/foreach-account.sh 自动遍历所有 Organizations 账号。
阿里云多账号查询时使用 aliyun configure 切换 profile。
knowledge/ 目录包含公司的知识库文件。CLAUDE.md 末尾有知识库索引，列出所有可用文件。遇到基础设施问题时，先根据索引用 Read 读取相关知识文件，而不是盲目搜索。
始终返回结构化、易读的 Markdown 表格结果。

## CRITICAL: kubectl 使用规则
kubeconfig 已预配置，可直接使用 kubectl。按以下步骤操作：
1. 用 Read 工具读取 knowledge/clusters.md 获取所有集群信息和 context 名称
2. 直接用 --context 参数执行 kubectl 命令，无需手动配置 kubeconfig
   kubectl --context <context-name> get pods -A
3. 或使用 ./scripts/kubectl-all.sh 一次查询所有集群
4. 汇总所有集群的结果到一张表中，表中必须包含「集群」列

重要：除非用户明确指定了某个集群，否则必须查询所有集群并汇总结果。

## kubectl 写操作审批
kubectl 写操作（apply/create/delete/patch/edit/scale/rollout/drain/cordon/taint 等）必须使用 ./scripts/kubectl-safe.sh 替代直接 kubectl。
例如: ./scripts/kubectl-safe.sh --context prod-cluster delete pod my-pod
该脚本会自动将写操作提交审批，读操作则直接执行。`;

export interface ClaudeClientOptions {
  workDir: string;
  pluginsConfigPath: string;
  mcpConfigPath: string;
  glossaryConfigPath?: string;
  accountsConfigPath?: string;
  skillsConfigPath?: string;
  knowledgeDir?: string;
  providerConfigPath?: string;
  tenantsConfigPath?: string;
  timeoutMs?: number;
}

export interface StreamChunk {
  type: 'init' | 'thinking' | 'text' | 'tool_use' | 'done';
  content: string;
  sessionId?: string | null;
  turn?: number;
  toolName?: string;
  durationMs?: number;
}

interface SessionEntry {
  sessionId: string;
  lastActiveAt: number;
}

export class ClaudeClient {
  private readonly workDir: string;
  private readonly pluginsConfigPath: string;
  private readonly mcpConfigPath: string;
  private readonly glossaryConfigPath: string;
  private readonly accountsConfigPath: string;
  private readonly skillsConfigPath: string;
  private readonly knowledgeDir: string;
  private readonly providerConfigPath: string;
  private readonly tenantsConfigPath: string;
  private readonly timeoutMs: number;

  /** Approval store for kubectl write operations (used by wrapper script) */
  public approvalStore?: ApprovalStore;

  /** Map of "platform:userId" -> session info */
  private readonly sessions = new Map<string, SessionEntry>();

  constructor(options: ClaudeClientOptions) {
    this.workDir = options.workDir;
    this.pluginsConfigPath = options.pluginsConfigPath;
    this.mcpConfigPath = options.mcpConfigPath;
    this.glossaryConfigPath = options.glossaryConfigPath ?? path.join(this.workDir, 'config/glossary.yaml');
    this.accountsConfigPath = options.accountsConfigPath ?? path.join(this.workDir, 'config/accounts.yaml');
    this.skillsConfigPath = options.skillsConfigPath ?? path.join(this.workDir, 'config/skills.yaml');
    this.knowledgeDir = options.knowledgeDir ?? path.join(this.workDir, 'knowledge');
    this.providerConfigPath = options.providerConfigPath ?? path.join(this.workDir, 'config/providers.yaml');
    this.tenantsConfigPath = options.tenantsConfigPath ?? path.join(this.workDir, 'config/tenants.yaml');
    this.timeoutMs = options.timeoutMs ?? CLAUDE_TIMEOUT_MS;
  }

  /**
   * Generate CLAUDE.md in workDir — Claude Code reads this natively.
   * Also generates knowledge files (glossary.md, accounts.md) and skills/*.md.
   */
  private findTenant(tenantId: string): TenantConfig | null {
    const file = loadTenants(this.tenantsConfigPath);
    if (!file) return null;
    return file.tenants.find(t => t.id === tenantId) || null;
  }

  /**
   * Get the tenant data directory: knowledge/_tenants/{tenantId}/
   * Contains tenant-specific glossary, skills, and knowledge files.
   */
  private getTenantDir(tenantId: string): string {
    return path.join(this.knowledgeDir, '_tenants', tenantId);
  }

  private generateClaudeMd(tenantId?: string): void {
    const parts: string[] = [BASE_SYSTEM_PROMPT];
    const tenant = tenantId ? this.findTenant(tenantId) : null;

    // Determine paths: tenant-specific or shared
    const tenantDir = tenantId ? this.getTenantDir(tenantId) : null;
    const tenantConfigDir = tenantDir ? path.join(tenantDir, '_config') : null;
    const glossaryPath = tenantConfigDir ? path.join(tenantConfigDir, 'glossary.yaml') : this.glossaryConfigPath;
    const skillsPath = tenantConfigDir ? path.join(tenantConfigDir, 'skills.yaml') : this.skillsConfigPath;
    const knowledgeDir = tenantDir || this.knowledgeDir;
    const skillsDir = tenantDir ? path.join(tenantDir, 'skills') : path.join(this.workDir, 'skills');

    // Ensure tenant dirs exist
    if (tenantDir && !fs.existsSync(tenantDir)) {
      fs.mkdirSync(tenantDir, { recursive: true });
    }
    if (tenantConfigDir && !fs.existsSync(tenantConfigDir)) {
      fs.mkdirSync(tenantConfigDir, { recursive: true });
    }

    // Glossary summary (tenant-scoped)
    const glossary = loadGlossary(glossaryPath);
    if (glossary) {
      const summary = generateGlossaryKnowledge(glossary, knowledgeDir);
      if (summary) {
        parts.push('');
        parts.push('## 公司术语速查');
        parts.push(summary);
        parts.push(`完整术语详情见 ${tenantId ? 'knowledge/_tenants/' + tenantId + '/glossary.md' : 'knowledge/glossary.md'} 文件。`);
      }
    }

    // Accounts summary (filtered by tenant)
    const accounts = loadAccounts(this.accountsConfigPath);
    if (accounts) {
      if (tenant?.aws_account_ids) {
        const allowedIds = new Set(tenant.aws_account_ids);
        if (accounts.accounts.extra) {
          accounts.accounts.extra = accounts.accounts.extra.filter(a => allowedIds.has(a.id));
        }
        if (accounts.accounts.overrides) {
          const filtered: Record<string, any> = {};
          for (const [id, override] of Object.entries(accounts.accounts.overrides)) {
            if (allowedIds.has(id)) filtered[id] = override;
          }
          accounts.accounts.overrides = filtered;
        }
      }
      const summary = generateAccountsKnowledge(accounts, knowledgeDir);
      if (summary) {
        parts.push('');
        parts.push('## 已配置的云账号');
        parts.push(summary);
      }
    }

    // Alicloud section for tenant
    if (tenant?.alicloud && tenant.alicloud.length > 0) {
      const aliSection = generateAlicloudPromptSection(tenant.alicloud);
      if (aliSection) {
        parts.push('');
        parts.push(aliSection);
      }
    }

    // Tenant scope info
    if (tenant) {
      parts.push('');
      parts.push(`## 当前租户: ${tenant.name} (${tenant.id})`);
      if (tenant.aws_account_ids?.length) {
        parts.push(`仅查询以下 AWS 账号: ${tenant.aws_account_ids.join(', ')}`);
        parts.push(`使用 foreach-account.sh 时必须加 --accounts ${tenant.aws_account_ids.join(',')} 参数限制范围。`);
      }
    }

    // Skills (tenant-scoped)
    const skillsConfig = loadSkills(skillsPath);
    if (skillsConfig) {
      const skillsPrompt = generateSkillsPrompt(skillsConfig, skillsDir);
      if (skillsPrompt) {
        parts.push('');
        parts.push('## 技能指引');
        parts.push(skillsPrompt);
      }
    }

    // Knowledge index (tenant-scoped)
    const knowledgeEntries = scanKnowledgeFiles(knowledgeDir);
    if (knowledgeEntries.length > 0) {
      generateKnowledgeIndexFile(knowledgeEntries, knowledgeDir);
      const knowledgeIndex = generateKnowledgeIndex(knowledgeEntries);
      if (knowledgeIndex) {
        parts.push('');
        parts.push('## 知识库索引');
        parts.push(knowledgeIndex);
      }
    }

    // Always write to CLAUDE.md — Claude Code only reads this file.
    // Tenant-scoped content overwrites it before each query.
    const claudeMdPath = path.join(this.workDir, 'CLAUDE.md');
    fs.writeFileSync(claudeMdPath, parts.join('\n'), 'utf-8');
    console.log(`[claude-client] Generated CLAUDE.md${tenantId ? ` [tenant=${tenantId}]` : ''} (${parts.join('\n').length} bytes)`);
  }

  private getSessionKey(platform: string, userId: string, tenantId?: string): string {
    return tenantId ? `tenant:${tenantId}:${platform}:${userId}` : `${platform}:${userId}`;
  }

  private getSession(platform: string, userId: string, tenantId?: string): string | null {
    const key = this.getSessionKey(platform, userId, tenantId);
    const entry = this.sessions.get(key);
    if (!entry) return null;
    // Expire stale sessions
    if (Date.now() - entry.lastActiveAt > SESSION_TTL_MS) {
      this.sessions.delete(key);
      console.log(`[claude-client] Session expired for ${key}`);
      return null;
    }
    return entry.sessionId;
  }

  private setSession(platform: string, userId: string, sessionId: string, tenantId?: string): void {
    const key = this.getSessionKey(platform, userId, tenantId);
    this.sessions.set(key, { sessionId, lastActiveAt: Date.now() });
    console.log(`[claude-client] Session stored for ${key}: ${sessionId}`);
  }

  private touchSession(platform: string, userId: string, tenantId?: string): void {
    const key = this.getSessionKey(platform, userId, tenantId);
    const entry = this.sessions.get(key);
    if (entry) entry.lastActiveAt = Date.now();
  }

  private getProvider(): ProviderConfig {
    return loadProvider(this.providerConfigPath);
  }

  private buildSpawnEnv(provider: ProviderConfig, tenant?: TenantConfig | null): Record<string, string | undefined> {
    const env: Record<string, string | undefined> = { ...process.env, ...buildProviderEnv(provider) };

    if (tenant?.alicloud && tenant.alicloud.length > 0) {
      // Inject first account as default credentials
      const ali = tenant.alicloud[0];
      const ak = process.env[ali.access_key_env];
      const sk = process.env[ali.secret_key_env];
      if (ak && sk) {
        env.ALICLOUD_ACCESS_KEY_ID = ak;
        env.ALICLOUD_SECRET_ACCESS_KEY = sk;
        env.ALICLOUD_REGION = ali.region;
      } else {
        console.warn(`[claude-client] Alicloud env vars not set for tenant ${tenant.id}: ${ali.access_key_env}, ${ali.secret_key_env}`);
      }
      // Also inject each account as named env vars so aliyun CLI profiles can be configured
      for (const account of tenant.alicloud) {
        const prefix = `ALICLOUD_${account.name.toUpperCase().replace(/[^A-Z0-9]/g, '_')}`;
        const accAk = process.env[account.access_key_env];
        const accSk = process.env[account.secret_key_env];
        if (accAk) env[`${prefix}_AK`] = accAk;
        if (accSk) env[`${prefix}_SK`] = accSk;
        env[`${prefix}_REGION`] = account.region;
      }
    }

    // Strip other tenants' alicloud env vars for isolation
    for (const key of Object.keys(env)) {
      if (key.startsWith('ALICLOUD_') && key.endsWith('_AK')) delete env[key];
      if (key.startsWith('ALICLOUD_') && key.endsWith('_SK')) delete env[key];
    }

    return env;
  }

  private prepareQuery(platform: string, userId: string, tenantId?: string): string | null {
    const plugins = loadPlugins(this.pluginsConfigPath);
    generateMcpConfig(plugins, this.mcpConfigPath);
    this.generateClaudeMd(tenantId);
    return this.getSession(platform, userId, tenantId);
  }

  async query(userMessage: string, platform: string = '', userId: string = '', tenantId?: string): Promise<string> {
    const existingSessionId = this.prepareQuery(platform, userId, tenantId);
    const provider = this.getProvider();
    const model = provider.model || 'opus';
    const maxTurns = String(provider.max_turns || 20);
    const timeout = provider.timeout_ms || this.timeoutMs;

    const args = [
      '-p', userMessage,
      '--allowedTools', 'Bash(git*:deny),Read,Glob,Grep',
      '--output-format', 'json',
      '--max-turns', maxTurns,
      '--permission-mode', 'bypassPermissions',
      '--model', model,
    ];

    if (existingSessionId) {
      args.push('--resume', existingSessionId);
      console.log(`[claude-client] Resuming session ${existingSessionId} for ${platform}:${userId}`);
    } else {
      console.log(`[claude-client] Starting new session for ${platform}:${userId}`);
    }

    console.log(`[claude-client] Executing query (provider=${provider.type}, model=${model}): ${userMessage.substring(0, 100)}...`);

    const rawOutput = await new Promise<string>((resolve, reject) => {
      const child = spawn(
        'claude',
        args,
        {
          cwd: this.workDir,
          stdio: ['ignore', 'pipe', 'pipe'],
          env: this.buildSpawnEnv(provider, tenantId ? this.findTenant(tenantId) : null),
        },
      );

      console.log(`[claude-client] Spawned claude process PID: ${child.pid}`);

      let stdout = '';
      let stderr = '';
      child.stdout.on('data', (data: Buffer) => { stdout += data.toString(); });
      child.stderr.on('data', (data: Buffer) => { stderr += data.toString(); });

      const timer = setTimeout(() => {
        child.kill('SIGTERM');
        // Escalate to SIGKILL if still alive after 5s
        setTimeout(() => { if (!child.killed) child.kill('SIGKILL'); }, 5000);
        reject(new Error(`Claude Code timed out after ${timeout / 1000}s`));
      }, timeout);

      child.on('close', (code) => {
        clearTimeout(timer);
        if (code !== 0) {
          console.error(`[claude-client] stderr: ${stderr}`);
          reject(new Error(`Claude Code exited with code ${code}: ${stderr}`));
        } else {
          resolve(stdout.trim());
        }
      });

      child.on('error', (err) => {
        clearTimeout(timer);
        reject(new Error(`Claude Code spawn error: ${err.message}`));
      });
    });

    // Parse JSON output to extract session_id and result text
    const { text, sessionId } = this.parseJsonOutput(rawOutput);

    // Store/update session
    if (sessionId && platform && userId) {
      this.setSession(platform, userId, sessionId, tenantId);
    } else if (platform && userId) {
      this.touchSession(platform, userId, tenantId);
    }

    console.log(`[claude-client] Query completed, response length: ${text.length}`);
    return text;
  }

  /**
   * Streaming query — returns an AsyncGenerator of StreamChunk.
   * Uses --output-format stream-json --verbose.
   */
  async *queryStream(
    userMessage: string,
    platform: string = '',
    userId: string = '',
    tenantId?: string,
  ): AsyncGenerator<StreamChunk> {
    const existingSessionId = this.prepareQuery(platform, userId, tenantId);
    const provider = this.getProvider();
    const model = provider.model || 'opus';
    const maxTurns = String(provider.max_turns || 20);

    const args = [
      '-p', userMessage,
      '--allowedTools', 'Bash(git*:deny),Read,Glob,Grep',
      '--output-format', 'stream-json',
      '--verbose',
      '--max-turns', maxTurns,
      '--permission-mode', 'bypassPermissions',
      '--model', model,
    ];

    if (existingSessionId) {
      args.push('--resume', existingSessionId);
    }

    console.log(`[claude-client] Stream query (provider=${provider.type}, model=${model}): ${userMessage.substring(0, 100)}...`);

    const child = spawn('claude', args, {
      cwd: this.workDir,
      stdio: ['ignore', 'pipe', 'pipe'],
      env: this.buildSpawnEnv(provider, tenantId ? this.findTenant(tenantId) : null),
    });

    let stderr = '';
    let sessionId: string | null = null;
    let fullText = '';
    let buffer = '';
    let turn = 0;

    child.stderr.on('data', (data: Buffer) => { stderr += data.toString(); });

    const streamTimeout = provider.timeout_ms || this.timeoutMs;
    const timer = setTimeout(() => {
      child.kill('SIGTERM');
      setTimeout(() => { if (!child.killed) child.kill('SIGKILL'); }, 5000);
      console.error(`[claude-client] Stream timed out, PID: ${child.pid}`);
    }, streamTimeout);

    const chunks = this.childStdoutIterator(child);

    try {
      for await (const raw of chunks) {
        buffer += raw;
        const lines = buffer.split('\n');
        buffer = lines.pop() || '';

        for (const line of lines) {
          if (!line.trim()) continue;
          try {
            const event = JSON.parse(line);

            if (event.type === 'system' && event.subtype === 'init') {
              sessionId = event.session_id || null;
              yield { type: 'init', content: '', sessionId, turn: 0 };
            } else if (event.type === 'assistant' && event.message?.content) {
              for (const block of event.message.content) {
                if (block.type === 'thinking' && block.thinking) {
                  yield { type: 'thinking', content: block.thinking.substring(0, 200), turn };
                } else if (block.type === 'text' && block.text) {
                  fullText += block.text;
                  yield { type: 'text', content: block.text, turn };
                } else if (block.type === 'tool_use') {
                  yield {
                    type: 'tool_use',
                    content: `Using ${block.name}`,
                    toolName: block.name,
                    turn,
                  };
                }
              }
              turn++;
            } else if (event.type === 'result') {
              sessionId = event.session_id || sessionId;
              const durationMs = event.duration_ms || 0;
              if (event.result && !fullText) {
                fullText = typeof event.result === 'string'
                  ? event.result
                  : JSON.stringify(event.result);
                yield { type: 'text', content: fullText, turn };
              }
              if (sessionId && platform && userId) {
                this.setSession(platform, userId, sessionId, tenantId);
              } else if (platform && userId) {
                this.touchSession(platform, userId, tenantId);
              }
              yield { type: 'done', content: fullText, sessionId, turn, durationMs };
              console.log(`[claude-client] Stream completed, length: ${fullText.length}, turns: ${turn}`);
            }
          } catch {
            // Non-JSON line, skip
          }
        }
      }

      // Process remaining buffer
      if (buffer.trim()) {
        try {
          const event = JSON.parse(buffer);
          if (event.type === 'result') {
            sessionId = event.session_id || sessionId;
            if (event.result && !fullText) {
              fullText = typeof event.result === 'string' ? event.result : JSON.stringify(event.result);
            }
            if (sessionId && platform && userId) {
              this.setSession(platform, userId, sessionId, tenantId);
            }
            yield { type: 'done', content: fullText, sessionId, turn, durationMs: event.duration_ms || 0 };
          }
        } catch { /* ignore */ }
      }
    } finally {
      clearTimeout(timer);
      if (!child.killed) child.kill('SIGTERM');
    }
  }

  /**
   * Async iterator over child process stdout chunks.
   */
  private async *childStdoutIterator(child: ChildProcess): AsyncGenerator<string> {
    const buffer: string[] = [];
    let resolve: (() => void) | null = null;
    let done = false;

    child.stdout!.on('data', (data: Buffer) => {
      buffer.push(data.toString());
      if (resolve) { resolve(); resolve = null; }
    });

    child.on('close', () => {
      done = true;
      if (resolve) { resolve(); resolve = null; }
    });

    child.on('error', () => {
      done = true;
      if (resolve) { resolve(); resolve = null; }
    });

    while (true) {
      if (buffer.length > 0) {
        yield buffer.shift()!;
      } else if (done) {
        break;
      } else {
        await new Promise<void>(r => { resolve = r; });
      }
    }
  }

  private parseJsonOutput(raw: string): { text: string; sessionId: string | null } {
    try {
      const parsed = JSON.parse(raw);

      // Claude Code JSON output format: { result: "...", session_id: "..." }
      const sessionId = parsed.session_id || null;

      // Extract text from result
      let text = '';
      if (typeof parsed.result === 'string') {
        text = parsed.result;
      } else if (typeof parsed === 'string') {
        text = parsed;
      } else {
        // Fallback: stringify
        text = JSON.stringify(parsed.result ?? parsed, null, 2);
      }

      return { text, sessionId };
    } catch {
      // If JSON parsing fails, return raw output
      console.warn(`[claude-client] Failed to parse JSON output, using raw text`);
      return { text: raw, sessionId: null };
    }
  }
}
