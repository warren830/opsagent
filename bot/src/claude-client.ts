import { ChildProcess, spawn } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';
import { loadPlugins, generateMcpConfig } from './plugin-loader';
import { loadGlossary, generateGlossaryKnowledge } from './glossary-loader';
import { loadAccounts, generateAccountsKnowledge } from './accounts-loader';
import { loadSkills, generateSkillsPrompt } from './skills-loader';

const CLAUDE_TIMEOUT_MS = 5 * 60 * 1000; // 5 minutes
const SESSION_TTL_MS = 30 * 60 * 1000; // 30 minutes idle -> expire session

const BASE_SYSTEM_PROMPT = `你是 OpsAgent，一个多云基础设施查询助手。
支持 AWS、阿里云、Azure、GCP 等多云平台。
使用 aws cli 查询 AWS 资源，使用 aliyun cli 查询阿里云资源，使用 az cli 查询 Azure 资源，使用 gcloud 查询 GCP 资源。
使用 kubectl 查询 Kubernetes 集群。
使用 MCP 工具查询 Confluence 文档、Jira 工单等外部数据源。
AWS 跨账号查询时使用 ./scripts/foreach-account.sh 自动遍历所有 Organizations 账号。
阿里云多账号查询时使用 aliyun configure 切换 profile。
knowledge/ 目录包含公司的知识库文件（runbook、架构文档等），遇到不确定的问题时先用 Grep 在该目录搜索相关内容，再用 Read 读取匹配的文件。
始终返回结构化、易读的 Markdown 表格结果。`;

export interface ClaudeClientOptions {
  workDir: string;
  pluginsConfigPath: string;
  mcpConfigPath: string;
  glossaryConfigPath?: string;
  accountsConfigPath?: string;
  skillsConfigPath?: string;
  knowledgeDir?: string;
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
  private readonly timeoutMs: number;

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
    this.timeoutMs = options.timeoutMs ?? CLAUDE_TIMEOUT_MS;
  }

  /**
   * Generate CLAUDE.md in workDir — Claude Code reads this natively.
   * Also generates knowledge files (glossary.md, accounts.md, skills/*.md).
   */
  private generateClaudeMd(): void {
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
        parts.push('## 已配置的云账号');
        parts.push(summary);
        parts.push('完整账号详情见 knowledge/accounts.md 文件。');
      }
    }

    // Skills (progressive: index only, full instructions in knowledge/skills/)
    const skillsConfig = loadSkills(this.skillsConfigPath);
    if (skillsConfig) {
      const skillsPrompt = generateSkillsPrompt(skillsConfig, this.knowledgeDir);
      if (skillsPrompt) {
        parts.push('');
        parts.push('## 技能指引');
        parts.push(skillsPrompt);
      }
    }

    const claudeMdPath = path.join(this.workDir, 'CLAUDE.md');
    fs.writeFileSync(claudeMdPath, parts.join('\n'), 'utf-8');
    console.log(`[claude-client] Generated CLAUDE.md (${parts.join('\n').length} bytes)`);
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

  private prepareQuery(platform: string, userId: string): string | null {
    const plugins = loadPlugins(this.pluginsConfigPath);
    generateMcpConfig(plugins, this.mcpConfigPath);
    this.generateClaudeMd();
    return this.getSession(platform, userId);
  }

  async query(userMessage: string, platform: string = '', userId: string = ''): Promise<string> {
    const existingSessionId = this.prepareQuery(platform, userId);

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

  /**
   * Streaming query — returns an AsyncGenerator of StreamChunk.
   * Uses --output-format stream-json --verbose.
   */
  async *queryStream(
    userMessage: string,
    platform: string = '',
    userId: string = '',
  ): AsyncGenerator<StreamChunk> {
    const existingSessionId = this.prepareQuery(platform, userId);

    const args = [
      '-p', userMessage,
      '--allowedTools', 'Bash(git*:deny),Read,Glob,Grep',
      '--output-format', 'stream-json',
      '--verbose',
      '--max-turns', '20',
      '--dangerously-skip-permissions',
      '--model', 'opus',
    ];

    if (existingSessionId) {
      args.push('--resume', existingSessionId);
    }

    console.log(`[claude-client] Stream query: ${userMessage.substring(0, 100)}...`);

    const child = spawn('claude', args, {
      cwd: this.workDir,
      stdio: ['ignore', 'pipe', 'pipe'],
      env: { ...process.env, CLAUDE_CODE_USE_BEDROCK: '1' },
    });

    let stderr = '';
    let sessionId: string | null = null;
    let fullText = '';
    let buffer = '';
    let turn = 0;

    child.stderr.on('data', (data: Buffer) => { stderr += data.toString(); });

    const timer = setTimeout(() => {
      child.kill('SIGTERM');
      console.error(`[claude-client] Stream timed out, PID: ${child.pid}`);
    }, this.timeoutMs);

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
                this.setSession(platform, userId, sessionId);
              } else if (platform && userId) {
                this.touchSession(platform, userId);
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
              this.setSession(platform, userId, sessionId);
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
