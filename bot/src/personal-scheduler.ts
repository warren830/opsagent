import * as fs from 'fs';
import * as path from 'path';
import * as yaml from 'js-yaml';
import * as cron from 'node-cron';
import { sanitizeUsername } from './user-config-loader';

let lastTimestamp = 0;

/** Returns a monotonically increasing timestamp (ms) to avoid filename collisions. */
function uniqueTimestamp(): number {
  const now = Date.now();
  lastTimestamp = now > lastTimestamp ? now : lastTimestamp + 1;
  return lastTimestamp;
}

export interface UserScheduledJob {
  name: string;
  enabled: boolean;
  cron: string;
  timezone?: string;
  query: string;
}

/**
 * Slugify a job name for use in file paths.
 * Lowercase, replace non-alphanumeric characters with hyphens, collapse multiple hyphens.
 */
function slugify(name: string): string {
  return name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '');
}

/**
 * Load user-specific scheduled jobs from their config file.
 * Path: {knowledgeDir}/_users/{username}/_config/scheduled-jobs.yaml
 *
 * Skips jobs with invalid cron expressions.
 * Returns empty array on any read/parse error (graceful degradation).
 */
export function loadUserScheduledJobs(knowledgeDir: string, username: string): UserScheduledJob[] {
  const configPath = path.join(knowledgeDir, '_users', username, '_config', 'scheduled-jobs.yaml');

  if (!fs.existsSync(configPath)) {
    return [];
  }

  let data: any;
  try {
    const content = fs.readFileSync(configPath, 'utf-8');
    data = yaml.load(content);
  } catch (err) {
    console.warn(`[personal-scheduler] Failed to load config for "${username}": ${err}`);
    return [];
  }

  const rawJobs: any[] = data?.scheduled_jobs;
  if (!Array.isArray(rawJobs)) {
    return [];
  }

  const jobs: UserScheduledJob[] = [];
  for (const raw of rawJobs) {
    if (!cron.validate(raw.cron)) {
      console.warn(`[personal-scheduler] Invalid cron expression for "${raw.name}": ${raw.cron}, skipping`);
      continue;
    }

    jobs.push({
      name: raw.name,
      enabled: raw.enabled,
      cron: raw.cron,
      timezone: raw.timezone,
      query: raw.query,
    });
  }

  return jobs;
}

/**
 * Save a job execution result to the user's _job_results directory.
 * Path: {knowledgeDir}/_users/{username}/_job_results/{slug}-{timestamp}.md
 *
 * Returns the absolute path to the saved result file.
 */
export function saveJobResult(
  knowledgeDir: string,
  username: string,
  jobName: string,
  result: string,
): string {
  sanitizeUsername(username);
  const resultsDir = path.join(knowledgeDir, '_users', username, '_job_results');
  if (!fs.existsSync(resultsDir)) {
    fs.mkdirSync(resultsDir, { recursive: true });
  }

  const slug = slugify(jobName);
  const timestamp = uniqueTimestamp();
  const filename = `${slug}-${timestamp}.md`;
  const filePath = path.join(resultsDir, filename);

  fs.writeFileSync(filePath, result, 'utf-8');
  return filePath;
}

/**
 * Clean up old job results, keeping only the most recent `keepCount` files.
 * Only removes files matching the specific job slug pattern.
 */
export function cleanupOldResults(
  knowledgeDir: string,
  username: string,
  jobName: string,
  keepCount: number = 10,
): void {
  sanitizeUsername(username);
  const resultsDir = path.join(knowledgeDir, '_users', username, '_job_results');
  if (!fs.existsSync(resultsDir)) {
    return;
  }

  const slug = slugify(jobName);
  const prefix = `${slug}-`;

  const matchingFiles = fs.readdirSync(resultsDir)
    .filter(f => f.startsWith(prefix) && f.endsWith('.md'))
    .sort(); // Lexicographic sort works since timestamps are fixed-length digits

  if (matchingFiles.length <= keepCount) {
    return;
  }

  const toDelete = matchingFiles.slice(0, matchingFiles.length - keepCount);
  for (const file of toDelete) {
    fs.unlinkSync(path.join(resultsDir, file));
  }
}
