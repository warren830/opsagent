import * as http from 'http';
import * as crypto from 'crypto';
import * as https from 'https';
import { PlatformAdapter, PlatformMessage, truncateForPlatform } from './types';

export interface SlackAdapterOptions {
  botToken: string;       // xoxb-...
  signingSecret: string;  // Used to verify requests from Slack
  allowedChannels?: string[];  // If set, only handle messages from these channels
}

export class SlackAdapter implements PlatformAdapter {
  readonly platform = 'slack';
  private readonly botToken: string;
  private readonly signingSecret: string;
  private readonly allowedChannels?: string[];

  constructor(options: SlackAdapterOptions) {
    this.botToken = options.botToken;
    this.signingSecret = options.signingSecret;
    this.allowedChannels = options.allowedChannels;
  }

  async handleWebhook(
    req: http.IncomingMessage,
    res: http.ServerResponse,
    body: any,
  ): Promise<PlatformMessage | null> {
    // Slack URL verification challenge (skip signature check for initial setup)
    if (body.type === 'url_verification') {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ challenge: body.challenge }));
      return null;
    }

    // Verify Slack request signature
    const timestamp = req.headers['x-slack-request-timestamp'] as string;
    const signature = req.headers['x-slack-signature'] as string;
    const rawBody = (req as any)._rawBody;
    if (timestamp && signature && rawBody && this.signingSecret) {
      if (!this.verifySignature(timestamp, rawBody, signature)) {
        console.warn('[slack] Signature verification failed');
        res.writeHead(401);
        res.end('Invalid signature');
        return null;
      }
    }

    // Only handle event_callback with message events
    if (body.type !== 'event_callback' || !body.event) {
      res.writeHead(200);
      res.end();
      return null;
    }

    const event = body.event;

    // Ignore bot messages, message edits, and non-message events
    if (
      event.type !== 'message' ||
      event.subtype ||
      event.bot_id ||
      !event.text
    ) {
      res.writeHead(200);
      res.end();
      return null;
    }

    // Filter by allowed channels if configured
    if (this.allowedChannels && this.allowedChannels.length > 0) {
      if (!this.allowedChannels.includes(event.channel)) {
        res.writeHead(200);
        res.end();
        return null;
      }
    }

    // Acknowledge immediately (Slack requires response within 3 seconds)
    res.writeHead(200);
    res.end();

    return {
      text: event.text.trim(),
      userId: event.user || '',
      userName: event.user || '',
      conversationId: event.channel || '',
      platform: this.platform,
      replyContext: { channel: event.channel, threadTs: event.ts },
    };
  }

  async sendTyping(_msg: PlatformMessage): Promise<void> {
    // Slack doesn't have an explicit typing API for bots
  }

  async sendReply(msg: PlatformMessage, text: string): Promise<void> {
    const ctx = msg.replyContext as { channel: string; threadTs: string };
    await this.postMessage(ctx.channel, truncateForPlatform(text, 'slack'), ctx.threadTs);
  }

  /**
   * Verify Slack request signature.
   * Call this in middleware before parsing body if needed.
   */
  verifySignature(timestamp: string, rawBody: string, signature: string): boolean {
    const fiveMinutes = 5 * 60;
    if (Math.abs(Date.now() / 1000 - parseInt(timestamp, 10)) > fiveMinutes) {
      return false;
    }
    const sigBasestring = `v0:${timestamp}:${rawBody}`;
    const mySignature = 'v0=' + crypto
      .createHmac('sha256', this.signingSecret)
      .update(sigBasestring)
      .digest('hex');
    return crypto.timingSafeEqual(Buffer.from(mySignature), Buffer.from(signature));
  }

  async updateReply(msg: PlatformMessage, text: string, messageId?: string): Promise<string> {
    const ctx = msg.replyContext as { channel: string; threadTs: string };
    text = truncateForPlatform(text, 'slack');

    if (!messageId) {
      // First call: post new message in thread, return its ts
      const ts = await this.postMessageAndGetTs(ctx.channel, text, ctx.threadTs);
      return ts;
    }

    // Subsequent calls: update existing message
    await this.chatUpdate(ctx.channel, messageId, text);
    return messageId;
  }

  async sendToChannel(channelId: string, text: string): Promise<void> {
    await this.postMessage(channelId, text);
  }

  private postMessageAndGetTs(channel: string, text: string, threadTs?: string): Promise<string> {
    return new Promise((resolve, reject) => {
      const payload = JSON.stringify({ channel, text, thread_ts: threadTs });
      const req = https.request({
        hostname: 'slack.com', path: '/api/chat.postMessage', method: 'POST',
        headers: { 'Content-Type': 'application/json; charset=utf-8', Authorization: `Bearer ${this.botToken}` },
      }, (res) => {
        let data = '';
        res.on('data', (chunk) => { data += chunk; });
        res.on('end', () => {
          try {
            const result = JSON.parse(data);
            if (!result.ok) reject(new Error(`Slack API: ${result.error}`));
            else resolve(result.ts || '');
          } catch { reject(new Error('Slack API parse error')); }
        });
      });
      req.on('error', reject);
      req.write(payload);
      req.end();
    });
  }

  private chatUpdate(channel: string, ts: string, text: string): Promise<void> {
    return new Promise((resolve, reject) => {
      const payload = JSON.stringify({ channel, ts, text });
      const req = https.request({
        hostname: 'slack.com', path: '/api/chat.update', method: 'POST',
        headers: { 'Content-Type': 'application/json; charset=utf-8', Authorization: `Bearer ${this.botToken}` },
      }, (res) => {
        let data = '';
        res.on('data', (chunk) => { data += chunk; });
        res.on('end', () => {
          try {
            const result = JSON.parse(data);
            if (!result.ok) console.error(`[slack] chat.update error: ${result.error}`);
            resolve();
          } catch { resolve(); }
        });
      });
      req.on('error', () => resolve());
      req.write(payload);
      req.end();
    });
  }

  private postMessage(channel: string, text: string, threadTs?: string): Promise<void> {
    const payload = JSON.stringify({
      channel,
      text,
      thread_ts: threadTs,
    });

    return new Promise((resolve, reject) => {
      const req = https.request(
        {
          hostname: 'slack.com',
          path: '/api/chat.postMessage',
          method: 'POST',
          headers: {
            'Content-Type': 'application/json; charset=utf-8',
            Authorization: `Bearer ${this.botToken}`,
          },
        },
        (res) => {
          let data = '';
          res.on('data', (chunk) => { data += chunk; });
          res.on('end', () => {
            const result = JSON.parse(data);
            if (!result.ok) {
              console.error(`[slack] postMessage error: ${result.error}`);
              reject(new Error(`Slack API error: ${result.error}`));
            } else {
              resolve();
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
