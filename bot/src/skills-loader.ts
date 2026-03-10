import * as fs from 'fs';
import * as path from 'path';
import * as yaml from 'js-yaml';

export interface Skill {
  name: string;
  description: string;
  enabled: boolean;
  instructions: string;
}

export interface SkillsConfig {
  skills: Skill[];
}

export function loadSkills(configPath: string): SkillsConfig | null {
  const absolutePath = path.resolve(configPath);
  if (!fs.existsSync(absolutePath)) return null;

  try {
    const content = fs.readFileSync(absolutePath, 'utf-8');
    const config = yaml.load(content) as SkillsConfig;
    if (!config || !Array.isArray(config.skills)) return null;
    return config;
  } catch (err) {
    console.warn(`[skills-loader] Failed to load skills config: ${(err as Error).message}`);
    return null;
  }
}

/**
 * Generate skill instructions for system prompt (only enabled skills).
 */
export function generateSkillsPrompt(config: SkillsConfig): string | null {
  const enabled = config.skills.filter(s => s.enabled);
  if (enabled.length === 0) return null;

  const lines = enabled.map(s =>
    `### ${s.name}\n${s.instructions.trim()}`
  );
  return lines.join('\n\n');
}
