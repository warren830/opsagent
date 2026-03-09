import * as fs from 'fs';
import * as path from 'path';
import * as yaml from 'js-yaml';

export interface PluginConfig {
  enabled: boolean;
  type: string;
  command: string;
  args: string[];
  env?: Record<string, string>;
}

interface PluginsFile {
  plugins: Record<string, PluginConfig>;
}

interface McpServerEntry {
  command: string;
  args: string[];
  env?: Record<string, string>;
}

interface McpConfig {
  mcpServers: Record<string, McpServerEntry>;
}

/**
 * Substitute ${VAR_NAME} patterns with actual environment variable values.
 */
function substituteEnvVars(value: string): string {
  return value.replace(/\$\{([^}]+)\}/g, (_match, varName: string) => {
    const envValue = process.env[varName];
    if (envValue === undefined) {
      console.warn(`[plugin-loader] Environment variable ${varName} is not set`);
      return '';
    }
    return envValue;
  });
}

/**
 * Load plugins from a YAML config file. Returns only enabled plugins.
 */
export function loadPlugins(configPath: string): Record<string, PluginConfig> {
  const absolutePath = path.resolve(configPath);

  if (!fs.existsSync(absolutePath)) {
    console.warn(`[plugin-loader] Config file not found: ${absolutePath}`);
    return {};
  }

  const content = fs.readFileSync(absolutePath, 'utf-8');
  const config = yaml.load(content) as PluginsFile;

  if (!config || !config.plugins) {
    console.warn('[plugin-loader] No plugins found in config');
    return {};
  }

  const enabledPlugins: Record<string, PluginConfig> = {};
  for (const [name, plugin] of Object.entries(config.plugins)) {
    if (plugin.enabled) {
      enabledPlugins[name] = plugin;
    }
  }

  console.log(`[plugin-loader] Loaded ${Object.keys(enabledPlugins).length} enabled plugin(s): ${Object.keys(enabledPlugins).join(', ')}`);
  return enabledPlugins;
}

/**
 * Generate MCP configuration from enabled plugins and write to disk.
 */
export function generateMcpConfig(plugins: Record<string, PluginConfig>, outputPath: string): void {
  const mcpConfig: McpConfig = { mcpServers: {} };

  for (const [name, plugin] of Object.entries(plugins)) {
    const entry: McpServerEntry = {
      command: plugin.command,
      args: plugin.args,
    };

    if (plugin.env) {
      entry.env = {};
      for (const [key, value] of Object.entries(plugin.env)) {
        entry.env[key] = substituteEnvVars(value);
      }
    }

    mcpConfig.mcpServers[name] = entry;
  }

  const outputDir = path.dirname(outputPath);
  if (!fs.existsSync(outputDir)) {
    fs.mkdirSync(outputDir, { recursive: true });
  }

  fs.writeFileSync(outputPath, JSON.stringify(mcpConfig, null, 2), 'utf-8');
  console.log(`[plugin-loader] MCP config written to ${outputPath}`);
}
