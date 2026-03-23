import * as http from 'http';
import * as crypto from 'crypto';
import * as fs from 'fs';
import * as https from 'https';
import * as path from 'path';
import { MessageAttachment, PlatformAdapter, PlatformMessage, truncateForPlatform } from './types';

const UPLOAD_DIR = '/tmp/opsagent-uploads';

export interface FeishuAdapterOptions {
  appId: string;
  appSecret: string;
  verificationToken?: string;  // For webhook verification
  encryptKey?: string;         // For event encryption (optional)
}

export class FeishuAdapter implements PlatformAdapter {
  readonly platform = 'feishu';
  private readonly appId: string;
  private readonly appSecret: string;
  private readonly verificationToken: string;
  private tenantAccessToken: string = '';
  private tokenExpiresAt: number = 0;

  constructor(options: FeishuAdapterOptions) {
    this.appId = options.appId;
    this.appSecret = options.appSecret;
    this.verificationToken = options.verificationToken || '';
  }

  async handleWebhook(
    req: http.IncomingMessage,
    res: http.ServerResponse,
    body: any,
  ): Promise<PlatformMessage | null> {
    // Feishu URL verification challenge
    if (body.type === 'url_verification') {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ challenge: body.challenge }));
      return null;
    }

    // v2 event format: body.header.event_type
    const header = body.header;
    const event = body.event;

    if (!header || !event) {
      res.writeHead(200);
      res.end();
      return null;
    }

    // Only handle im.message.receive_v1
    if (header.event_type !== 'im.message.receive_v1') {
      res.writeHead(200);
      res.end();
      return null;
    }

    const message = event.message;
    if (!message) {
      res.writeHead(200);
      res.end();
      return null;
    }

    const msgType = message.message_type;
    const supportedTypes = ['text', 'image', 'file'];
    if (!supportedTypes.includes(msgType)) {
      res.writeHead(200);
      res.end();
      return null;
    }

    // Acknowledge immediately
    res.writeHead(200);
    res.end();

    let text = '';
    const attachments: MessageAttachment[] = [];

    if (msgType === 'text') {
      // Parse text content (Feishu wraps text in JSON: {"text":"actual message"})
      try {
        const content = JSON.parse(message.content);
        text = content.text || '';
      } catch {
        text = message.content || '';
      }
      // Strip @mentions (format: @_user_xxx)
      text = text.replace(/@_user_\w+/g, '').trim();
      if (!text) return null;
    } else if (msgType === 'image') {
      // Image: {"image_key":"img_xxx"}
      try {
        const content = JSON.parse(message.content);
        const imageKey = content.image_key;
        if (imageKey) {
          const filePath = await this.downloadFeishuResource(message.message_id, imageKey, 'image');
          if (filePath) {
            attachments.push({ filePath, fileName: path.basename(filePath), mimeType: 'image/png' });
            text = '(用户发送了一张图片)';
          }
        }
      } catch (e) {
        console.error(`[feishu] Failed to process image: ${(e as Error).message}`);
        return null;
      }
    } else if (msgType === 'file') {
      // File: {"file_key":"file_xxx","file_name":"xxx.pdf"}
      try {
        const content = JSON.parse(message.content);
        const fileKey = content.file_key;
        const fileName = content.file_name || 'unknown';
        if (fileKey) {
          const filePath = await this.downloadFeishuResource(message.message_id, fileKey, 'file');
          if (filePath) {
            // Rename to original filename
            const finalPath = path.join(path.dirname(filePath), fileName);
            fs.renameSync(filePath, finalPath);
            const ext = path.extname(fileName).toLowerCase();
            const mimeMap: Record<string, string> = {
              '.pdf': 'application/pdf', '.png': 'image/png', '.jpg': 'image/jpeg',
              '.jpeg': 'image/jpeg', '.txt': 'text/plain', '.csv': 'text/csv',
              '.json': 'application/json', '.yaml': 'text/yaml', '.yml': 'text/yaml',
              '.md': 'text/markdown', '.log': 'text/plain', '.xlsx': 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
            };
            attachments.push({ filePath: finalPath, fileName, mimeType: mimeMap[ext] || 'application/octet-stream' });
            text = `(用户发送了文件: ${fileName})`;
          }
        }
      } catch (e) {
        console.error(`[feishu] Failed to process file: ${(e as Error).message}`);
        return null;
      }
    }

    return {
      text,
      userId: event.sender?.sender_id?.open_id || '',
      userName: event.sender?.sender_id?.open_id || '',
      conversationId: message.chat_id || '',
      platform: this.platform,
      replyContext: {
        chatId: message.chat_id,
        messageId: message.message_id,
      },
      attachments: attachments.length > 0 ? attachments : undefined,
    };
  }

  async sendTyping(_msg: PlatformMessage): Promise<void> {
    // Feishu doesn't have a typing indicator API
  }

  async sendReply(msg: PlatformMessage, text: string): Promise<void> {
    text = truncateForPlatform(text, 'feishu');
    const ctx = msg.replyContext as { chatId: string; messageId: string };
    const token = await this.getTenantAccessToken();

    const payload = JSON.stringify({
      content: this.buildCardContent(text),
      msg_type: 'interactive',
    });

    // Use reply API which works without bot being a chat member
    return new Promise((resolve, reject) => {
      const req = https.request(
        {
          hostname: 'open.feishu.cn',
          path: `/open-apis/im/v1/messages/${ctx.messageId}/reply`,
          method: 'POST',
          headers: {
            'Content-Type': 'application/json; charset=utf-8',
            Authorization: `Bearer ${token}`,
          },
        },
        (res) => {
          let data = '';
          res.on('data', (chunk) => { data += chunk; });
          res.on('end', () => {
            try {
              const result = JSON.parse(data);
              if (result.code !== 0) {
                console.error(`[feishu] Send message error code=${result.code}: ${result.msg}`);
                reject(new Error(`Feishu API error: ${result.msg}`));
              } else {
                resolve();
              }
            } catch (e) {
              reject(e);
            }
          });
        },
      );
      req.on('error', reject);
      req.write(payload);
      req.end();
    });
  }

  async updateReply(msg: PlatformMessage, text: string, messageId?: string): Promise<string> {
    const ctx = msg.replyContext as { chatId: string; messageId: string };

    if (!messageId) {
      // First call: send a new reply and return its message_id
      return this.sendFeishuReplyAndGetId(ctx.messageId, text);
    }

    // Subsequent calls: PATCH existing message
    await this.patchFeishuMessage(messageId, text);
    return messageId;
  }

  /**
   * Download an image or file from Feishu message resources API.
   * Returns local file path or null on failure.
   */
  private async downloadFeishuResource(messageId: string, fileKey: string, type: 'image' | 'file'): Promise<string | null> {
    const token = await this.getTenantAccessToken();
    const dir = path.join(UPLOAD_DIR, `feishu-${Date.now()}`);
    fs.mkdirSync(dir, { recursive: true });
    const ext = type === 'image' ? '.png' : '';
    const filePath = path.join(dir, `${fileKey}${ext}`);

    return new Promise((resolve) => {
      const req = https.request(
        {
          hostname: 'open.feishu.cn',
          path: `/open-apis/im/v1/messages/${messageId}/resources/${fileKey}?type=${type}`,
          method: 'GET',
          headers: {
            Authorization: `Bearer ${token}`,
          },
        },
        (res) => {
          if (res.statusCode !== 200) {
            console.error(`[feishu] Download resource failed: status ${res.statusCode}`);
            res.resume();
            resolve(null);
            return;
          }
          const ws = fs.createWriteStream(filePath);
          res.pipe(ws);
          ws.on('finish', () => {
            console.log(`[feishu] Downloaded ${type} to ${filePath}`);
            resolve(filePath);
          });
          ws.on('error', (err) => {
            console.error(`[feishu] Write file error: ${err.message}`);
            resolve(null);
          });
        },
      );
      req.on('error', (err) => {
        console.error(`[feishu] Download request error: ${err.message}`);
        resolve(null);
      });
      req.end();
    });
  }

  /**
   * Convert markdown tables to code blocks since Feishu card markdown
   * does not support table syntax. Preserves alignment in monospace.
   */
  /**
   * Convert Markdown tables to aligned plain-text tables for Feishu card.
   * Feishu card markdown doesn't support | table syntax, so we render
   * as a padded monospace block with clean column alignment.
   */
  private convertTablesToCodeBlocks(text: string): string {
    const lines = text.split('\n');
    const result: string[] = [];
    let tableLines: string[] = [];
    let inTable = false;

    const flushTable = () => {
      if (tableLines.length === 0) { inTable = false; return; }
      // Parse cells and compute column widths
      const rows = tableLines
        .filter(l => !/^\|[\s\-:|]+\|$/.test(l.trim())) // skip separator rows
        .map(l => l.trim().replace(/^\||\|$/g, '').split('|').map(c => c.trim()));
      if (rows.length === 0) { inTable = false; tableLines = []; return; }
      const colCount = Math.max(...rows.map(r => r.length));
      const widths = Array(colCount).fill(0);
      for (const row of rows) {
        for (let i = 0; i < colCount; i++) {
          widths[i] = Math.max(widths[i], (row[i] || '').length);
        }
      }
      result.push('```');
      for (let r = 0; r < rows.length; r++) {
        const padded = rows[r].map((cell, i) => (cell || '').padEnd(widths[i] || 0)).join('  ');
        result.push(padded);
        // Add separator after header row
        if (r === 0) result.push(widths.map(w => '-'.repeat(w)).join('  '));
      }
      result.push('```');
      tableLines = [];
      inTable = false;
    };

    for (const line of lines) {
      const trimmed = line.trim();
      const isTableRow = /^\|.+\|$/.test(trimmed);
      const isSeparator = /^\|[\s\-:|]+\|$/.test(trimmed);
      if (isTableRow || isSeparator) {
        if (!inTable) inTable = true;
        tableLines.push(line);
      } else {
        if (inTable) flushTable();
        result.push(line);
      }
    }
    if (inTable) flushTable();

    return result.join('\n');
  }

  /** Build interactive card JSON for streaming updates (PATCHable) */
  private buildCardContent(text: string): string {
    const processed = this.convertTablesToCodeBlocks(text);
    return JSON.stringify({
      config: { wide_screen_mode: true },
      elements: [{ tag: 'markdown', content: processed }],
    });
  }

  private async sendFeishuReplyAndGetId(replyToMessageId: string, text: string): Promise<string> {
    const token = await this.getTenantAccessToken();

    // Use interactive card so we can PATCH it later
    const payload = JSON.stringify({
      content: this.buildCardContent(text),
      msg_type: 'interactive',
    });

    return new Promise((resolve, reject) => {
      const req = https.request(
        {
          hostname: 'open.feishu.cn',
          path: `/open-apis/im/v1/messages/${replyToMessageId}/reply`,
          method: 'POST',
          headers: {
            'Content-Type': 'application/json; charset=utf-8',
            Authorization: `Bearer ${token}`,
          },
        },
        (res) => {
          let data = '';
          res.on('data', (chunk) => { data += chunk; });
          res.on('end', () => {
            try {
              const result = JSON.parse(data);
              if (result.code !== 0) {
                reject(new Error(`Feishu API error: ${result.msg}`));
              } else {
                resolve(result.data?.message_id || '');
              }
            } catch (e) {
              reject(e);
            }
          });
        },
      );
      req.on('error', reject);
      req.write(payload);
      req.end();
    });
  }

  private async patchFeishuMessage(messageId: string, text: string): Promise<void> {
    const token = await this.getTenantAccessToken();

    const payload = JSON.stringify({
      content: this.buildCardContent(text),
    });

    return new Promise((resolve, reject) => {
      const req = https.request(
        {
          hostname: 'open.feishu.cn',
          path: `/open-apis/im/v1/messages/${messageId}`,
          method: 'PATCH',
          headers: {
            'Content-Type': 'application/json; charset=utf-8',
            Authorization: `Bearer ${token}`,
          },
        },
        (res) => {
          let data = '';
          res.on('data', (chunk) => { data += chunk; });
          res.on('end', () => {
            try {
              const result = JSON.parse(data);
              if (result.code !== 0) {
                console.error(`[feishu] patchMessage error code=${result.code}: ${result.msg}`);
              }
              resolve();
            } catch {
              resolve(); // best-effort PATCH
            }
          });
        },
      );
      req.on('error', () => resolve()); // best-effort
      req.write(payload);
      req.end();
    });
  }

  async sendToChannel(channelId: string, text: string): Promise<void> {
    const token = await this.getTenantAccessToken();

    const payload = JSON.stringify({
      receive_id: channelId,
      content: this.buildCardContent(text),
      msg_type: 'interactive',
    });

    return new Promise((resolve, reject) => {
      const req = https.request(
        {
          hostname: 'open.feishu.cn',
          path: '/open-apis/im/v1/messages?receive_id_type=chat_id',
          method: 'POST',
          headers: {
            'Content-Type': 'application/json; charset=utf-8',
            Authorization: `Bearer ${token}`,
          },
        },
        (res) => {
          let data = '';
          res.on('data', (chunk) => { data += chunk; });
          res.on('end', () => {
            try {
              const result = JSON.parse(data);
              if (result.code !== 0) {
                console.error(`[feishu] sendToChannel error code=${result.code}: ${result.msg}`);
                reject(new Error(`Feishu API error: ${result.msg}`));
              } else {
                resolve();
              }
            } catch (e) {
              reject(e);
            }
          });
        },
      );
      req.on('error', reject);
      req.write(payload);
      req.end();
    });
  }

  private async getTenantAccessToken(): Promise<string> {
    if (this.tenantAccessToken && Date.now() < this.tokenExpiresAt) {
      return this.tenantAccessToken;
    }

    const payload = JSON.stringify({
      app_id: this.appId,
      app_secret: this.appSecret,
    });

    return new Promise((resolve, reject) => {
      const req = https.request(
        {
          hostname: 'open.feishu.cn',
          path: '/open-apis/auth/v3/tenant_access_token/internal',
          method: 'POST',
          headers: { 'Content-Type': 'application/json; charset=utf-8' },
        },
        (res) => {
          let data = '';
          res.on('data', (chunk) => { data += chunk; });
          res.on('end', () => {
            try {
              const result = JSON.parse(data);
              if (result.code !== 0) {
                reject(new Error(`Feishu token error: ${result.msg}`));
              } else {
                this.tenantAccessToken = result.tenant_access_token;
                // Expire 5 minutes early to be safe
                this.tokenExpiresAt = Date.now() + (result.expire - 300) * 1000;
                resolve(this.tenantAccessToken);
              }
            } catch (e) {
              reject(e);
            }
          });
        },
      );
      req.on('error', reject);
      req.write(payload);
      req.end();
    });
  }
}
