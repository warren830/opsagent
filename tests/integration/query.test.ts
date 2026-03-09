import * as path from 'path';
import * as fs from 'fs';

// ── plugin-loader tests ──────────────────────────────────────────────

describe('plugin-loader', () => {
  // We import these dynamically so the test file can live outside bot/src
  let loadPlugins: typeof import('../../bot/src/plugin-loader').loadPlugins;
  let generateMcpConfig: typeof import('../../bot/src/plugin-loader').generateMcpConfig;

  beforeAll(() => {
    // eslint-disable-next-line @typescript-eslint/no-var-requires
    const mod = require('../../bot/src/plugin-loader');
    loadPlugins = mod.loadPlugins;
    generateMcpConfig = mod.generateMcpConfig;
  });

  const fixturesDir = path.join(__dirname, '__fixtures__');
  const pluginsYamlPath = path.join(fixturesDir, 'plugins.yaml');

  beforeAll(() => {
    // Create a fixture plugins.yaml for tests
    fs.mkdirSync(fixturesDir, { recursive: true });
    fs.writeFileSync(
      pluginsYamlPath,
      `plugins:
  confluence:
    enabled: true
    type: mcp
    command: "npx"
    args: ["-y", "@anthropic-ai/mcp-remote"]
    env:
      MCP_HEADERS: "Bearer \${CONFLUENCE_TOKEN}"
  jira:
    enabled: false
    type: mcp
    command: "npx"
    args: ["-y", "@anthropic-ai/mcp-remote"]
  github:
    enabled: true
    type: mcp
    command: "npx"
    args: ["-y", "@modelcontextprotocol/server-github"]
    env:
      GITHUB_TOKEN: "\${GITHUB_TOKEN}"
`,
    );
  });

  afterAll(() => {
    fs.rmSync(fixturesDir, { recursive: true, force: true });
  });

  test('loadPlugins returns only enabled plugins', () => {
    const plugins = loadPlugins(pluginsYamlPath);
    const names = Object.keys(plugins);

    expect(names).toContain('confluence');
    expect(names).toContain('github');
    expect(names).not.toContain('jira');
    expect(names).toHaveLength(2);
  });

  test('loadPlugins returns empty object for missing file', () => {
    const plugins = loadPlugins('/nonexistent/path/plugins.yaml');
    expect(plugins).toEqual({});
  });

  test('generateMcpConfig produces valid mcp.json structure', () => {
    const plugins = loadPlugins(pluginsYamlPath);
    const outputPath = path.join(fixturesDir, 'mcp.json');

    // Set env vars so substitution works
    process.env.CONFLUENCE_TOKEN = 'test-token-123';
    process.env.GITHUB_TOKEN = 'ghp_test456';

    generateMcpConfig(plugins, outputPath);

    const mcpJson = JSON.parse(fs.readFileSync(outputPath, 'utf-8'));

    // Top-level structure
    expect(mcpJson).toHaveProperty('mcpServers');
    expect(typeof mcpJson.mcpServers).toBe('object');

    // Only enabled plugins present
    expect(Object.keys(mcpJson.mcpServers)).toEqual(
      expect.arrayContaining(['confluence', 'github']),
    );
    expect(mcpJson.mcpServers).not.toHaveProperty('jira');

    // Each entry has command + args
    for (const entry of Object.values<any>(mcpJson.mcpServers)) {
      expect(entry).toHaveProperty('command');
      expect(entry).toHaveProperty('args');
      expect(Array.isArray(entry.args)).toBe(true);
    }

    // Environment variable substitution
    expect(mcpJson.mcpServers.confluence.env.MCP_HEADERS).toBe(
      'Bearer test-token-123',
    );
    expect(mcpJson.mcpServers.github.env.GITHUB_TOKEN).toBe('ghp_test456');

    // Cleanup
    delete process.env.CONFLUENCE_TOKEN;
    delete process.env.GITHUB_TOKEN;
  });
});

// ── glossary-loader tests ───────────────────────────────────────────

