import * as http from 'http';
import * as crypto from 'crypto';
import * as https from 'https';
import { PlatformAdapter, PlatformMessage } from './types';

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
    if (!message || message.message_type !== 'text') {
      res.writeHead(200);
      res.end();
      return null;
    }

    // Acknowledge immediately
    res.writeHead(200);
    res.end();

    // Parse text content (Feishu wraps text in JSON: {"text":"actual message"})
    let text = '';
    try {
      const content = JSON.parse(message.content);
      text = content.text || '';
    } catch {
      text = message.content || '';
    }

    // Strip @mentions (format: @_user_xxx)
    text = text.replace(/@_user_\w+/g, '').trim();

    if (!text) return null;

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
    };
  }

  async sendTyping(_msg: PlatformMessage): Promise<void> {
    // Feishu doesn't have a typing indicator API
  }

  async sendReply(msg: PlatformMessage, text: string): Promise<void> {
    const ctx = msg.replyContext as { chatId: string; messageId: string };
    const token = await this.getTenantAccessToken();

    const payload = JSON.stringify({
      content: JSON.stringify({ text }),
      msg_type: 'text',
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
