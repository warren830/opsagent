import * as fs from 'fs';
import * as path from 'path';
import * as yaml from 'js-yaml';
import * as cron from 'node-cron';
import { PlatformAdapter } from './adapters/types';
import { ClaudeClient } from './claude-client';
import { loadUserScheduledJobs, saveJobResult, cleanupOldResults, UserScheduledJob } from './personal-scheduler';

export interface ScheduledJobTarget {
  platform: string;
  channel: string;
}

export interface ScheduledJob {
  name: string;
  enabled: boolean;
  cron: string;
  timezone?: string;
  query: string;
  auto_jira?: boolean;
  targets: ScheduledJobTarget[];
}

export class SchedulerManager {
  private tasks: cron.ScheduledTask[] = [];
  private readonly adapters: Map<string, PlatformAdapter>;
  private readonly claudeClient: ClaudeClient;
  private readonly configPath: string;
  private knowledgeDir?: string;

  constructor(
    adapters: Map<string, PlatformAdapter>,
    claudeClient: ClaudeClient,
    configPath: string,
    knowledgeDir?: string,
  ) {
    this.adapters = adapters;
    this.claudeClient = claudeClient;
    this.configPath = configPath;
    this.knowledgeDir = knowledgeDir;
  }

  start(): void {
    // Global jobs
    const jobs = this.loadJobs();
    for (const job of jobs) {
      if (!job.enabled) continue;
      if (!cron.validate(job.cron)) {
        console.error(`[scheduler] Invalid cron expression for "${job.name}": ${job.cron}`);
        continue;
      }
      const task = cron.schedule(job.cron, () => {
        this.executeJob(job).catch(err => {
          console.error(`[scheduler] Error executing job "${job.name}": ${err}`);
        });
      }, {
        timezone: job.timezone || 'UTC',
      });
      this.tasks.push(task);
      console.log(`[scheduler] Registered job "${job.name}" (${job.cron}, tz=${job.timezone || 'UTC'})`);
    }

    // User-scoped jobs
    const userJobCount = this.loadUserJobs();

    console.log(`[scheduler] Started with ${this.tasks.length} active job(s) (${userJobCount} user)`);
  }

  private loadUserJobs(): number {
    if (!this.knowledgeDir) return 0;
    const usersDir = path.join(this.knowledgeDir, '_users');
    if (!fs.existsSync(usersDir)) return 0;

    let count = 0;
    const userDirs = fs.readdirSync(usersDir).filter(d => {
      try { return fs.statSync(path.join(usersDir, d)).isDirectory() && !d.startsWith('_'); }
      catch { return false; }
    });

    for (const username of userDirs) {
      const userJobs = loadUserScheduledJobs(this.knowledgeDir, username);
      for (const job of userJobs) {
        const task = cron.schedule(job.cron, () => {
          this.executeUserJob(username, job).catch(err => {
            console.error(`[scheduler] Error executing user job "${job.name}" for ${username}: ${err}`);
          });
        }, {
          timezone: job.timezone || 'UTC',
        });
        this.tasks.push(task);
        count++;
        console.log(`[scheduler] Registered user job "${job.name}" for ${username} (${job.cron})`);
      }
    }
    return count;
  }

  private async executeUserJob(username: string, job: UserScheduledJob): Promise<void> {
    console.log(`[scheduler] Executing user job "${job.name}" for ${username}`);
    try {
      const result = await this.claudeClient.query(job.query, 'scheduler', username, undefined, username);
      if (result && this.knowledgeDir) {
        saveJobResult(this.knowledgeDir, username, job.name, result);
        cleanupOldResults(this.knowledgeDir, username, job.name);
      }
    } catch (err) {
      console.error(`[scheduler] Claude query failed for user job "${job.name}" (${username}): ${err}`);
    }
  }

  reload(): void {
    this.stop();
    this.start();
  }

  stop(): void {
    for (const task of this.tasks) {
      task.stop();
    }
    this.tasks = [];
  }

  private loadJobs(): ScheduledJob[] {
    if (!fs.existsSync(this.configPath)) return [];
    try {
      const content = fs.readFileSync(this.configPath, 'utf-8');
      const data = yaml.load(content) as any;
      return data?.scheduled_jobs || [];
    } catch (err) {
      console.error(`[scheduler] Failed to load config: ${err}`);
      return [];
    }
  }

  private async executeJob(job: ScheduledJob): Promise<void> {
    console.log(`[scheduler] Executing job "${job.name}"`);

    let query = job.query;
    if (job.auto_jira) {
      query += '\n\n重要：如果发现异常，使用 Jira MCP 工具自动创建工单。参考 Jira Integration skill 中的建单规范。';
    }

    let result: string;
    try {
      result = await this.claudeClient.query(query, 'scheduler', job.name);
    } catch (err) {
      console.error(`[scheduler] Claude query failed for "${job.name}": ${err}`);
      return;
    }

    if (!result) {
      console.warn(`[scheduler] Empty result for job "${job.name}"`);
      return;
    }

    for (const target of job.targets) {
      const adapter = this.adapters.get(target.platform);
      if (!adapter) {
        console.warn(`[scheduler] No adapter for platform "${target.platform}" (job "${job.name}")`);
        continue;
      }
      if (!adapter.sendToChannel) {
        console.warn(`[scheduler] Adapter "${target.platform}" does not support sendToChannel (job "${job.name}")`);
        continue;
      }
      try {
        await adapter.sendToChannel(target.channel, result);
        console.log(`[scheduler] Sent result to ${target.platform}/${target.channel} for job "${job.name}"`);
      } catch (err) {
        console.error(`[scheduler] Failed to send to ${target.platform}/${target.channel}: ${err}`);
      }
    }
  }
}
