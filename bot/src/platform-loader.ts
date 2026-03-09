import * as fs from 'fs';
import * as path from 'path';
import * as yaml from 'js-yaml';

export interface PlatformSettings {
  enabled: boolean;
  settings: Record<string, any>;
}

export interface PlatformsConfig {
  platforms: Record<string, PlatformSettings>;
}

/**
 * Load platform configuration from a YAML file.
 * Returns the full config or a default (all disabled) if the file doesn't exist.
 */
export function loadPlatforms(configPath: string): PlatformsConfig {
  const absolutePath = path.resolve(configPath);

  if (!fs.existsSync(absolutePath)) {
    console.warn(`[platform-loader] Config file not found: ${absolutePath}, using defaults`);
    return {
      platforms: {
        teams: { enabled: true, settings: {} },
        slack: { enabled: false, settings: {} },
        feishu: { enabled: false, settings: {} },
      },
    };
  }

  try {
    const content = fs.readFileSync(absolutePath, 'utf-8');
    const config = yaml.load(content) as PlatformsConfig;
    if (!config || !config.platforms) {
      console.warn('[platform-loader] No platforms found in config, using defaults');
      return { platforms: {} };
    }
    const enabled = Object.entries(config.platforms)
      .filter(([, p]) => p.enabled)
      .map(([name]) => name);
    console.log(`[platform-loader] Enabled platforms: ${enabled.join(', ') || 'none'}`);
    return config;
  } catch (err) {
    console.warn(`[platform-loader] Failed to load platforms config: ${(err as Error).message}`);
    return { platforms: {} };
  }
}

/**
 * Check if a specific platform is enabled.
 */
export function isPlatformEnabled(config: PlatformsConfig, platform: string): boolean {
  return config.platforms[platform]?.enabled === true;
}

/**
 * Get settings for a specific platform.
 */
export function getPlatformSettings(config: PlatformsConfig, platform: string): Record<string, any> {
  return config.platforms[platform]?.settings || {};
}
