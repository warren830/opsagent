/**
 * Tool executor for OpsAgent SDK agent loop.
 * Dispatches tool calls and returns results.
 */
import { execFile } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';
import { validateCommand, SandboxConfig } from './command-sandbox';

const MAX_OUTPUT_BYTES = 100 * 1024; // 100KB max per tool result
const DEFAULT_CMD_TIMEOUT = 60_000;
const MAX_CMD_TIMEOUT = 180_000;

export interface ToolExecutorConfig {
  workDir: string;
  knowledgeDir: string;
  sandboxConfig: SandboxConfig;
  env: Record<string, string | undefined>;
  glossary?: Record<string, any>;
  skills?: Array<{ name: string; description: string; instructions: string; enabled: boolean }>;
}

export interface ToolResult {
  content: string;
  is_error: boolean;
}

/**
 * Execute a tool call and return the result.
 */
export async function executeTool(
  name: string,
  input: Record<string, any>,
  config: ToolExecutorConfig,
): Promise<ToolResult> {
  try {
    switch (name) {
      case 'run_command':
        return await executeRunCommand(input, config);
      case 'read_file':
        return executeReadFile(input, config);
      case 'search_files':
        return executeSearchFiles(input, config);
      case 'lookup_glossary':
        return executeLookupGlossary(input, config);
      case 'lookup_skill':
        return executeLookupSkill(input, config);
      default:
        return { content: `Unknown tool: ${name}`, is_error: true };
    }
  } catch (err) {
    return { content: `Tool error: ${(err as Error).message}`, is_error: true };
  }
}

// ── run_command ────────────────────────────────────────────────

async function executeRunCommand(
  input: Record<string, any>,
  config: ToolExecutorConfig,
): Promise<ToolResult> {
  const command = input.command as string;
  if (!command) return { content: 'command is required', is_error: true };

  const validation = validateCommand(command, config.sandboxConfig);
  if (!validation.allowed) {
    if (validation.needsApproval) {
      return { content: `⚠️ 此操作需要管理员批准: ${validation.reason}`, is_error: false };
    }
    return { content: `Command denied: ${validation.reason}`, is_error: true };
  }

  const timeoutMs = Math.min(
    Math.max(input.timeout_ms || DEFAULT_CMD_TIMEOUT, 1000),
    MAX_CMD_TIMEOUT,
  );

  return new Promise((resolve) => {
    const child = execFile('/bin/sh', ['-c', command], {
      cwd: config.workDir,
      env: config.env as NodeJS.ProcessEnv,
      timeout: timeoutMs,
      maxBuffer: MAX_OUTPUT_BYTES * 2,
    }, (error, stdout, stderr) => {
      let output = stdout || '';
      if (stderr) output += (output ? '\n--- stderr ---\n' : '') + stderr;

      // Truncate if too large
      if (output.length > MAX_OUTPUT_BYTES) {
        output = output.substring(0, MAX_OUTPUT_BYTES) + '\n...(output truncated at 100KB)';
      }

      if (error) {
        if (error.killed) {
          resolve({ content: `Command timed out after ${timeoutMs / 1000}s`, is_error: true });
        } else {
          resolve({ content: output || error.message, is_error: true });
        }
      } else {
        resolve({ content: output || '(no output)', is_error: false });
      }
    });
  });
}

// ── read_file ─────────────────────────────────────────────────

function executeReadFile(
  input: Record<string, any>,
  config: ToolExecutorConfig,
): ToolResult {
  const filePath = input.path as string;
  if (!filePath) return { content: 'path is required', is_error: true };

  // Security: prevent path traversal
  const resolved = path.resolve(config.workDir, filePath);
  if (!resolved.startsWith(config.workDir)) {
    return { content: 'Path traversal not allowed', is_error: true };
  }
  if (filePath.includes('..')) {
    return { content: 'Path traversal (..) not allowed', is_error: true };
  }

  if (!fs.existsSync(resolved)) {
    return { content: `File not found: ${filePath}`, is_error: true };
  }

  const stat = fs.statSync(resolved);
  if (stat.isDirectory()) {
    return { content: `${filePath} is a directory, not a file. Use search_files to list files.`, is_error: true };
  }

  const content = fs.readFileSync(resolved, 'utf-8');
  const lines = content.split('\n');
  const offset = Math.max((input.offset || 1) - 1, 0);
  const limit = Math.min(input.limit || 500, 2000);
  const selected = lines.slice(offset, offset + limit);

  const numbered = selected.map((line, i) => `${String(offset + i + 1).padStart(4)} │ ${line}`).join('\n');
  const header = `File: ${filePath} (${lines.length} lines, showing ${offset + 1}-${offset + selected.length})`;
  return { content: `${header}\n${numbered}`, is_error: false };
}

// ── search_files ──────────────────────────────────────────────

