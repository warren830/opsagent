import * as fs from 'fs';
import * as crypto from 'crypto';
import { PlatformAdapter, PlatformMessage } from './adapters/types';
import { ClaudeClient } from './claude-client';
import { AuditLogger } from './audit-logger';
import { TenantResolver } from './tenant-resolver';

const STREAM_UPDATE_INTERVAL_MS = 2000; // Throttle IM updates to every 2 seconds

export class MessageHandler {
  private readonly claudeClient: ClaudeClient;
  private readonly auditLogger: AuditLogger;
  private readonly tenantResolver?: TenantResolver;

  constructor(claudeClient: ClaudeClient, auditLogger: AuditLogger, tenantResolver?: TenantResolver) {
    this.claudeClient = claudeClient;
    this.auditLogger = auditLogger;
    this.tenantResolver = tenantResolver;
  }

  /**
   * Build the final query text, injecting attachment file paths if present.
   */
  private buildQuery(msg: PlatformMessage): string {
    let query = msg.text;
    if (msg.attachments && msg.attachments.length > 0) {
      const fileList = msg.attachments
        .map(a => `- ${a.fileName} (${a.mimeType}): ${a.filePath}`)
        .join('\n');
      query += `\n\n用户附带了以下文件，请用 Read 工具查看并分析:\n${fileList}`;
    }
    return query;
  }

  /**
   * Clean up temporary attachment files after query completes.
   */
  private cleanupAttachments(msg: PlatformMessage): void {
    if (!msg.attachments) return;
    for (const att of msg.attachments) {
      try {
        if (fs.existsSync(att.filePath)) {
          const dir = require('path').dirname(att.filePath);
          fs.rmSync(dir, { recursive: true, force: true });
          console.log(`[message-handler] Cleaned up temp dir: ${dir}`);
        }
      } catch { /* best effort */ }
    }
  }

  async handleMessage(adapter: PlatformAdapter, msg: PlatformMessage): Promise<void> {
    msg.requestId = crypto.randomUUID().substring(0, 8);
    console.log(`[message-handler] [${msg.requestId}] [${msg.platform}] Received from ${msg.userName}: ${msg.text.substring(0, 100)}`);
    if (msg.attachments?.length) {
      console.log(`[message-handler] Attachments: ${msg.attachments.map(a => a.fileName).join(', ')}`);
    }

    // Tenant resolution gate
    if (this.tenantResolver) {
      const resolution = this.tenantResolver.resolve(msg.platform, msg.conversationId);
      if (resolution.denied) {
        console.log(`[message-handler] Tenant denied for ${msg.platform}:${msg.conversationId}: ${resolution.denyReason}`);
        await adapter.sendReply(msg, resolution.denyReason || 'Access denied').catch(() => {});
        return;
      }
      if (resolution.tenant) {
        msg.tenantId = resolution.tenant.id;
        console.log(`[message-handler] Tenant resolved: ${resolution.tenant.id} (${resolution.tenant.name})`);
      }
    }

    // If adapter supports updateReply, use streaming path
    if (adapter.updateReply) {
      return this.handleStreamingMessage(adapter, msg);
    }

    // Fallback: non-streaming path
    return this.handleNonStreamingMessage(adapter, msg);
  }