describe('glossary-loader', () => {
  let loadGlossary: typeof import('../../bot/src/glossary-loader').loadGlossary;
  let generateGlossaryKnowledge: typeof import('../../bot/src/glossary-loader').generateGlossaryKnowledge;

  beforeAll(() => {
    const mod = require('../../bot/src/glossary-loader');
    loadGlossary = mod.loadGlossary;
    generateGlossaryKnowledge = mod.generateGlossaryKnowledge;
  });

  const fixturesDir = path.join(__dirname, '__fixtures_glossary__');

  afterAll(() => {
    fs.rmSync(fixturesDir, { recursive: true, force: true });
  });

  test('loadGlossary returns null for missing file', () => {
    expect(loadGlossary('/nonexistent/glossary.yaml')).toBeNull();
  });

  test('loadGlossary parses valid glossary', () => {
    fs.mkdirSync(fixturesDir, { recursive: true });
    const yamlPath = path.join(fixturesDir, 'glossary.yaml');
    fs.writeFileSync(yamlPath, `glossary:
  ics:
    full_name: "Inventory Control System"
    description: "Core inventory system"
    aliases: ["inventory-system"]
    aws_accounts: ["123456789012"]
    services: ["ics-api"]
`);
    const result = loadGlossary(yamlPath);
    expect(result).not.toBeNull();
    expect(result!.glossary.ics.full_name).toBe('Inventory Control System');
  });

  test('generateGlossaryKnowledge creates markdown and returns summary', () => {
    const config = {
      glossary: {
        ics: {
          full_name: 'Inventory Control System',
          description: 'Core inventory',
          aws_accounts: ['123456789012'],
        },
      },
    };
    const knowledgeDir = path.join(fixturesDir, 'knowledge');
    const summary = generateGlossaryKnowledge(config as any, knowledgeDir);
    expect(summary).toContain('ics');
    expect(summary).toContain('123456789012');
    expect(fs.existsSync(path.join(knowledgeDir, 'glossary.md'))).toBe(true);
  });
});

// ── accounts-loader tests ───────────────────────────────────────────

describe('accounts-loader', () => {
  let loadAccounts: typeof import('../../bot/src/accounts-loader').loadAccounts;
  let generateAccountsKnowledge: typeof import('../../bot/src/accounts-loader').generateAccountsKnowledge;

  beforeAll(() => {
    const mod = require('../../bot/src/accounts-loader');
    loadAccounts = mod.loadAccounts;
    generateAccountsKnowledge = mod.generateAccountsKnowledge;
  });

  const fixturesDir = path.join(__dirname, '__fixtures_accounts__');

  afterAll(() => {
    fs.rmSync(fixturesDir, { recursive: true, force: true });
  });

  test('loadAccounts returns null for missing file', () => {
    expect(loadAccounts('/nonexistent/accounts.yaml')).toBeNull();
  });

  test('loadAccounts parses valid accounts config', () => {
    fs.mkdirSync(fixturesDir, { recursive: true });
    const yamlPath = path.join(fixturesDir, 'accounts.yaml');
    fs.writeFileSync(yamlPath, `accounts:
  extra:
    - id: "111111111111"
      name: "partner-staging"
      role_name: "OpsAgentPartnerRole"
  defaults:
    role_name: "OpsAgentReadOnly"
`);
    const result = loadAccounts(yamlPath);
    expect(result).not.toBeNull();
    expect(result!.accounts.extra![0].name).toBe('partner-staging');
  });

  test('generateAccountsKnowledge creates markdown and returns summary', () => {
    const config = {
      accounts: {
        extra: [{ id: '111111111111', name: 'partner-staging' }],
        overrides: { '034362076319': { alias: 'hub-account' } },
      },
    };
    const knowledgeDir = path.join(fixturesDir, 'knowledge');
    const summary = generateAccountsKnowledge(config as any, knowledgeDir);
    expect(summary).toContain('partner-staging');
    expect(summary).toContain('hub-account');
    expect(fs.existsSync(path.join(knowledgeDir, 'accounts.md'))).toBe(true);
  });
});

// ── platform-loader tests ───────────────────────────────────────────

describe('platform-loader', () => {
  let loadPlatforms: typeof import('../../bot/src/platform-loader').loadPlatforms;
  let isPlatformEnabled: typeof import('../../bot/src/platform-loader').isPlatformEnabled;
  let getPlatformSettings: typeof import('../../bot/src/platform-loader').getPlatformSettings;

  beforeAll(() => {
    const mod = require('../../bot/src/platform-loader');
    loadPlatforms = mod.loadPlatforms;
    isPlatformEnabled = mod.isPlatformEnabled;
    getPlatformSettings = mod.getPlatformSettings;
  });

  const fixturesDir = path.join(__dirname, '__fixtures_platforms__');

  afterAll(() => {
    fs.rmSync(fixturesDir, { recursive: true, force: true });
  });

  test('loadPlatforms returns defaults for missing file', () => {
    const config = loadPlatforms('/nonexistent/platforms.yaml');
    expect(config.platforms.teams.enabled).toBe(true);
  });

  test('loadPlatforms parses valid config', () => {
    fs.mkdirSync(fixturesDir, { recursive: true });
    const yamlPath = path.join(fixturesDir, 'platforms.yaml');
    fs.writeFileSync(yamlPath, `platforms:
  teams:
    enabled: true
    settings:
      welcome_message: "Hello"
  slack:
    enabled: false
    settings:
      allowed_channels: ["C123"]
  feishu:
    enabled: false
    settings: {}
`);
    const config = loadPlatforms(yamlPath);
    expect(isPlatformEnabled(config, 'teams')).toBe(true);
    expect(isPlatformEnabled(config, 'slack')).toBe(false);
    expect(getPlatformSettings(config, 'teams').welcome_message).toBe('Hello');
    expect(getPlatformSettings(config, 'slack').allowed_channels).toEqual(['C123']);
  });
});

