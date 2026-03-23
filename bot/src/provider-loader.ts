import * as fs from 'fs';
import * as path from 'path';
import * as yaml from 'js-yaml';
import Anthropic from '@anthropic-ai/sdk';
import AnthropicBedrock from '@anthropic-ai/bedrock-sdk';

export type ProviderType = 'bedrock' | 'anthropic' | 'gateway' | 'bedrock-gateway' | 'vertex' | 'foundry';

export interface ProviderConfig {
  type: ProviderType;
  model?: string;
  max_turns?: number;
  timeout_ms?: number;
  // Anthropic API
  api_key?: string;
  base_url?: string;
  // Gateway
  auth_token?: string;
  // Vertex
  project_id?: string;
  region?: string;
  // Foundry
  resource?: string;
}

interface ProviderFile {
  provider: ProviderConfig;
}

const DEFAULT_PROVIDER: ProviderConfig = {
  type: 'bedrock',
  model: 'opus',
};

export function loadProvider(configPath: string): ProviderConfig {
  const absolutePath = path.resolve(configPath);
  if (!fs.existsSync(absolutePath)) return DEFAULT_PROVIDER;

  try {
    const content = fs.readFileSync(absolutePath, 'utf-8');
    const config = yaml.load(content) as ProviderFile;
    if (!config?.provider?.type) return DEFAULT_PROVIDER;
    return { ...DEFAULT_PROVIDER, ...config.provider };
  } catch (err) {
    console.warn(`[provider-loader] Failed to load provider config: ${(err as Error).message}`);
    return DEFAULT_PROVIDER;
  }
}

/**
 * Build environment variables for Claude Code CLI based on provider config.
 */
export function buildProviderEnv(provider: ProviderConfig): Record<string, string> {
  const env: Record<string, string> = {};

  switch (provider.type) {
    case 'bedrock':
      env.CLAUDE_CODE_USE_BEDROCK = '1';
      break;

    case 'anthropic':
      if (provider.api_key) env.ANTHROPIC_API_KEY = provider.api_key;
      if (provider.base_url) env.ANTHROPIC_BASE_URL = provider.base_url;
      break;

    case 'gateway':
      if (provider.base_url) env.ANTHROPIC_BASE_URL = provider.base_url;
      if (provider.auth_token) env.ANTHROPIC_AUTH_TOKEN = provider.auth_token;
      if (provider.api_key) env.ANTHROPIC_API_KEY = provider.api_key;
      break;

    case 'bedrock-gateway':
      env.CLAUDE_CODE_USE_BEDROCK = '1';
      env.CLAUDE_CODE_SKIP_BEDROCK_AUTH = '1';
      if (provider.base_url) env.ANTHROPIC_BEDROCK_BASE_URL = provider.base_url;
      if (provider.auth_token) env.ANTHROPIC_AUTH_TOKEN = provider.auth_token;
      break;

    case 'vertex':
      env.CLAUDE_CODE_USE_VERTEX = '1';
      if (provider.project_id) env.ANTHROPIC_VERTEX_PROJECT_ID = provider.project_id;
      if (provider.region) env.CLOUD_ML_REGION = provider.region;
      break;

    case 'foundry':
      env.CLAUDE_CODE_USE_FOUNDRY = '1';
      if (provider.resource) env.ANTHROPIC_FOUNDRY_RESOURCE = provider.resource;
      if (provider.api_key) env.ANTHROPIC_FOUNDRY_API_KEY = provider.api_key;
      if (provider.base_url) env.ANTHROPIC_FOUNDRY_BASE_URL = provider.base_url;
      break;
  }

  return env;
}

// ── SDK client builder ──────────────────────────────────────────

/** Unified client type wrapping both Anthropic and AnthropicBedrock */
export type SdkClient = Anthropic | AnthropicBedrock;

/**
 * Build an Anthropic SDK client based on provider config.
 * Returns either Anthropic (for API/gateway/vertex/foundry) or AnthropicBedrock.
 */
export function buildSdkClient(provider: ProviderConfig): SdkClient {
  switch (provider.type) {
    case 'bedrock':
      return new AnthropicBedrock();

    case 'bedrock-gateway':
      return new AnthropicBedrock({
        baseURL: provider.base_url,
      });

    case 'anthropic':
      return new Anthropic({
        apiKey: provider.api_key,
        baseURL: provider.base_url,
      });

    case 'gateway':
      return new Anthropic({
        apiKey: provider.api_key || 'not-needed',
        baseURL: provider.base_url,
        defaultHeaders: provider.auth_token
          ? { Authorization: `Bearer ${provider.auth_token}` }
          : undefined,
      });

    case 'vertex':
      // Vertex uses the standard Anthropic client with Google auth
      return new Anthropic({
        baseURL: provider.base_url,
        apiKey: provider.api_key,
      });

    case 'foundry':
      return new Anthropic({
        apiKey: provider.api_key,
        baseURL: provider.base_url,
      });

    default:
      console.warn(`[provider-loader] Unknown provider type: ${provider.type}, falling back to Bedrock`);
      return new AnthropicBedrock();
  }
}

/** Model shorthand to full model ID mapping */
const MODEL_MAP: Record<string, Record<string, string>> = {
  bedrock: {
    opus: 'us.anthropic.claude-opus-4-20250514-v1:0',
    sonnet: 'us.anthropic.claude-sonnet-4-20250514-v1:0',
    haiku: 'us.anthropic.claude-haiku-4-5-20251001-v1:0',
  },
  'bedrock-gateway': {
    opus: 'us.anthropic.claude-opus-4-20250514-v1:0',
    sonnet: 'us.anthropic.claude-sonnet-4-20250514-v1:0',
    haiku: 'us.anthropic.claude-haiku-4-5-20251001-v1:0',
  },
  anthropic: {
    opus: 'claude-opus-4-0-20250514',
    sonnet: 'claude-sonnet-4-0-20250514',
    haiku: 'claude-haiku-4-5-20251001',
  },
  gateway: {
    opus: 'claude-opus-4-0-20250514',
    sonnet: 'claude-sonnet-4-0-20250514',
    haiku: 'claude-haiku-4-5-20251001',
  },
};

/**
 * Resolve a model shorthand (e.g. "opus") to a full model ID.
 */
export function resolveModelId(provider: ProviderConfig): string {
  const shorthand = provider.model || 'sonnet';
  const map = MODEL_MAP[provider.type] || MODEL_MAP.anthropic;
  return map[shorthand] || shorthand; // passthrough if already a full ID
}