function executeSearchFiles(
  input: Record<string, any>,
  config: ToolExecutorConfig,
): ToolResult {
  const dir = input.directory
    ? path.resolve(config.workDir, input.directory as string)
    : config.knowledgeDir;

  if (!dir.startsWith(config.workDir)) {
    return { content: 'Directory must be within work directory', is_error: true };
  }

  if (!fs.existsSync(dir)) {
    return { content: `Directory not found: ${input.directory || 'knowledge'}`, is_error: true };
  }

  const pattern = input.pattern as string | undefined;
  const contentPattern = input.content_pattern as string | undefined;

  // Collect files recursively
  const files: string[] = [];
  function walk(d: string) {
    for (const entry of fs.readdirSync(d, { withFileTypes: true })) {
      if (entry.name.startsWith('.') || entry.name.startsWith('_')) continue;
      const full = path.join(d, entry.name);
      if (entry.isDirectory()) {
        walk(full);
      } else {
        const rel = path.relative(config.workDir, full);
        if (!pattern || matchGlob(entry.name, pattern)) {
          files.push(rel);
        }
      }
    }
  }
  walk(dir);

  if (files.length === 0) {
    return { content: 'No matching files found', is_error: false };
  }

  // Content search if requested
  if (contentPattern) {
    const regex = new RegExp(contentPattern, 'i');
    const matches: string[] = [];
    for (const rel of files.slice(0, 100)) {
      try {
        const content = fs.readFileSync(path.join(config.workDir, rel), 'utf-8');
        const lines = content.split('\n');
        const matchedLines = lines
          .map((line, i) => ({ line, num: i + 1 }))
          .filter(({ line }) => regex.test(line))
          .slice(0, 5);
        if (matchedLines.length > 0) {
          matches.push(`${rel}:\n${matchedLines.map(m => `  ${m.num}: ${m.line.substring(0, 120)}`).join('\n')}`);
        }
      } catch { /* skip unreadable files */ }
    }
    return {
      content: matches.length > 0
        ? `Found ${matches.length} files with matches:\n\n${matches.join('\n\n')}`
        : `No content matches for "${contentPattern}" in ${files.length} files`,
      is_error: false,
    };
  }

  return {
    content: `Found ${files.length} files:\n${files.map(f => `- ${f}`).join('\n')}`,
    is_error: false,
  };
}

function matchGlob(filename: string, pattern: string): boolean {
  const regex = new RegExp('^' + pattern.replace(/\*/g, '.*').replace(/\?/g, '.') + '$', 'i');
  return regex.test(filename);
}

// ── lookup_glossary ───────────────────────────────────────────

function executeLookupGlossary(
  input: Record<string, any>,
  config: ToolExecutorConfig,
): ToolResult {
  const term = (input.term as string || '').toLowerCase();
  if (!term) return { content: 'term is required', is_error: true };
  if (!config.glossary) return { content: 'Glossary not configured', is_error: false };

  const entries = config.glossary;
  const results: string[] = [];

  // 1. Exact key match
  if (entries[term]) {
    results.push(formatGlossaryEntry(term, entries[term]));
  }

  // 2. Alias match
  for (const [key, entry] of Object.entries(entries)) {
    if (key === term) continue;
    const aliases = (entry as any).aliases || [];
    if (aliases.some((a: string) => a.toLowerCase() === term)) {
      results.push(formatGlossaryEntry(key, entry));
    }
  }

  // 3. Fuzzy match (substring in key, full_name, or description)
  if (results.length === 0) {
    for (const [key, entry] of Object.entries(entries)) {
      const e = entry as any;
      if (key.includes(term) || (e.full_name || '').toLowerCase().includes(term) ||
          (e.description || '').toLowerCase().includes(term)) {
        results.push(formatGlossaryEntry(key, entry));
      }
    }
  }

  if (results.length === 0) {
    const allKeys = Object.keys(entries).join(', ');
    return { content: `No glossary entry found for "${term}". Available terms: ${allKeys}`, is_error: false };
  }

  return { content: results.join('\n\n'), is_error: false };
}

function formatGlossaryEntry(key: string, entry: any): string {
  const parts = [`**${key}**: ${entry.full_name || ''}`];
  if (entry.description) parts.push(`Description: ${entry.description}`);
  if (entry.aliases?.length) parts.push(`Aliases: ${entry.aliases.join(', ')}`);
  if (entry.services?.length) parts.push(`Services: ${entry.services.join(', ')}`);
  if (entry.aws_accounts?.length) parts.push(`AWS Accounts: ${entry.aws_accounts.join(', ')}`);
  return parts.join('\n');
}

// ── lookup_skill ──────────────────────────────────────────────

function executeLookupSkill(
  input: Record<string, any>,
  config: ToolExecutorConfig,
): ToolResult {
  const name = (input.skill_name as string || '').toLowerCase();
  if (!name) return { content: 'skill_name is required', is_error: true };
  if (!config.skills?.length) return { content: 'No skills configured', is_error: false };

  const enabled = config.skills.filter(s => s.enabled);

  // Exact match
  const exact = enabled.find(s => s.name.toLowerCase() === name);
  if (exact) {
    return { content: `# ${exact.name}\n\n${exact.description}\n\n${exact.instructions}`, is_error: false };
  }

  // Fuzzy match
  const fuzzy = enabled.find(s =>
    s.name.toLowerCase().includes(name) || s.description.toLowerCase().includes(name),
  );
  if (fuzzy) {
    return { content: `# ${fuzzy.name}\n\n${fuzzy.description}\n\n${fuzzy.instructions}`, is_error: false };
  }

  const available = enabled.map(s => `- ${s.name}: ${s.description}`).join('\n');
  return { content: `Skill "${name}" not found.\n\nAvailable skills:\n${available}`, is_error: false };
}
