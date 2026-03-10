import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import * as fs from 'fs';
import * as path from 'path';

const PROJECT_ROOT = path.resolve(__dirname, '..', '..', '..');

describe('Security: foreach-account.sh', () => {
  it('should NOT contain eval (command injection risk)', () => {
    const scriptPath = path.join(PROJECT_ROOT, 'scripts', 'foreach-account.sh');
    assert.ok(fs.existsSync(scriptPath), `Script not found: ${scriptPath}`);
    const content = fs.readFileSync(scriptPath, 'utf-8');
    // Check for eval usage (word-boundary match to avoid false positives like "evaluation")
    const evalMatches = content.match(/\beval\b/g);
    assert.equal(
      evalMatches,
      null,
      `foreach-account.sh contains 'eval' which is a command injection risk. ` +
        `Found ${evalMatches?.length ?? 0} occurrence(s). Use direct execution instead.`,
    );
  });
});

describe('Security: claude-client.ts', () => {
  it('should use allowedTools to restrict tool access', () => {
    const clientPath = path.join(PROJECT_ROOT, 'bot', 'src', 'claude-client.ts');
    assert.ok(fs.existsSync(clientPath), `File not found: ${clientPath}`);
    const content = fs.readFileSync(clientPath, 'utf-8');
    assert.ok(
      content.includes('--allowedTools'),
      'claude-client.ts should use --allowedTools to restrict tool access',
    );
    // Verify git commands are denied
    assert.ok(
      content.includes('git*:deny'),
      'claude-client.ts should deny git commands via allowedTools',
    );
  });
});

describe('Security: admin-api.ts path traversal', () => {
  it('blocks filenames containing ".."', () => {
    const apiPath = path.join(PROJECT_ROOT, 'bot', 'src', 'admin-api.ts');
    assert.ok(fs.existsSync(apiPath), `File not found: ${apiPath}`);
    const content = fs.readFileSync(apiPath, 'utf-8');
    // Verify that the path traversal check exists for both ".." and "/"
    assert.ok(
      content.includes("filename.includes('..')"),
      'admin-api.ts should check for ".." in filenames',
    );
    assert.ok(
      content.includes("filename.includes('/')"),
      'admin-api.ts should check for "/" in filenames',
    );
  });

  it('applies path traversal check in GET, PUT, and DELETE knowledge file handlers', () => {
    const apiPath = path.join(PROJECT_ROOT, 'bot', 'src', 'admin-api.ts');
    const content = fs.readFileSync(apiPath, 'utf-8');
    // Each of the three methods (getKnowledgeFile, putKnowledgeFile, deleteKnowledgeFile)
    // should contain the traversal check
    const methods = ['getKnowledgeFile', 'putKnowledgeFile', 'deleteKnowledgeFile'];
    for (const method of methods) {
      const methodRegex = new RegExp(
        `private\\s+${method}[\\s\\S]*?filename\\.includes\\(['"]\\.\\.['"\\)]`,
      );
      assert.ok(
        methodRegex.test(content),
        `${method} should contain path traversal check for ".."`,
      );
    }
  });
});
