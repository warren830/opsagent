import * as fs from 'fs';
import * as path from 'path';
import * as yaml from 'js-yaml';

import { loadSkills } from './skills-loader';
import type { Skill, SkillsConfig } from './skills-loader';

export interface UserSkillsConfig {
  overrides?: Array<{ name: string; enabled: boolean }>;
  custom_skills?: Array<{ name: string; description: string; instructions: string }>;
}

/**
 * Load a user (or tenant) skills config from a YAML file.
 * Returns null if file doesn't exist or content is invalid.
 */
export function loadUserSkillsConfig(configPath: string): UserSkillsConfig | null {
  const absolutePath = path.resolve(configPath);
  if (!fs.existsSync(absolutePath)) return null;

  try {
    const content = fs.readFileSync(absolutePath, 'utf-8');
    const parsed = yaml.load(content) as Record<string, unknown>;

    if (!parsed || typeof parsed !== 'object') return null;

    const hasOverrides = Array.isArray(parsed.overrides);
    const hasCustomSkills = Array.isArray(parsed.custom_skills);

    if (!hasOverrides && !hasCustomSkills) return null;

    return {
      overrides: hasOverrides ? (parsed.overrides as UserSkillsConfig['overrides']) : undefined,
      custom_skills: hasCustomSkills ? (parsed.custom_skills as UserSkillsConfig['custom_skills']) : undefined,
    };
  } catch (err) {
    console.warn(`[personal-skills] Failed to load config: ${(err as Error).message}`);
    return null;
  }
}

/**
 * Apply overrides (enable/disable by name) to a skills array.
 * Returns a new array — does not mutate the input.
 */
function applyOverrides(
  skills: Skill[],
  overrides: Array<{ name: string; enabled: boolean }>,
): Skill[] {
  const overrideMap = new Map(overrides.map(o => [o.name, o.enabled]));
  return skills.map(skill => {
    if (overrideMap.has(skill.name)) {
      return { ...skill, enabled: overrideMap.get(skill.name)! };
    }
    return skill;
  });
}

/**
 * Three-layer skill merging:
 *   1. Global skills (from loadSkills)
 *   2. Tenant overrides (enable/disable by name)
 *   3. User overrides (enable/disable by name) + user custom_skills (appended, always enabled)
 *
 * If any layer's config is missing or invalid, that layer is skipped.
 */
export function loadMergedSkills(
  globalConfigPath: string,
  knowledgeDir: string,
  tenantId?: string,
  username?: string,
): SkillsConfig {
  // Layer 1: Global
  const globalConfig = loadSkills(globalConfigPath);
  let skills: Skill[] = globalConfig?.skills ?? [];

  // Layer 2: Tenant overrides
  if (tenantId) {
    const tenantConfigPath = path.join(
      knowledgeDir, '_tenants', tenantId, '_config', 'skills.yaml',
    );
    const tenantConfig = loadUserSkillsConfig(tenantConfigPath);
    if (tenantConfig?.overrides) {
      skills = applyOverrides(skills, tenantConfig.overrides);
    }
  }

  // Layer 3: User overrides + custom skills
  if (username) {
    const userConfigPath = path.join(
      knowledgeDir, '_users', username, '_config', 'skills.yaml',
    );
    const userConfig = loadUserSkillsConfig(userConfigPath);
    if (userConfig) {
      // Apply overrides
      if (userConfig.overrides) {
        skills = applyOverrides(skills, userConfig.overrides);
      }
      // Append custom skills (always enabled)
      if (userConfig.custom_skills) {
        for (const custom of userConfig.custom_skills) {
          skills.push({
            name: custom.name,
            description: custom.description,
            enabled: true,
            instructions: custom.instructions,
          });
        }
      }
    }
  }

  return { skills };
}