  private async handleStreamingMessage(adapter: PlatformAdapter, msg: PlatformMessage): Promise<void> {
    const startTime = Date.now();
    let replyMsgId: string | undefined;
    let fullText = '';
    let lastUpdateAt = 0;
    let statusText = 'Processing...';

    // Send initial "processing" message and get its message ID
    try {
      replyMsgId = await adapter.updateReply!(msg, statusText);
    } catch (e) {
      console.warn(`[message-handler] Could not send initial reply: ${(e as Error).message}`);
    }

    try {
      const query = this.buildQuery(msg);
      for await (const chunk of this.claudeClient.queryStream(query, msg.platform, msg.userId, msg.tenantId)) {
        const now = Date.now();

        if (chunk.type === 'text') {
          fullText += chunk.content;
          // Throttle updates to IM platform
          if (now - lastUpdateAt >= STREAM_UPDATE_INTERVAL_MS && replyMsgId) {
            try {
              replyMsgId = await adapter.updateReply!(msg, fullText + ' \u258D', replyMsgId);
              lastUpdateAt = now;
            } catch { /* best effort */ }
          }
        } else if (chunk.type === 'thinking') {
          statusText = `Thinking... (turn ${chunk.turn})`;
          if (now - lastUpdateAt >= STREAM_UPDATE_INTERVAL_MS && replyMsgId && !fullText) {
            try {
              replyMsgId = await adapter.updateReply!(msg, statusText, replyMsgId);
              lastUpdateAt = now;
            } catch { /* best effort */ }
          }
        } else if (chunk.type === 'tool_use') {
          statusText = `Using ${chunk.toolName}...`;
          if (now - lastUpdateAt >= STREAM_UPDATE_INTERVAL_MS && replyMsgId) {
            const display = fullText ? fullText + `\n\n_${statusText}_` : statusText;
            try {
              replyMsgId = await adapter.updateReply!(msg, display, replyMsgId);
              lastUpdateAt = now;
            } catch { /* best effort */ }
          }
        } else if (chunk.type === 'done') {
          // Use final full text from done event as authoritative
          if (chunk.content) fullText = chunk.content;
        }
      }

      const durationMs = Date.now() - startTime;

      // Final update without cursor
      if (replyMsgId && fullText) {
        try {
          await adapter.updateReply!(msg, fullText, replyMsgId);
        } catch { /* best effort */ }
      } else if (!fullText) {
        try {
          await adapter.updateReply!(msg, 'Query completed but returned no results.', replyMsgId);
        } catch { /* best effort */ }
      }

      console.log(`[message-handler] Stream result: ${fullText.substring(0, 200)}`);

      this.auditLogger.log({
        platform: msg.platform,
        userId: msg.userId,
        userName: msg.userName,
        query: msg.text,
        responseLength: fullText.length,
        durationMs,
        success: true,
        sessionId: msg.conversationId,
        tenantId: msg.tenantId,
        requestId: msg.requestId,
      });

    } catch (error: unknown) {
      const durationMs = Date.now() - startTime;
      const errMsg = error instanceof Error ? error.message : String(error);
      console.error(`[message-handler] Stream error: ${errMsg}`);

      this.auditLogger.log({
        platform: msg.platform,
        userId: msg.userId,
        userName: msg.userName,
        query: msg.text,
        responseLength: 0,
        durationMs,
        success: false,
        error: errMsg,
        sessionId: msg.conversationId,
        tenantId: msg.tenantId,
        requestId: msg.requestId,
      });

      const errorText = errMsg.includes('timed out')
        ? 'Your query timed out after 5 minutes. Please try a simpler query.'
        : `Error: ${errMsg}`;

      if (replyMsgId) {
        try { await adapter.updateReply!(msg, errorText, replyMsgId); } catch { /* */ }
      } else {
        await adapter.sendReply(msg, errorText).catch(() => {});
      }
    } finally {
      this.cleanupAttachments(msg);
    }
  }

  private async handleNonStreamingMessage(adapter: PlatformAdapter, msg: PlatformMessage): Promise<void> {
    try {
      await adapter.sendTyping(msg);
      await adapter.sendReply(msg, 'Processing your query, this may take a few minutes...');
    } catch (e) {
      console.warn(`[message-handler] Could not send typing indicator: ${(e as Error).message}`);
    }

    const startTime = Date.now();

    try {
      const query = this.buildQuery(msg);
      const result = await this.claudeClient.query(query, msg.platform, msg.userId, msg.tenantId);
      const durationMs = Date.now() - startTime;

      console.log(`[message-handler] Claude Code result: ${result.substring(0, 200)}`);

      this.auditLogger.log({
        platform: msg.platform,
        userId: msg.userId,
        userName: msg.userName,
        query: msg.text,
        responseLength: result.length,
        durationMs,
        success: true,
        sessionId: msg.conversationId,
        tenantId: msg.tenantId,
        requestId: msg.requestId,
      });

      if (!result) {
        await adapter.sendReply(msg, 'Query completed but returned no results.').catch(() => {});
        return;
      }

      await adapter.sendReply(msg, result).catch(() => {});
    } catch (error: unknown) {
      const durationMs = Date.now() - startTime;
      const errMsg = error instanceof Error ? error.message : String(error);
      console.error(`[message-handler] Error processing query: ${errMsg}`);

      this.auditLogger.log({
        platform: msg.platform,
        userId: msg.userId,
        userName: msg.userName,
        query: msg.text,
        responseLength: 0,
        durationMs,
        success: false,
        error: errMsg,
        sessionId: msg.conversationId,
        tenantId: msg.tenantId,
        requestId: msg.requestId,
      });

      if (errMsg.includes('timed out')) {
        await adapter.sendReply(
          msg,
          'Your query timed out after 5 minutes. Please try a simpler query or break it into smaller parts.',
        ).catch(() => {});
      } else {
        await adapter.sendReply(msg, `Error processing your query: ${errMsg}`).catch(() => {});
      }
    } finally {
      this.cleanupAttachments(msg);
    }
  }
}
