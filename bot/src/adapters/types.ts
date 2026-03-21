import * as http from 'http';

/**
 * An attachment (image or file) uploaded by the user alongside a message.
 */
export interface MessageAttachment {
  /** Local file path where the attachment is saved */
  filePath: string;
  /** Original filename */
  fileName: string;
  /** MIME type (e.g. image/png, application/pdf) */
  mimeType: string;
}

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
  /** Files/images attached to this message */
  attachments?: MessageAttachment[];
  /** Tenant ID (set by tenant resolver) */
  tenantId?: string;
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

  /**
   * Update an existing reply message (for streaming).
   * First call (no messageId): sends a new reply, returns its message ID.
   * Subsequent calls: updates the existing message in-place.
   */
  updateReply?(msg: PlatformMessage, text: string, messageId?: string): Promise<string>;

  /** Send a proactive message to a channel (for scheduled jobs) */
  sendToChannel?(channelId: string, text: string): Promise<void>;
}
