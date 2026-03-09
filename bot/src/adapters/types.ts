import * as http from 'http';

/**
 * Platform-agnostic message context.
 * Each adapter converts platform-specific payloads into this format.
 */
export interface PlatformMessage {
  text: string;
  userId: string;
  userName: string;
  conversationId: string;
  platform: string;
  /** Opaque context each adapter needs to send replies */
  replyContext: unknown;
}

/**
 * Common interface for all chat platform adapters.
 */
export interface PlatformAdapter {
  readonly platform: string;

  /**
   * Handle an incoming HTTP webhook request.
   * Returns null if the request is not a user message (e.g. challenge, health, etc.).
   */
  handleWebhook(
    req: http.IncomingMessage,
    res: http.ServerResponse,
    body: any,
  ): Promise<PlatformMessage | null>;

  /** Send a typing / processing indicator */
  sendTyping(msg: PlatformMessage): Promise<void>;

  /** Send a text reply back to the user */
  sendReply(msg: PlatformMessage, text: string): Promise<void>;
}
