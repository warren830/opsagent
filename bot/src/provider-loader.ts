import * as fs from 'fs';
import * as path from 'path';
import * as yaml from 'js-yaml';

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
