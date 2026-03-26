/**
 * ClaudeClient — Anthropic SDK based agent loop.
 * Replaces the old CLI-spawn approach with direct API calls.
 *
 * Public interface is unchanged: query() and queryStream() return
 * the same types consumed by message-handler.ts and index.ts.
 */
import * as fs from 'fs';
import * as path from 'path';
import Anthropic from '@anthropic-ai/sdk';
import { loadProvider, buildSdkClient, resolveModelId, SdkClient, ProviderConfig } from './provider-loader';
import { loadGlossary } from './glossary-loader';
import { loadSkills, Skill } from './skills-loader';
import { loadTenants, TenantConfig } from './tenant-loader';
import { buildSystemPrompt, SystemPromptConfig } from './system-prompt-builder';
import { buildToolSet, ToolContext } from './tool-definitions';
import { executeTool, ToolExecutorConfig } from './tool-executor';
import { SandboxConfig } from './command-sandbox';
import { ApprovalStore } from './approval-store';

const SESSION_TTL_MS = 30 * 60 * 1000; // 30 minutes
const DEFAULT_MAX_TURNS = 20;
const DEFAULT_TIMEOUT_MS = 5 * 60 * 1000;

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
  messages: Anthropic.MessageParam[];
  lastActiveAt: number;
}

export class ClaudeClient {
  private readonly workDir: string;
  private readonly glossaryConfigPath: string;
  private readonly accountsConfigPath: string;
  private readonly skillsConfigPath: string;
  private readonly knowledgeDir: string;
  private readonly providerConfigPath: string;
  private readonly tenantsConfigPath: string;
  private readonly timeoutMs: number;
  private readonly sessions = new Map<string, SessionEntry>();
  public approvalStore?: ApprovalStore;

  constructor(options: ClaudeClientOptions) {
    this.workDir = options.workDir;
    this.glossaryConfigPath = options.glossaryConfigPath ?? path.join(this.workDir, 'config/glossary.yaml');
    this.accountsConfigPath = options.accountsConfigPath ?? path.join(this.workDir, 'config/accounts.yaml');
    this.skillsConfigPath = options.skillsConfigPath ?? path.join(this.workDir, 'config/skills.yaml');
    this.knowledgeDir = options.knowledgeDir ?? path.join(this.workDir, 'knowledge');
    this.providerConfigPath = options.providerConfigPath ?? path.join(this.workDir, 'config/providers.yaml');
    this.tenantsConfigPath = options.tenantsConfigPath ?? path.join(this.workDir, 'config/tenants.yaml');
    this.timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  }

  // ── Public API ──────────────────────────────────────────────

  /**
   * Non-streaming query. Returns final text response.
   */
  async query(
    userMessage: string,
    platform = '',
    userId = '',
    tenantId?: string,
  ): Promise<string> {
    const ctx = this.prepareContext(tenantId, platform, userId);
    const session = this.getOrCreateSession(platform, userId, tenantId);
    session.messages.push({ role: 'user', content: userMessage });

    const startTime = Date.now();
    let turn = 0;

    while (turn < ctx.maxTurns) {
      // Timeout check
      if (Date.now() - startTime > this.timeoutMs) {
        throw new Error(`Query timed out after ${this.timeoutMs / 1000}s`);
      }

      const response = await ctx.client.messages.create({
        model: ctx.model,
        max_tokens: 16384,
        system: ctx.systemPrompt,
        tools: ctx.tools,
        messages: session.messages,
      });

      session.messages.push({ role: 'assistant', content: response.content });

      if (response.stop_reason === 'end_turn' || !this.hasToolUse(response.content)) {
        session.lastActiveAt = Date.now();
        return this.extractText(response.content);
      }

      // Execute tools and continue loop
      const toolResults = await this.executeToolCalls(response.content, ctx.toolConfig);
      session.messages.push({ role: 'user', content: toolResults });
      turn++;
    }

    return 'Reached maximum tool call turns.';
  }

