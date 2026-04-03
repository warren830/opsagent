import * as fs from 'fs';
import * as yaml from 'js-yaml';

export interface GithubRepo {
  id: string;
  name: string;
  repo: string;       // "org/repo"
  token: string;      // fine-grained PAT
  enabled: boolean;
  description?: string;
}

export interface GithubReposConfig {
  github_repos: GithubRepo[];
}

export function loadGithubRepos(configPath: string): GithubReposConfig {
  try {
    if (!fs.existsSync(configPath)) {
      return { github_repos: [] };
    }
    const content = fs.readFileSync(configPath, 'utf-8');
    const data = yaml.load(content) as any;
    return {
      github_repos: Array.isArray(data?.github_repos) ? data.github_repos : [],
    };
  } catch {
    return { github_repos: [] };
  }
}

export function saveGithubRepos(configPath: string, config: GithubReposConfig): void {
  const content = yaml.dump(config, { lineWidth: -1 });
  fs.writeFileSync(configPath, content, 'utf-8');
}
