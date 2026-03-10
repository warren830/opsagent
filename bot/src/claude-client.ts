import { spawn } from 'child_process';
import * as path from 'path';
import { loadPlugins, generateMcpConfig } from './plugin-loader';
import { loadGlossary, generateGlossaryKnowledge } from './glossary-loader';
import { loadAccounts, generateAccountsKnowledge } from './accounts-loader';

const CLAUDE_TIMEOUT_MS = 5 * 60 * 1000; // 5 minutes
const SESSION_TTL_MS = 30 * 60 * 1000; // 30 minutes idle -> expire session

const BASE_SYSTEM_PROMPT = `你是 OpsAgent，一个基础设施查询助手。
使用 aws cli、kubectl 等工具查询 AWS 资源。
使用 MCP 工具查询 Confluence 文档、Jira 工单等外部数据源。
跨账号查询时使用 ./scripts/foreach-account.sh 自动遍历所有 Organizations 账号。
knowledge/ 目录包含公司的知识库文件（runbook、架构文档等），遇到不确定的问题时先用 Grep 在该目录搜索相关内容，再用 Read 读取匹配的文件。
始终返回结构化、易读的 Markdown 表格结果。`;

export interface ClaudeClientOptions {
  workDir: string;
  pluginsConfigPath: string;
  mcpConfigPath: string;
  glossaryConfigPath?: string;
  accountsConfigPath?: string;
  knowledgeDir?: string;
  timeoutMs?: number;
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
  private readonly knowledgeDir: string;
  private readonly timeoutMs: number;

  /** Map of "platform:userId" -> session info */
  private readonly sessions = new Map<string, SessionEntry>();

  constructor(options: ClaudeClientOptions) {
    this.workDir = options.workDir;
    this.pluginsConfigPath = options.pluginsConfigPath;
    this.mcpConfigPath = options.mcpConfigPath;
    this.glossaryConfigPath = options.glossaryConfigPath ?? path.join(this.workDir, 'config/glossary.yaml');
    this.accountsConfigPath = options.accountsConfigPath ?? path.join(this.workDir, 'config/accounts.yaml');
    this.knowledgeDir = options.knowledgeDir ?? path.join(this.workDir, 'knowledge');
    this.timeoutMs = options.timeoutMs ?? CLAUDE_TIMEOUT_MS;
  }

  private buildSystemPrompt(): string {
    const parts: string[] = [BASE_SYSTEM_PROMPT];

    // Glossary summary
    const glossary = loadGlossary(this.glossaryConfigPath);
    if (glossary) {
      const summary = generateGlossaryKnowledge(glossary, this.knowledgeDir);
      if (summary) {
        parts.push('');
        parts.push('## 公司术语速查');
        parts.push(summary);
        parts.push('完整术语详情见 knowledge/glossary.md 文件。');
      }
    }

    // Accounts summary
    const accounts = loadAccounts(this.accountsConfigPath);
    if (accounts) {
      const summary = generateAccountsKnowledge(accounts, this.knowledgeDir);
      if (summary) {
        parts.push('');
        parts.push('## 已配置的 AWS 账号');
        parts.push(summary);
        parts.push('完整账号详情见 knowledge/accounts.md 文件。');
      }
    }

    return parts.join('\n');
  }

  private getSessionKey(platform: string, userId: string): string {
    return `${platform}:${userId}`;
  }

  private getSession(platform: string, userId: string): string | null {
    const key = this.getSessionKey(platform, userId);
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

  private setSession(platform: string, userId: string, sessionId: string): void {
    const key = this.getSessionKey(platform, userId);
    this.sessions.set(key, { sessionId, lastActiveAt: Date.now() });
    console.log(`[claude-client] Session stored for ${key}: ${sessionId}`);
  }

  private touchSession(platform: string, userId: string): void {
    const key = this.getSessionKey(platform, userId);
    const entry = this.sessions.get(key);
    if (entry) entry.lastActiveAt = Date.now();
  }

  async query(userMessage: string, platform: string = '', userId: string = ''): Promise<string> {
    // Load and apply plugin configuration before each query
    const plugins = loadPlugins(this.pluginsConfigPath);
    generateMcpConfig(plugins, this.mcpConfigPath);

    // Build dynamic system prompt (also generates knowledge files)
    const systemPrompt = this.buildSystemPrompt();

    // Check for existing session
    const existingSessionId = this.getSession(platform, userId);

    const args = [
      '-p', userMessage,
      '--allowedTools', 'Bash(git*:deny),Read,Glob,Grep',
      '--output-format', 'json',
      '--max-turns', '20',
      '--dangerously-skip-permissions',
      '--model', 'opus',
    ];

    if (existingSessionId) {
      args.push('--resume', existingSessionId);
      console.log(`[claude-client] Resuming session ${existingSessionId} for ${platform}:${userId}`);
    } else {
      // Only set system prompt for new sessions
      args.push('--system-prompt', systemPrompt);
      console.log(`[claude-client] Starting new session for ${platform}:${userId}`);
    }

    console.log(`[claude-client] Executing query: ${userMessage.substring(0, 100)}...`);

    const rawOutput = await new Promise<string>((resolve, reject) => {
      const child = spawn(
        'claude',
        args,
        {
          cwd: this.workDir,
          stdio: ['ignore', 'pipe', 'pipe'],
          env: {
            ...process.env,
            CLAUDE_CODE_USE_BEDROCK: '1',
          },
        },
      );

      console.log(`[claude-client] Spawned claude process PID: ${child.pid}`);

      let stdout = '';
      let stderr = '';
      child.stdout.on('data', (data: Buffer) => { stdout += data.toString(); });
      child.stderr.on('data', (data: Buffer) => { stderr += data.toString(); });

      const timer = setTimeout(() => {
        child.kill('SIGTERM');
        reject(new Error(`Claude Code timed out after ${this.timeoutMs / 1000}s`));
      }, this.timeoutMs);

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
      this.setSession(platform, userId, sessionId);
    } else if (platform && userId) {
      this.touchSession(platform, userId);
    }

    console.log(`[claude-client] Query completed, response length: ${text.length}`);
    return text;
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
