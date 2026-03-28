import { describe, it, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import * as yaml from 'js-yaml';

import { loadUserSkillsConfig, loadMergedSkills } from '../personal-skills';
import type { SkillsConfig } from '../skills-loader';

/* ── helpers ─────────────────────────────────────────────────── */

function writeYaml(filePath: string, data: unknown): void {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, yaml.dump(data), 'utf-8');
}

function makeGlobalConfig(dir: string, skills: SkillsConfig['skills']): string {
  const configPath = path.join(dir, 'skills.yaml');
  writeYaml(configPath, { skills });
  return configPath;
}

function makeTenantConfig(
  knowledgeDir: string,
  tenantId: string,
  overrides: Array<{ name: string; enabled: boolean }>,
): void {
  const configPath = path.join(
    knowledgeDir, '_tenants', tenantId, '_config', 'skills.yaml',
  );
  writeYaml(configPath, { overrides });
}

function makeUserConfig(
  knowledgeDir: string,
  username: string,
  data: {
    overrides?: Array<{ name: string; enabled: boolean }>;
    custom_skills?: Array<{ name: string; description: string; instructions: string }>;
  },
): void {
  const configPath = path.join(
    knowledgeDir, '_users', username, '_config', 'skills.yaml',
  );
  writeYaml(configPath, data);
}

const SKILL_A = {
  name: 'ECS Troubleshooting',
  description: 'ECS troubleshooting guide',
  enabled: true,
  instructions: 'Check ECS service status',
};

const SKILL_B = {
  name: 'Jira Integration',
  description: 'Jira ticket management',
  enabled: true,
  instructions: 'Create and query Jira tickets',
};

const SKILL_C_DISABLED = {
  name: 'Legacy Tool',
  description: 'A disabled skill',
  enabled: false,
  instructions: 'Should remain disabled',
};

/* ── loadUserSkillsConfig ────────────────────────────────────── */

describe('loadUserSkillsConfig', () => {
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'ps-test-'));
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  it('returns null when file does not exist', () => {
    const result = loadUserSkillsConfig(path.join(tmpDir, 'nonexistent.yaml'));
    assert.equal(result, null);
  });

  it('parses overrides from YAML', () => {
    const configPath = path.join(tmpDir, 'skills.yaml');
    writeYaml(configPath, {
      overrides: [{ name: 'ECS Troubleshooting', enabled: false }],
    });
    const result = loadUserSkillsConfig(configPath);
    assert.ok(result);
    assert.equal(result.overrides!.length, 1);
    assert.equal(result.overrides![0].name, 'ECS Troubleshooting');
    assert.equal(result.overrides![0].enabled, false);
  });

  it('parses custom_skills from YAML', () => {
    const configPath = path.join(tmpDir, 'skills.yaml');
    writeYaml(configPath, {
      custom_skills: [{
        name: 'My Deploy Checklist',
        description: 'Personal deploy checklist',
        instructions: '1. Check alarms\n2. Verify snapshots',
      }],
    });
    const result = loadUserSkillsConfig(configPath);
    assert.ok(result);
    assert.equal(result.custom_skills!.length, 1);
    assert.equal(result.custom_skills![0].name, 'My Deploy Checklist');
  });

  it('parses both overrides and custom_skills', () => {
    const configPath = path.join(tmpDir, 'skills.yaml');
    writeYaml(configPath, {
      overrides: [{ name: 'Jira Integration', enabled: false }],
      custom_skills: [{
        name: 'Custom Skill',
        description: 'desc',
        instructions: 'do something',
      }],
    });
    const result = loadUserSkillsConfig(configPath);
    assert.ok(result);
    assert.equal(result.overrides!.length, 1);
    assert.equal(result.custom_skills!.length, 1);
  });

  it('returns null for invalid YAML (not an object)', () => {
    const configPath = path.join(tmpDir, 'skills.yaml');
    fs.writeFileSync(configPath, 'just a string', 'utf-8');
    const result = loadUserSkillsConfig(configPath);
    assert.equal(result, null);
  });

  it('returns null for YAML with neither overrides nor custom_skills', () => {
    const configPath = path.join(tmpDir, 'skills.yaml');
    writeYaml(configPath, { something_else: true });
    const result = loadUserSkillsConfig(configPath);
    assert.equal(result, null);
  });
});

/* ── loadMergedSkills ────────────────────────────────────────── */