// ── message-handler tests ────────────────────────────────────────────

describe('message-handler', () => {
  let MessageHandler: typeof import('../../bot/src/message-handler').MessageHandler;

  beforeAll(() => {
    const mod = require('../../bot/src/message-handler');
    MessageHandler = mod.MessageHandler;
  });

  function makeMockAdapter() {
    return {
      platform: 'test',
      handleWebhook: jest.fn(),
      sendTyping: jest.fn().mockResolvedValue(undefined),
      sendReply: jest.fn().mockResolvedValue(undefined),
    };
  }

  function makeMockMessage(text: string) {
    return {
      text,
      userId: 'user1',
      userName: 'TestUser',
      conversationId: 'conv1',
      platform: 'test',
      replyContext: {},
    };
  }

  test('short response is sent as reply', async () => {
    const mockClient = { query: jest.fn().mockResolvedValue('All good') };
    const handler = new MessageHandler(mockClient as any);
    const adapter = makeMockAdapter();
    const msg = makeMockMessage('status check');

    await handler.handleMessage(adapter as any, msg);

    expect(mockClient.query).toHaveBeenCalledWith('status check');
    // typing + processing message + result
    expect(adapter.sendReply).toHaveBeenCalledTimes(2);
  });

  test('timeout error sends friendly message', async () => {
    const mockClient = {
      query: jest.fn().mockRejectedValue(new Error('timed out after 300s')),
    };
    const handler = new MessageHandler(mockClient as any);
    const adapter = makeMockAdapter();
    const msg = makeMockMessage('complex query');

    await handler.handleMessage(adapter as any, msg);

    const lastCall = adapter.sendReply.mock.calls[adapter.sendReply.mock.calls.length - 1];
    expect(lastCall[1]).toContain('timed out');
  });
});

// ── claude-client tests ─────────────────────────────────────────────

// Mock child_process before importing claude-client
const mockSpawn = jest.fn();
jest.mock('child_process', () => ({
  ...jest.requireActual('child_process'),
  spawn: mockSpawn,
}));

describe('claude-client', () => {
  let ClaudeClient: typeof import('../../bot/src/claude-client').ClaudeClient;

  beforeAll(() => {
    // Re-require after mock is set up
    jest.isolateModules(() => {
      const mod = require('../../bot/src/claude-client');
      ClaudeClient = mod.ClaudeClient;
    });
  });

  afterEach(() => {
    mockSpawn.mockReset();
  });

  test('query calls spawn with correct arguments', async () => {
    const EventEmitter = require('events');

    // Create mock child process
    const mockChild = new EventEmitter();
    mockChild.pid = 12345;
    mockChild.stdout = new EventEmitter();
    mockChild.stderr = new EventEmitter();
    mockChild.kill = jest.fn();

    mockSpawn.mockReturnValue(mockChild);

    // Create a minimal fixture so loadPlugins doesn't warn
    const fixturesDir = path.join(__dirname, '__fixtures_claude__');
    fs.mkdirSync(fixturesDir, { recursive: true });
    fs.writeFileSync(
      path.join(fixturesDir, 'plugins.yaml'),
      'plugins:\n  test:\n    enabled: false\n    type: mcp\n    command: echo\n    args: []\n',
    );

    const client = new ClaudeClient({
      workDir: '/tmp/test-workdir',
      pluginsConfigPath: path.join(fixturesDir, 'plugins.yaml'),
      mcpConfigPath: path.join(fixturesDir, 'mcp.json'),
      timeoutMs: 5000,
    });

    const queryPromise = client.query('list ec2 instances');

    // Simulate stdout and process exit
    mockChild.stdout.emit('data', Buffer.from('mock claude output'));
    mockChild.emit('close', 0);

    const result = await queryPromise;

    expect(mockSpawn).toHaveBeenCalledWith(
      'claude',
      expect.arrayContaining([
        '-p', 'list ec2 instances',
        '--allowedTools',
        '--system-prompt',
        '--output-format', 'text',
      ]),
      expect.objectContaining({
        cwd: '/tmp/test-workdir',
      }),
    );

    expect(result).toBe('mock claude output');

    fs.rmSync(fixturesDir, { recursive: true, force: true });
  });
});
