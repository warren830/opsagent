import { PlatformAdapter, PlatformMessage } from './adapters/types';
import { ClaudeClient } from './claude-client';
import { AuditLogger } from './audit-logger';

export class MessageHandler {
  private readonly claudeClient: ClaudeClient;
  private readonly auditLogger: AuditLogger;

  constructor(claudeClient: ClaudeClient, auditLogger: AuditLogger) {
    this.claudeClient = claudeClient;
    this.auditLogger = auditLogger;
  }

  async handleMessage(adapter: PlatformAdapter, msg: PlatformMessage): Promise<void> {
    console.log(`[message-handler] [${msg.platform}] Received from ${msg.userName}: ${msg.text.substring(0, 100)}`);

    try {
      await adapter.sendTyping(msg);
      await adapter.sendReply(msg, 'Processing your query, this may take a few minutes...');
    } catch (e) {
      console.warn(`[message-handler] Could not send typing indicator: ${(e as Error).message}`);
    }

    const startTime = Date.now();

    try {
      const result = await this.claudeClient.query(msg.text, msg.platform, msg.userId);
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
      });

      if (errMsg.includes('timed out')) {
        await adapter.sendReply(
          msg,
          'Your query timed out after 5 minutes. Please try a simpler query or break it into smaller parts.',
        ).catch(() => {});
      } else {
        await adapter.sendReply(msg, `Error processing your query: ${errMsg}`).catch(() => {});
      }
    }
  }
}