describe('loadMergedSkills', () => {
  let tmpDir: string;
  let knowledgeDir: string;
  let globalConfigPath: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'ps-test-'));
    knowledgeDir = path.join(tmpDir, 'knowledge');
    fs.mkdirSync(knowledgeDir, { recursive: true });
    globalConfigPath = makeGlobalConfig(tmpDir, [SKILL_A, SKILL_B, SKILL_C_DISABLED]);
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  /* -- layer 1: global only ---------------------------------- */

  it('returns global skills when no tenant or user', () => {
    const result = loadMergedSkills(globalConfigPath, knowledgeDir);
    assert.equal(result.skills.length, 3);
    assert.equal(result.skills[0].name, 'ECS Troubleshooting');
    assert.equal(result.skills[0].enabled, true);
    assert.equal(result.skills[2].name, 'Legacy Tool');
    assert.equal(result.skills[2].enabled, false);
  });

  it('returns empty skills array when global config missing', () => {
    const result = loadMergedSkills(
      path.join(tmpDir, 'nonexistent.yaml'), knowledgeDir,
    );
    assert.deepEqual(result.skills, []);
  });

  /* -- layer 2: tenant overrides ----------------------------- */

  it('applies tenant override to disable a global skill', () => {
    makeTenantConfig(knowledgeDir, 'acme', [
      { name: 'ECS Troubleshooting', enabled: false },
    ]);
    const result = loadMergedSkills(globalConfigPath, knowledgeDir, 'acme');
    const ecs = result.skills.find(s => s.name === 'ECS Troubleshooting');
    assert.ok(ecs);
    assert.equal(ecs.enabled, false);
    // Other skills remain unchanged
    const jira = result.skills.find(s => s.name === 'Jira Integration');
    assert.ok(jira);
    assert.equal(jira.enabled, true);
  });

  it('applies tenant override to enable a disabled global skill', () => {
    makeTenantConfig(knowledgeDir, 'acme', [
      { name: 'Legacy Tool', enabled: true },
    ]);
    const result = loadMergedSkills(globalConfigPath, knowledgeDir, 'acme');
    const legacy = result.skills.find(s => s.name === 'Legacy Tool');
    assert.ok(legacy);
    assert.equal(legacy.enabled, true);
  });

  it('skips tenant layer when tenantId not provided', () => {
    makeTenantConfig(knowledgeDir, 'acme', [
      { name: 'ECS Troubleshooting', enabled: false },
    ]);
    const result = loadMergedSkills(globalConfigPath, knowledgeDir);
    const ecs = result.skills.find(s => s.name === 'ECS Troubleshooting');
    assert.ok(ecs);
    assert.equal(ecs.enabled, true); // unchanged
  });

  it('skips tenant layer when tenant config does not exist', () => {
    const result = loadMergedSkills(
      globalConfigPath, knowledgeDir, 'nonexistent-tenant',
    );
    assert.equal(result.skills.length, 3);
    const ecs = result.skills.find(s => s.name === 'ECS Troubleshooting');
    assert.ok(ecs);
    assert.equal(ecs.enabled, true);
  });

  /* -- layer 3: user overrides + custom skills --------------- */

  it('applies user override to disable a skill', () => {
    makeUserConfig(knowledgeDir, 'alice', {
      overrides: [{ name: 'Jira Integration', enabled: false }],
    });
    const result = loadMergedSkills(
      globalConfigPath, knowledgeDir, undefined, 'alice',
    );
    const jira = result.skills.find(s => s.name === 'Jira Integration');
    assert.ok(jira);
    assert.equal(jira.enabled, false);
  });

  it('appends user custom_skills to the result', () => {
    makeUserConfig(knowledgeDir, 'alice', {
      custom_skills: [{
        name: 'My Deploy Checklist',
        description: 'Personal deploy checklist',
        instructions: '1. Check alarms\n2. Verify snapshots',
      }],
    });
    const result = loadMergedSkills(
      globalConfigPath, knowledgeDir, undefined, 'alice',
    );
    assert.equal(result.skills.length, 4); // 3 global + 1 custom
    const custom = result.skills.find(s => s.name === 'My Deploy Checklist');
    assert.ok(custom);
    assert.equal(custom.enabled, true); // always enabled
    assert.equal(custom.description, 'Personal deploy checklist');
    assert.equal(custom.instructions, '1. Check alarms\n2. Verify snapshots');
  });

  it('skips user layer when username not provided', () => {
    makeUserConfig(knowledgeDir, 'alice', {
      overrides: [{ name: 'ECS Troubleshooting', enabled: false }],
      custom_skills: [{
        name: 'Custom', description: 'd', instructions: 'i',
      }],
    });
    const result = loadMergedSkills(globalConfigPath, knowledgeDir);
    assert.equal(result.skills.length, 3); // no custom appended
    const ecs = result.skills.find(s => s.name === 'ECS Troubleshooting');
    assert.ok(ecs);
    assert.equal(ecs.enabled, true); // override not applied
  });

  it('skips user layer when user config does not exist', () => {
    const result = loadMergedSkills(
      globalConfigPath, knowledgeDir, undefined, 'nonexistent-user',
    );
    assert.equal(result.skills.length, 3);
  });

  /* -- full three-layer merge -------------------------------- */

  it('applies all three layers: global -> tenant -> user', () => {
    // Tenant disables ECS
    makeTenantConfig(knowledgeDir, 'acme', [
      { name: 'ECS Troubleshooting', enabled: false },
    ]);
    // User disables Jira, adds custom skill
    makeUserConfig(knowledgeDir, 'bob', {
      overrides: [{ name: 'Jira Integration', enabled: false }],
      custom_skills: [{
        name: 'Bob Custom',
        description: 'Bob-specific workflow',
        instructions: 'Do the Bob thing',
      }],
    });

    const result = loadMergedSkills(
      globalConfigPath, knowledgeDir, 'acme', 'bob',
    );

    // ECS disabled by tenant
    const ecs = result.skills.find(s => s.name === 'ECS Troubleshooting');
    assert.ok(ecs);
    assert.equal(ecs.enabled, false);

    // Jira disabled by user
    const jira = result.skills.find(s => s.name === 'Jira Integration');
    assert.ok(jira);
    assert.equal(jira.enabled, false);

    // Legacy still disabled (nobody re-enabled it)
    const legacy = result.skills.find(s => s.name === 'Legacy Tool');
    assert.ok(legacy);
    assert.equal(legacy.enabled, false);

    // Custom skill appended
    const custom = result.skills.find(s => s.name === 'Bob Custom');
    assert.ok(custom);
    assert.equal(custom.enabled, true);

    assert.equal(result.skills.length, 4); // 3 global + 1 custom
  });

  it('user override can re-enable a tenant-disabled skill', () => {
    makeTenantConfig(knowledgeDir, 'acme', [
      { name: 'ECS Troubleshooting', enabled: false },
    ]);
    makeUserConfig(knowledgeDir, 'alice', {
      overrides: [{ name: 'ECS Troubleshooting', enabled: true }],
    });

    const result = loadMergedSkills(
      globalConfigPath, knowledgeDir, 'acme', 'alice',
    );
    const ecs = result.skills.find(s => s.name === 'ECS Troubleshooting');
    assert.ok(ecs);
    assert.equal(ecs.enabled, true);
  });

  /* -- error/rescue: invalid config fallback ----------------- */

  it('falls back to global skills when tenant config is malformed', () => {
    const tenantConfigPath = path.join(
      knowledgeDir, '_tenants', 'bad', '_config', 'skills.yaml',
    );
    fs.mkdirSync(path.dirname(tenantConfigPath), { recursive: true });
    fs.writeFileSync(tenantConfigPath, ':::invalid yaml{{[', 'utf-8');

    const result = loadMergedSkills(
      globalConfigPath, knowledgeDir, 'bad',
    );
    // Should still have global skills intact
    assert.equal(result.skills.length, 3);
    const ecs = result.skills.find(s => s.name === 'ECS Troubleshooting');
    assert.ok(ecs);
    assert.equal(ecs.enabled, true);
  });

  it('falls back to previous layer when user config is malformed', () => {
    const userConfigPath = path.join(
      knowledgeDir, '_users', 'baduser', '_config', 'skills.yaml',
    );
    fs.mkdirSync(path.dirname(userConfigPath), { recursive: true });
    fs.writeFileSync(userConfigPath, ':::invalid yaml{{[', 'utf-8');

    const result = loadMergedSkills(
      globalConfigPath, knowledgeDir, undefined, 'baduser',
    );
    // Should still have global skills intact
    assert.equal(result.skills.length, 3);
  });

  /* -- tenant config uses loadSkills format (with overrides field) */

  it('tenant config with overrides array is applied correctly', () => {
    makeTenantConfig(knowledgeDir, 'corp', [
      { name: 'ECS Troubleshooting', enabled: false },
      { name: 'Legacy Tool', enabled: true },
    ]);
    const result = loadMergedSkills(globalConfigPath, knowledgeDir, 'corp');

    const ecs = result.skills.find(s => s.name === 'ECS Troubleshooting');
    assert.ok(ecs);
    assert.equal(ecs.enabled, false);

    const legacy = result.skills.find(s => s.name === 'Legacy Tool');
    assert.ok(legacy);
    assert.equal(legacy.enabled, true);

    // Jira unchanged
    const jira = result.skills.find(s => s.name === 'Jira Integration');
    assert.ok(jira);
    assert.equal(jira.enabled, true);
  });
});
