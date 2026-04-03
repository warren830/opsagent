/**
 * GitHub source-code reader for RCA.
 *
 * Uses the GitHub REST API (Contents API) to fetch source files
 * from configured repositories using fine-grained PATs.
 *
 * Only requires Node.js built-ins — no extra dependencies.
 */
import * as https from 'https';
import * as url from 'url';

// ── Types ──────────────────────────────────────────────────────

export interface GithubSourceFile {
  path: string;
  content: string;
  size: number;
}

export interface GithubSourceContext {
  repo: string;
  files: GithubSourceFile[];
}

// ── HTTP helper ────────────────────────────────────────────────

function githubGet(
  token: string,
  apiPath: string,
  timeoutMs = 10_000,
): Promise<any> {
  return new Promise((resolve, reject) => {
    const options = {
      hostname: 'api.github.com',
      port:     443,
      path:     apiPath,
      method:   'GET',
      headers: {
        Authorization: `Bearer ${token}`,
        Accept:        'application/vnd.github+json',
        'User-Agent':  'OpsAgent-RCA/1.0',
        'X-GitHub-Api-Version': '2022-11-28',
      },
    };
    const req = https.request(options, (res) => {
      let data = '';
      res.on('data', (chunk: Buffer) => { data += chunk; });
      res.on('end', () => {
        if ((res.statusCode ?? 0) >= 400) {
          reject(new Error(`GitHub API HTTP ${res.statusCode}: ${data.substring(0, 200)}`));
          return;
        }
        try { resolve(JSON.parse(data)); }
        catch (e: any) { reject(new Error(`JSON parse error: ${e.message}`)); }
      });
    });
    req.on('error', reject);
    req.setTimeout(timeoutMs, () => req.destroy(new Error(`GitHub API timeout after ${timeoutMs}ms`)));
    req.end();
  });
}

// ── File fetcher ───────────────────────────────────────────────

/**
 * Fetch file content from a GitHub repo via Contents API.
 * Returns null if the file doesn't exist or is too large (> 200 KB).
 */
export async function fetchGithubFile(
  token: string,
  repo: string,    // "owner/repo"
  filePath: string,
  ref = 'HEAD',
): Promise<GithubSourceFile | null> {
  try {
    const apiPath = `/repos/${repo}/contents/${filePath}?ref=${ref}`;
    const data = await githubGet(token, apiPath);

    if (data.type !== 'file' || !data.content) return null;
    if (data.size > 200_000) {
      return {
        path: filePath,
        content: `[File too large to inline: ${data.size} bytes — read via GitHub UI]`,
        size: data.size,
      };
    }

    const content = Buffer.from(data.content.replace(/\n/g, ''), 'base64').toString('utf-8');

    return { path: filePath, content, size: data.size };
  } catch {
    return null;
  }
}

/**
 * List files in a directory of a GitHub repo.
 * Returns [] if directory doesn't exist.
 */
export async function listGithubDir(
  token: string,
  repo: string,
  dirPath: string,
  ref = 'HEAD',
): Promise<Array<{ name: string; type: 'file' | 'dir'; path: string }>> {
  try {
    const apiPath = `/repos/${repo}/contents/${dirPath}?ref=${ref}`;
    const data = await githubGet(token, apiPath);
    if (!Array.isArray(data)) return [];
    return data.map((f: any) => ({
      name: f.name as string,
      type: f.type === 'dir' ? 'dir' : 'file',
      path: f.path as string,
    }));
  } catch {
    return [];
  }
}

// ── High-level: collect RCA-relevant source files ─────────────

/**
 * Given a service name and repo, collect the most relevant source files
 * for RCA: main entry points, config, k8s manifests.
 *
 * Returns a formatted string ready to inject into the RCA prompt.
 */
export async function fetchSourceContext(
  token: string,
  repo: string,           // "owner/repo"
  serviceName?: string,
): Promise<GithubSourceContext> {
  const files: GithubSourceFile[] = [];

  // 1. README / architecture docs
  for (const f of ['README.md', 'ARCHITECTURE.md']) {
    const file = await fetchGithubFile(token, repo, f);
    if (file) files.push(file);
  }

  // 2. Root directory listing to understand structure
  const rootEntries = await listGithubDir(token, repo, '');
  const rootFiles   = rootEntries.filter(e => e.type === 'file');
  const rootDirs    = rootEntries.filter(e => e.type === 'dir').map(d => d.name);

  // 3. Source code directory — try common names
  const srcDirCandidates = ['app', 'src', 'lib', serviceName || ''].filter(Boolean);
  let srcDir: string | null = null;
  for (const candidate of srcDirCandidates) {
    if (rootDirs.includes(candidate)) { srcDir = candidate; break; }
  }

  if (srcDir) {
    const srcEntries = await listGithubDir(token, repo, srcDir);
    const sourceExts = ['.py', '.ts', '.js', '.go', '.java', '.rb'];
    const srcFiles   = srcEntries
      .filter(e => e.type === 'file' && sourceExts.some(ext => e.name.endsWith(ext)))
      .slice(0, 6);  // at most 6 source files

    for (const entry of srcFiles) {
      const file = await fetchGithubFile(token, repo, entry.path);
      if (file) files.push(file);
    }
  }

  // 4. K8s manifests (deployment, configmap)
  const k8sDirCandidates = ['k8s', 'kubernetes', 'deploy', 'manifests', 'helm'];
  let k8sDir: string | null = null;
  for (const candidate of k8sDirCandidates) {
    if (rootDirs.includes(candidate)) { k8sDir = candidate; break; }
  }

  if (k8sDir) {
    const k8sEntries = await listGithubDir(token, repo, k8sDir);
    for (const entry of k8sEntries.filter(e => e.type === 'file').slice(0, 4)) {
      const file = await fetchGithubFile(token, repo, entry.path);
      if (file) files.push(file);
    }
  }

  return { repo, files };
}

// ── Prompt formatter ───────────────────────────────────────────

export function formatSourceForPrompt(ctx: GithubSourceContext): string {
  if (ctx.files.length === 0) return '';

  const lines: string[] = [
    '',
    `### GitHub 源码 (${ctx.repo})`,
    '',
    `共读取 ${ctx.files.length} 个文件，用于分析代码逻辑与配置：`,
    '',
  ];

  for (const f of ctx.files) {
    const ext  = f.path.split('.').pop() ?? '';
    const lang = { py: 'python', ts: 'typescript', js: 'javascript', go: 'go', yaml: 'yaml', yml: 'yaml', md: 'markdown' }[ext] ?? ext;
    lines.push(`#### \`${f.path}\``);
    lines.push(`\`\`\`${lang}`);
    lines.push(f.content.trimEnd());
    lines.push('```');
    lines.push('');
  }

  lines.push(
    '请基于源码分析：',
    '1. 是否存在连接/资源泄漏（异常路径未 finally 释放）',
    '2. 硬编码的资源限制（连接池大小、超时、内存上限）',
    '3. K8s 资源配置是否合理（requests/limits, replicas）',
    '4. 代码逻辑是否与日志/指标/Trace 中观察到的现象一致',
    '',
  );

  return lines.join('\n');
}
