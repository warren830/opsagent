import * as fs from 'fs';
import * as path from 'path';
import * as yaml from 'js-yaml';

export interface GlossaryEntry {
  full_name: string;
  description: string;
  aliases?: string[];
  aws_accounts?: string[];
  services?: string[];
}

export interface GlossaryConfig {
  glossary: Record<string, GlossaryEntry>;
}

const MAX_PROMPT_ENTRIES = 50;

/**
 * Load glossary from a YAML config file.
 * Returns null if the file doesn't exist or is empty.
 */
export function loadGlossary(configPath: string): GlossaryConfig | null {
  const absolutePath = path.resolve(configPath);

  if (!fs.existsSync(absolutePath)) {
    return null;
  }

  try {
    const content = fs.readFileSync(absolutePath, 'utf-8');
    const config = yaml.load(content) as GlossaryConfig;
    if (!config || !config.glossary || Object.keys(config.glossary).length === 0) {
      return null;
    }
    return config;
  } catch (err) {
    console.warn(`[glossary-loader] Failed to load glossary: ${(err as Error).message}`);
    return null;
  }
}

/**
 * Generate glossary knowledge file and return a summary for the system prompt.
 * Writes full details to knowledge/glossary.md and returns a concise summary.
 */
export function generateGlossaryKnowledge(config: GlossaryConfig, knowledgeDir: string): string | null {
  const entries = Object.entries(config.glossary);
  if (entries.length === 0) return null;

  // Generate full markdown knowledge file
  const mdLines: string[] = [
    '# Company Glossary',
    '',
    '| Term | Full Name | Description | Aliases | AWS Accounts | Services |',
    '|------|-----------|-------------|---------|--------------|----------|',
  ];

  for (const [key, entry] of entries) {
    const aliases = entry.aliases?.join(', ') || '-';
    const accounts = entry.aws_accounts?.join(', ') || '-';
    const services = entry.services?.join(', ') || '-';
    mdLines.push(`| ${key} | ${entry.full_name} | ${entry.description} | ${aliases} | ${accounts} | ${services} |`);
  }

  if (!fs.existsSync(knowledgeDir)) {
    fs.mkdirSync(knowledgeDir, { recursive: true });
  }
  fs.writeFileSync(path.join(knowledgeDir, 'glossary.md'), mdLines.join('\n'), 'utf-8');
  console.log(`[glossary-loader] Generated knowledge/glossary.md with ${entries.length} entries`);

  // Generate concise summary for system prompt (limit to MAX_PROMPT_ENTRIES)
  const summaryEntries = entries.slice(0, MAX_PROMPT_ENTRIES);
  const summaryLines = summaryEntries.map(([key, entry]) => {
    let line = `- **${key}**: ${entry.full_name}`;
    if (entry.aws_accounts?.length) {
      line += ` (accounts: ${entry.aws_accounts.join(', ')})`;
    }
    return line;
  });

  if (entries.length > MAX_PROMPT_ENTRIES) {
    summaryLines.push(`- ... and ${entries.length - MAX_PROMPT_ENTRIES} more terms (see glossary.md)`);
  }

  return summaryLines.join('\n');
}