  /**
   * Streaming query. Yields StreamChunk objects compatible with
   * message-handler.ts and the admin SSE endpoint.
   */
  async *queryStream(
    userMessage: string,
    platform = '',
    userId = '',
    tenantId?: string,
  ): AsyncGenerator<StreamChunk> {
    const ctx = this.prepareContext(tenantId, platform, userId);
    const session = this.getOrCreateSession(platform, userId, tenantId);
    session.messages.push({ role: 'user', content: userMessage });

    const sessionKey = this.getSessionKey(platform, userId, tenantId);
    yield { type: 'init', content: '', sessionId: sessionKey, turn: 0 };

    const startTime = Date.now();
    let turn = 0;
    let fullText = '';

    while (turn < ctx.maxTurns) {
      if (Date.now() - startTime > this.timeoutMs) {
        yield { type: 'text', content: '\n\n(Query timed out)', turn };
        break;
      }

      const stream = ctx.client.messages.stream({
        model: ctx.model,
        max_tokens: 16384,
        system: ctx.systemPrompt,
        tools: ctx.tools,
        messages: session.messages,
      });

      // Stream text deltas to the caller
      for await (const event of stream) {
        if (event.type === 'content_block_delta') {
          const delta = event.delta as any;
          if (delta.type === 'text_delta') {
            fullText += delta.text;
            yield { type: 'text', content: delta.text, turn };
          } else if (delta.type === 'thinking_delta') {
            yield { type: 'thinking', content: (delta.thinking || '').substring(0, 200), turn };
          }
        }
      }

      const finalMessage = await stream.finalMessage();
      session.messages.push({ role: 'assistant', content: finalMessage.content });

      // Check if done
      if (finalMessage.stop_reason === 'end_turn' || !this.hasToolUse(finalMessage.content)) {
        break;
      }

      // Execute tools
      const toolUseBlocks = finalMessage.content.filter(
        (b): b is Anthropic.ToolUseBlock => b.type === 'tool_use',
      );
      for (const block of toolUseBlocks) {
        yield { type: 'tool_use', content: `Calling ${block.name}`, toolName: block.name, turn };
      }

      const toolResults = await this.executeToolCalls(finalMessage.content, ctx.toolConfig);
      session.messages.push({ role: 'user', content: toolResults });
      turn++;
    }

    session.lastActiveAt = Date.now();

    yield {
      type: 'done',
      content: fullText,
      sessionId: sessionKey,
      turn,
      durationMs: Date.now() - startTime,
    };
  }

  // ── Private helpers ──────────────────────────────────────────

  private prepareContext(tenantId?: string, platform?: string, userId?: string) {
    const provider = this.loadProviderConfig();
    const client = buildSdkClient(provider);
    const model = resolveModelId(provider);
    const maxTurns = provider.max_turns || DEFAULT_MAX_TURNS;
    const tenant = tenantId ? this.findTenant(tenantId) : null;

    // Load data for system prompt and tools
    const tenantDir = tenantId ? path.join(this.knowledgeDir, '_tenants', tenantId) : null;
    const tenantConfigDir = tenantDir ? path.join(tenantDir, '_config') : null;
    const glossaryPath = tenantConfigDir && fs.existsSync(path.join(tenantConfigDir, 'glossary.yaml'))
      ? path.join(tenantConfigDir, 'glossary.yaml')
      : this.glossaryConfigPath;
    const skillsPath = tenantConfigDir && fs.existsSync(path.join(tenantConfigDir, 'skills.yaml'))
      ? path.join(tenantConfigDir, 'skills.yaml')
      : this.skillsConfigPath;

    const glossaryConfig = loadGlossary(glossaryPath);
    const glossary = glossaryConfig?.glossary || null;
    const skillsConfig = loadSkills(skillsPath);
    const enabledSkills = skillsConfig?.skills?.filter((s: Skill) => s.enabled) || [];

    // System prompt
    const systemPrompt = buildSystemPrompt({
      workDir: this.workDir,
      glossaryConfigPath: glossaryPath,
      accountsConfigPath: this.accountsConfigPath,
      skillsConfigPath: skillsPath,
      knowledgeDir: tenantDir && fs.existsSync(tenantDir) ? tenantDir : this.knowledgeDir,
      tenantId,
      tenant,
    });

    // Tools
    const tools = buildToolSet({
      hasGlossary: !!glossary && Object.keys(glossary || {}).length > 0,
      hasSkills: enabledSkills.length > 0,
      skillNames: enabledSkills.map((s: Skill) => s.name),
      enableEnrichedTools: true,
    });

    // Tool executor config
    const env: Record<string, string | undefined> = { ...process.env };
    // Inject alicloud creds for tenant
    if (tenant?.alicloud && tenant.alicloud.length > 0) {
      const ali = tenant.alicloud[0];
      const ak = process.env[ali.access_key_env];
      const sk = process.env[ali.secret_key_env];
      if (ak && sk) {
        env.ALICLOUD_ACCESS_KEY_ID = ak;
        env.ALICLOUD_SECRET_ACCESS_KEY = sk;
        env.ALICLOUD_REGION = ali.region;
      }
      for (const account of tenant.alicloud) {
        const prefix = `ALICLOUD_${account.name.toUpperCase().replace(/[^A-Z0-9]/g, '_')}`;
        const accAk = process.env[account.access_key_env];
        const accSk = process.env[account.secret_key_env];
        if (accAk) env[`${prefix}_AK`] = accAk;
        if (accSk) env[`${prefix}_SK`] = accSk;
        env[`${prefix}_REGION`] = account.region;
      }
    }

    const sandboxConfig: SandboxConfig = {
      allowedAwsAccountIds: tenant?.aws_account_ids,
      kubectlReadOnly: true, // default: write ops need approval
    };

    const toolConfig: ToolExecutorConfig = {
      workDir: this.workDir,
      knowledgeDir: tenantDir && fs.existsSync(tenantDir) ? tenantDir : this.knowledgeDir,
      sandboxConfig,
      env,
      glossary: glossary as Record<string, any> | undefined,
      skills: skillsConfig?.skills,
      approvalStore: this.approvalStore,
      userId,
      userName: userId,
      platform,
      channelId: '',
      tenantId,
    };

    const providerLabel = `${provider.type}/${model.split('-').slice(0, 2).join('-')}`;
    console.log(`[claude-client] SDK query (provider=${providerLabel}${tenantId ? `, tenant=${tenantId}` : ''}, tools=${tools.length})`);

    return { client, model, maxTurns, systemPrompt, tools, toolConfig };
  }

