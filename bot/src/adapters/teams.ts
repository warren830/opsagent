import * as http from 'http';
import {
  BotFrameworkAdapter,
  BotFrameworkAdapterSettings,
  TurnContext,
  ConversationState,
  MemoryStorage,
} from 'botbuilder';
import { PlatformAdapter, PlatformMessage } from './types';

export interface TeamsAdapterOptions {
  appId: string;
  appPassword: string;
}

export class TeamsAdapter implements PlatformAdapter {
  readonly platform = 'teams';
  private readonly adapter: BotFrameworkAdapter;
  private readonly conversationState: ConversationState;

  constructor(options: TeamsAdapterOptions) {
    const settings: Partial<BotFrameworkAdapterSettings> = {
      appId: options.appId,
      appPassword: options.appPassword,
    };

    this.adapter = new BotFrameworkAdapter(settings);
    this.conversationState = new ConversationState(new MemoryStorage());

    this.adapter.onTurnError = async (context: TurnContext, error: Error) => {
      console.error(`[teams] Unhandled error: ${error.message}`);
      try {
        await context.sendActivity('An internal error occurred. Please try again later.');
      } catch (_) { /* ignore */ }
      try {
        await this.conversationState.clear(context);
        await this.conversationState.saveChanges(context);
      } catch (_) { /* ignore */ }
    };
  }

  async handleWebhook(
    req: http.IncomingMessage,
    res: http.ServerResponse,
    body: any,
  ): Promise<PlatformMessage | null> {
    (req as any).body = body;

    // Wrap raw http.ServerResponse to satisfy BotFramework WebResponse interface
    const webRes = Object.assign(res, {
      status: (code: number) => { res.statusCode = code; return webRes; },
      send: (body?: any) => { res.end(typeof body === 'string' ? body : JSON.stringify(body)); return webRes; },
    });

    return new Promise<PlatformMessage | null>((resolve, reject) => {
      this.adapter.processActivity(req, webRes as any, async (context: TurnContext) => {
        await this.conversationState.saveChanges(context);
        if (context.activity.type !== 'message' || !context.activity.text?.trim()) {
          resolve(null);
          return;
        }
        resolve({
          text: context.activity.text.trim(),
          userId: context.activity.from?.id || '',
          userName: context.activity.from?.name || '',
          conversationId: context.activity.conversation?.id || '',
          platform: this.platform,
          replyContext: context,
        });
      }).catch(reject);
    });
  }

  async sendTyping(msg: PlatformMessage): Promise<void> {
    const context = msg.replyContext as TurnContext;
    await context.sendActivity({ type: 'typing' });
  }

  async sendReply(msg: PlatformMessage, text: string): Promise<void> {
    const context = msg.replyContext as TurnContext;
    if (text.length < 500 && !text.includes('|')) {
      await context.sendActivity(text);
    } else {
      // For longer responses, use Adaptive Card
      const { CardFactory } = await import('botbuilder');
      const card = CardFactory.adaptiveCard({
        $schema: 'http://adaptivecards.io/schemas/adaptive-card.json',
        type: 'AdaptiveCard',
        version: '1.4',
        body: [{ type: 'TextBlock', text, wrap: true }],
      });
      await context.sendActivity({ attachments: [card] });
    }
  }
}
