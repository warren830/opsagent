import { describe, it, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import * as http from 'http';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { AdminApi } from '../admin-api';

/**
 * Helper: create a minimal mock http.ServerResponse that captures
 * the status code, headers, and body written to it.
 */
function mockResponse(): http.ServerResponse & {
  _status: number;
  _headers: Record<string, string>;
  _body: string;
} {
  const res = Object.create(http.ServerResponse.prototype) as any;
  res._status = 0;
  res._headers = {} as Record<string, string>;
  res._body = '';
  res.setHeader = (name: string, value: string) => {
    res._headers[name.toLowerCase()] = value;
  };
  res.writeHead = (status: number, headers?: Record<string, string>) => {
    res._status = status;
    if (headers) {
      for (const [k, v] of Object.entries(headers)) {
        res._headers[k.toLowerCase()] = v;
      }
    }
  };
  res.end = (data?: string) => {
    if (data) res._body = data;
  };
  return res;
}

function mockRequest(method: string, headers: Record<string, string> = {}): http.IncomingMessage {
  const req = Object.create(http.IncomingMessage.prototype) as any;
  req.method = method;
  req.headers = {};
  for (const [k, v] of Object.entries(headers)) {
    req.headers[k.toLowerCase()] = v;
  }
  return req;
}

describe('AdminApi authentication', () => {
  let tmpDir: string;
  let api: AdminApi;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'admin-api-test-'));
    api = new AdminApi({
      glossaryConfigPath: path.join(tmpDir, 'glossary.yaml'),
      accountsConfigPath: path.join(tmpDir, 'accounts.yaml'),
      platformsConfigPath: path.join(tmpDir, 'platforms.yaml'),
      skillsConfigPath: path.join(tmpDir, 'skills.yaml'),
      knowledgeDir: path.join(tmpDir, 'knowledge'),
    });
  });

  afterEach(() => {
    delete process.env.ADMIN_API_KEY;
    delete process.env.ADMIN_CORS_ORIGIN;
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  it('returns 401 when ADMIN_API_KEY is set and no header provided', async () => {
    process.env.ADMIN_API_KEY = 'secret-key-123';
    const req = mockRequest('GET');
    const res = mockResponse();
    await api.handleRequest(req, res, '/admin/api/glossary');
    assert.equal(res._status, 401);
    const body = JSON.parse(res._body);
    assert.ok(body.error.includes('Unauthorized'));
  });

  it('returns 401 when ADMIN_API_KEY is set and wrong key provided', async () => {
    process.env.ADMIN_API_KEY = 'secret-key-123';
    const req = mockRequest('GET', { 'X-Admin-Key': 'wrong-key' });
    const res = mockResponse();
    await api.handleRequest(req, res, '/admin/api/glossary');
    assert.equal(res._status, 401);
  });

  it('succeeds when ADMIN_API_KEY is set and correct key provided', async () => {
    process.env.ADMIN_API_KEY = 'secret-key-123';
    const req = mockRequest('GET', { 'X-Admin-Key': 'secret-key-123' });
    const res = mockResponse();
    await api.handleRequest(req, res, '/admin/api/glossary');
    assert.equal(res._status, 200);
  });

  it('succeeds without auth when ADMIN_API_KEY is not set (backwards compat)', async () => {
    delete process.env.ADMIN_API_KEY;
    const req = mockRequest('GET');
    const res = mockResponse();
    await api.handleRequest(req, res, '/admin/api/glossary');
    assert.equal(res._status, 200);
  });
});

describe('AdminApi CORS', () => {
  let tmpDir: string;
  let api: AdminApi;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'admin-api-cors-'));
    api = new AdminApi({
      glossaryConfigPath: path.join(tmpDir, 'glossary.yaml'),
      accountsConfigPath: path.join(tmpDir, 'accounts.yaml'),
      platformsConfigPath: path.join(tmpDir, 'platforms.yaml'),
      skillsConfigPath: path.join(tmpDir, 'skills.yaml'),
      knowledgeDir: path.join(tmpDir, 'knowledge'),
    });
  });

  afterEach(() => {
    delete process.env.ADMIN_CORS_ORIGIN;
    delete process.env.ADMIN_API_KEY;
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  it('sets Access-Control-Allow-Origin to * by default', async () => {
    delete process.env.ADMIN_CORS_ORIGIN;
    const req = mockRequest('GET');
    const res = mockResponse();
    await api.handleRequest(req, res, '/admin/api/glossary');
    assert.equal(res._headers['access-control-allow-origin'], '*');
  });

  it('reflects ADMIN_CORS_ORIGIN env var in CORS header', async () => {
    process.env.ADMIN_CORS_ORIGIN = 'https://admin.example.com';
    const req = mockRequest('GET');
    const res = mockResponse();
    await api.handleRequest(req, res, '/admin/api/glossary');
    assert.equal(res._headers['access-control-allow-origin'], 'https://admin.example.com');
  });
});