  private loadProviderConfig(): ProviderConfig {
    return loadProvider(this.providerConfigPath);
  }

  private findTenant(tenantId: string): TenantConfig | null {
    const tenantsFile = loadTenants(this.tenantsConfigPath);
    return tenantsFile?.tenants?.find(t => t.id === tenantId) || null;
  }

  private getSessionKey(platform: string, userId: string, tenantId?: string): string {
    return tenantId ? `tenant:${tenantId}:${platform}:${userId}` : `${platform}:${userId}`;
  }

  private getOrCreateSession(platform: string, userId: string, tenantId?: string): SessionEntry {
    const key = this.getSessionKey(platform, userId, tenantId);
    const existing = this.sessions.get(key);

    if (existing && Date.now() - existing.lastActiveAt < SESSION_TTL_MS) {
      return existing;
    }

    // New session or expired
    const session: SessionEntry = { messages: [], lastActiveAt: Date.now() };
    this.sessions.set(key, session);
    return session;
  }

  private hasToolUse(content: Anthropic.ContentBlock[]): boolean {
    return content.some(b => b.type === 'tool_use');
  }

  private extractText(content: Anthropic.ContentBlock[]): string {
    return content
      .filter((b): b is Anthropic.TextBlock => b.type === 'text')
      .map(b => b.text)
      .join('');
  }

  private async executeToolCalls(
    content: Anthropic.ContentBlock[],
    config: ToolExecutorConfig,
  ): Promise<Anthropic.ToolResultBlockParam[]> {
    const toolUseBlocks = content.filter(
      (b): b is Anthropic.ToolUseBlock => b.type === 'tool_use',
    );

    const results: Anthropic.ToolResultBlockParam[] = [];

    for (const block of toolUseBlocks) {
      console.log(`[claude-client] Tool call: ${block.name}(${JSON.stringify(block.input).substring(0, 100)})`);
      const result = await executeTool(
        block.name,
        block.input as Record<string, any>,
        config,
      );
      console.log(`[claude-client] Tool result: ${result.is_error ? 'ERROR' : 'OK'} (${result.content.length} chars)`);
      results.push({
        type: 'tool_result',
        tool_use_id: block.id,
        content: result.content,
        is_error: result.is_error,
      });
    }

    return results;
  }
}
