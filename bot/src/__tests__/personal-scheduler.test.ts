import { describe, it, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';

import {
  loadUserScheduledJobs,
  saveJobResult,
  cleanupOldResults,
  type UserScheduledJob,
} from '../personal-scheduler';

describe('loadUserScheduledJobs', () => {
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'ps-test-'));
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  it('loads valid jobs from YAML config', () => {
    const configDir = path.join(tmpDir, '_users', 'alice', '_config');
    fs.mkdirSync(configDir, { recursive: true });
    fs.writeFileSync(
      path.join(configDir, 'scheduled-jobs.yaml'),
      `scheduled_jobs:
  - name: "Daily Check"
    enabled: true
    cron: "0 9 * * 1-5"
    timezone: "Asia/Shanghai"
    query: "Check EC2 instances"
  - name: "Weekly Report"
    enabled: false
    cron: "0 10 * * 1"
    query: "Generate weekly report"
`,
    );

    const jobs = loadUserScheduledJobs(tmpDir, 'alice');
    assert.equal(jobs.length, 2);
    assert.deepStrictEqual(jobs[0], {
      name: 'Daily Check',
      enabled: true,
      cron: '0 9 * * 1-5',
      timezone: 'Asia/Shanghai',
      query: 'Check EC2 instances',
    });
    assert.deepStrictEqual(jobs[1], {
      name: 'Weekly Report',
      enabled: false,
      cron: '0 10 * * 1',
      timezone: undefined,
      query: 'Generate weekly report',
    });
  });

  it('returns empty array when config file does not exist', () => {
    // No config file created — user dir doesn't even exist
    const jobs = loadUserScheduledJobs(tmpDir, 'bob');
    assert.deepStrictEqual(jobs, []);
  });

  it('returns empty array when YAML has no scheduled_jobs key', () => {
    const configDir = path.join(tmpDir, '_users', 'carol', '_config');
    fs.mkdirSync(configDir, { recursive: true });
    fs.writeFileSync(
      path.join(configDir, 'scheduled-jobs.yaml'),
      'some_other_key: value\n',
    );

    const jobs = loadUserScheduledJobs(tmpDir, 'carol');
    assert.deepStrictEqual(jobs, []);
  });

  it('returns empty array when YAML file is empty', () => {
    const configDir = path.join(tmpDir, '_users', 'dave', '_config');
    fs.mkdirSync(configDir, { recursive: true });
    fs.writeFileSync(path.join(configDir, 'scheduled-jobs.yaml'), '');

    const jobs = loadUserScheduledJobs(tmpDir, 'dave');
    assert.deepStrictEqual(jobs, []);
  });

  it('skips jobs with invalid cron expressions', () => {
    const configDir = path.join(tmpDir, '_users', 'eve', '_config');
    fs.mkdirSync(configDir, { recursive: true });
    fs.writeFileSync(
      path.join(configDir, 'scheduled-jobs.yaml'),
      `scheduled_jobs:
  - name: "Valid Job"
    enabled: true
    cron: "0 9 * * 1-5"
    query: "Check stuff"
  - name: "Invalid Cron Job"
    enabled: true
    cron: "not a cron"
    query: "This should be skipped"
  - name: "Another Valid"
    enabled: true
    cron: "*/5 * * * *"
    query: "Check more stuff"
`,
    );

    const jobs = loadUserScheduledJobs(tmpDir, 'eve');
    assert.equal(jobs.length, 2);
    assert.equal(jobs[0].name, 'Valid Job');
    assert.equal(jobs[1].name, 'Another Valid');
  });

  it('returns empty array on malformed YAML (graceful degradation)', () => {
    const configDir = path.join(tmpDir, '_users', 'frank', '_config');
    fs.mkdirSync(configDir, { recursive: true });
    fs.writeFileSync(
      path.join(configDir, 'scheduled-jobs.yaml'),
      '{{{{ invalid yaml :::',
    );

    const jobs = loadUserScheduledJobs(tmpDir, 'frank');
    assert.deepStrictEqual(jobs, []);
  });
});

