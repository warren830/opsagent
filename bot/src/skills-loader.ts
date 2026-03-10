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
 * Progressive skill loading (Claude Code style):
 * 1. Write each enabled skill's full instructions to knowledge/skills/<slug>.md
 * 2. Return a concise index for the system prompt (name + description only)
 * 3. Claude decides which skill to Read based on the user's query
 */
export function generateSkillsPrompt(config: SkillsConfig, knowledgeDir: string): string | null {
  const enabled = config.skills.filter(s => s.enabled);
  if (enabled.length === 0) return null;

  // Ensure knowledge/skills/ directory exists
  const skillsDir = path.join(knowledgeDir, 'skills');
  if (!fs.existsSync(skillsDir)) {
    fs.mkdirSync(skillsDir, { recursive: true });
  }

  // Clean up old skill files that are no longer enabled
  const existingFiles = fs.existsSync(skillsDir) ? fs.readdirSync(skillsDir) : [];
  const enabledSlugs = new Set(enabled.map(s => slugify(s.name)));
  for (const file of existingFiles) {
    if (file.endsWith('.md') && !enabledSlugs.has(file.replace('.md', ''))) {
      fs.unlinkSync(path.join(skillsDir, file));
    }
  }

  // Write each enabled skill to its own file
  for (const skill of enabled) {
    const slug = slugify(skill.name);
    const content = [
      `# Skill: ${skill.name}`,
      '',
      `> ${skill.description}`,
      '',
      '## Instructions',
      '',
      skill.instructions.trim(),
    ].join('\n');
    fs.writeFileSync(path.join(skillsDir, `${slug}.md`), content, 'utf-8');
  }
  console.log(`[skills-loader] Generated ${enabled.length} skill files in knowledge/skills/`);

  // Return index-only summary for system prompt
  const lines = [
    `共 ${enabled.length} 个已启用的技能。当用户的问题匹配某个技能时，用 Read 工具读取对应文件获取完整指引：`,
    '',
  ];
  for (const skill of enabled) {
    const slug = slugify(skill.name);
    lines.push(`- **${skill.name}** — ${skill.description} → \`knowledge/skills/${slug}.md\``);
  }
  return lines.join('\n');
}

function slugify(name: string): string {
  return name.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/(^-|-$)/g, '');
}