describe('AdminApi tenants CRUD', () => {
  let tmpDir: string;
  let api: AdminApi;
  let tenantsPath: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'admin-api-tenants-'));
    tenantsPath = path.join(tmpDir, 'tenants.yaml');
    api = new AdminApi({
      glossaryConfigPath: path.join(tmpDir, 'glossary.yaml'),
      accountsConfigPath: path.join(tmpDir, 'accounts.yaml'),
      platformsConfigPath: path.join(tmpDir, 'platforms.yaml'),
      skillsConfigPath: path.join(tmpDir, 'skills.yaml'),
      knowledgeDir: path.join(tmpDir, 'knowledge'),
      tenantsConfigPath: tenantsPath,
    });
  });

  afterEach(() => {
    delete process.env.ADMIN_API_KEY;
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  it('GET /admin/api/tenants returns empty list when file missing', async () => {
    const req = mockRequest('GET');
    const res = mockResponse();
    await api.handleRequest(req, res, '/admin/api/tenants');
    assert.equal(res._status, 200);
    const body = JSON.parse(res._body);
    assert.deepEqual(body.tenants, []);
  });

  it('PUT /admin/api/tenants saves valid config', async () => {
    const req = mockRequest('PUT');
    const res = mockResponse();
    const tenants = [{
      id: 'team-a',
      name: 'Team A',
      channels: [{ platform: 'feishu', channel_id: 'oc_1' }],
    }];
    await api.handleRequest(req, res, '/admin/api/tenants', { tenants });
    assert.equal(res._status, 200);
    const body = JSON.parse(res._body);
    assert.equal(body.ok, true);
    assert.equal(body.count, 1);

    // Verify file was written
    assert.ok(fs.existsSync(tenantsPath));
  });

  it('PUT /admin/api/tenants validates duplicate channels → 400', async () => {
    const req = mockRequest('PUT');
    const res = mockResponse();
    const tenants = [
      { id: 'a', name: 'A', channels: [{ platform: 'feishu', channel_id: 'oc_1' }] },
      { id: 'b', name: 'B', channels: [{ platform: 'feishu', channel_id: 'oc_1' }] },
    ];
    await api.handleRequest(req, res, '/admin/api/tenants', { tenants });
    assert.equal(res._status, 400);
    const body = JSON.parse(res._body);
    assert.ok(body.error.includes('Duplicate'));
  });

  it('PUT /admin/api/tenants rejects non-array', async () => {
    const req = mockRequest('PUT');
    const res = mockResponse();
    await api.handleRequest(req, res, '/admin/api/tenants', { tenants: 'not-array' });
    assert.equal(res._status, 400);
  });
});

describe('AdminApi path traversal protection', () => {
  let tmpDir: string;
  let api: AdminApi;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'admin-api-traversal-'));
    api = new AdminApi({
      glossaryConfigPath: path.join(tmpDir, 'glossary.yaml'),
      accountsConfigPath: path.join(tmpDir, 'accounts.yaml'),
      platformsConfigPath: path.join(tmpDir, 'platforms.yaml'),
      skillsConfigPath: path.join(tmpDir, 'skills.yaml'),
      knowledgeDir: path.join(tmpDir, 'knowledge'),
    });
  });

  afterEach(() => {
    delete process.env.ADMIN_API_KEY;
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  it('blocks GET with .. in filename', async () => {
    const req = mockRequest('GET');
    const res = mockResponse();
    await api.handleRequest(req, res, '/admin/api/knowledge/..%2F..%2Fetc%2Fpasswd');
    assert.equal(res._status, 400);
    const body = JSON.parse(res._body);
    assert.equal(body.error, 'Invalid filename');
  });

  it('blocks PUT with .. in filename', async () => {
    const req = mockRequest('PUT');
    const res = mockResponse();
    await api.handleRequest(req, res, '/admin/api/knowledge/../secret', { content: 'evil' });
    assert.equal(res._status, 400);
  });

  it('blocks DELETE with / in filename', async () => {
    const req = mockRequest('DELETE');
    const res = mockResponse();
    await api.handleRequest(req, res, '/admin/api/knowledge/sub/dir/file.txt');
    assert.equal(res._status, 400);
  });

  it('blocks filename containing ..', async () => {
    const req = mockRequest('GET');
    const res = mockResponse();
    await api.handleRequest(req, res, '/admin/api/knowledge/..%2Fsecret.txt');
    assert.equal(res._status, 400);
  });
});