describe('saveJobResult', () => {
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'ps-test-'));
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  it('saves result to _job_results directory and returns the file path', () => {
    const resultPath = saveJobResult(tmpDir, 'alice', 'Daily Check', '# Result\nAll good');
    assert.ok(fs.existsSync(resultPath));
    assert.equal(fs.readFileSync(resultPath, 'utf-8'), '# Result\nAll good');
  });

  it('creates _job_results directory if it does not exist', () => {
    const resultsDir = path.join(tmpDir, '_users', 'alice', '_job_results');
    assert.ok(!fs.existsSync(resultsDir));

    saveJobResult(tmpDir, 'alice', 'My Job', 'result data');
    assert.ok(fs.existsSync(resultsDir));
  });

  it('slugifies job name in the filename', () => {
    const resultPath = saveJobResult(tmpDir, 'alice', 'My Daily Check!', 'data');
    const filename = path.basename(resultPath);
    // Should start with "my-daily-check-" (slugified) and end with ".md"
    assert.ok(filename.startsWith('my-daily-check-'), `Expected slugified prefix, got: ${filename}`);
    assert.ok(filename.endsWith('.md'), `Expected .md extension, got: ${filename}`);
  });

  it('includes timestamp in the filename', () => {
    const resultPath = saveJobResult(tmpDir, 'alice', 'Check', 'data');
    const filename = path.basename(resultPath, '.md');
    // Format: {slug}-{timestamp} where timestamp is digits
    const parts = filename.split('-');
    const lastPart = parts[parts.length - 1];
    assert.ok(/^\d+$/.test(lastPart), `Expected numeric timestamp, got: ${lastPart}`);
  });

  it('saves multiple results as separate files', () => {
    const path1 = saveJobResult(tmpDir, 'alice', 'Check', 'result 1');
    const path2 = saveJobResult(tmpDir, 'alice', 'Check', 'result 2');
    assert.notEqual(path1, path2);
    assert.equal(fs.readFileSync(path1, 'utf-8'), 'result 1');
    assert.equal(fs.readFileSync(path2, 'utf-8'), 'result 2');
  });
});

describe('cleanupOldResults', () => {
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'ps-test-'));
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  it('keeps only the last N results (default 10)', () => {
    // Create 12 result files with staggered timestamps
    const resultsDir = path.join(tmpDir, '_users', 'alice', '_job_results');
    fs.mkdirSync(resultsDir, { recursive: true });

    const files: string[] = [];
    for (let i = 0; i < 12; i++) {
      const ts = 1000000000000 + i * 1000;
      const filename = `daily-check-${ts}.md`;
      fs.writeFileSync(path.join(resultsDir, filename), `result ${i}`);
      files.push(filename);
    }

    cleanupOldResults(tmpDir, 'alice', 'Daily Check');

    const remaining = fs.readdirSync(resultsDir).filter(f => f.startsWith('daily-check-'));
    assert.equal(remaining.length, 10);

    // The 2 oldest should be removed (index 0 and 1)
    assert.ok(!fs.existsSync(path.join(resultsDir, files[0])));
    assert.ok(!fs.existsSync(path.join(resultsDir, files[1])));
    // The newest should remain
    assert.ok(fs.existsSync(path.join(resultsDir, files[11])));
  });

  it('supports custom keepCount', () => {
    const resultsDir = path.join(tmpDir, '_users', 'bob', '_job_results');
    fs.mkdirSync(resultsDir, { recursive: true });

    for (let i = 0; i < 5; i++) {
      const ts = 1000000000000 + i * 1000;
      fs.writeFileSync(path.join(resultsDir, `my-job-${ts}.md`), `result ${i}`);
    }

    cleanupOldResults(tmpDir, 'bob', 'My Job', 3);

    const remaining = fs.readdirSync(resultsDir).filter(f => f.startsWith('my-job-'));
    assert.equal(remaining.length, 3);
  });

  it('does nothing when fewer results than keepCount', () => {
    const resultsDir = path.join(tmpDir, '_users', 'carol', '_job_results');
    fs.mkdirSync(resultsDir, { recursive: true });

    for (let i = 0; i < 3; i++) {
      const ts = 1000000000000 + i * 1000;
      fs.writeFileSync(path.join(resultsDir, `check-${ts}.md`), `result ${i}`);
    }

    cleanupOldResults(tmpDir, 'carol', 'Check');

    const remaining = fs.readdirSync(resultsDir).filter(f => f.startsWith('check-'));
    assert.equal(remaining.length, 3);
  });

  it('does nothing when _job_results directory does not exist', () => {
    // Should not throw
    cleanupOldResults(tmpDir, 'nonexistent', 'Some Job');
  });

  it('only cleans up files matching the specific job slug', () => {
    const resultsDir = path.join(tmpDir, '_users', 'dave', '_job_results');
    fs.mkdirSync(resultsDir, { recursive: true });

    // Create files for two different jobs
    for (let i = 0; i < 5; i++) {
      const ts = 1000000000000 + i * 1000;
      fs.writeFileSync(path.join(resultsDir, `job-a-${ts}.md`), `a-${i}`);
      fs.writeFileSync(path.join(resultsDir, `job-b-${ts}.md`), `b-${i}`);
    }

    cleanupOldResults(tmpDir, 'dave', 'Job A', 2);

    const remainingA = fs.readdirSync(resultsDir).filter(f => f.startsWith('job-a-'));
    const remainingB = fs.readdirSync(resultsDir).filter(f => f.startsWith('job-b-'));
    assert.equal(remainingA.length, 2);
    assert.equal(remainingB.length, 5); // untouched
  });
});
