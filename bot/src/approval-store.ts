/**
 * Approval queue for kubectl write operations.
 * In-memory store with periodic JSON persistence.
 */
import * as fs from 'fs';
import * as path from 'path';

export interface ApprovalRequest {
  id: string;
  command: string;
  status: 'pending' | 'approved' | 'rejected' | 'executed' | 'failed';
  requestedBy: string;
  requestedByName: string;
  platform: string;
  channelId: string;
  tenantId?: string;
  createdAt: string;
  resolvedAt?: string;
  resolvedBy?: string;
  rejectReason?: string;
  executionResult?: string;
}

const MAX_RECORDS = 200;

export class ApprovalStore {
  private requests: ApprovalRequest[] = [];
  private nextId = 1;
  private readonly filePath: string;
  private saveTimer: ReturnType<typeof setInterval> | null = null;
  private dirty = false;

  constructor(persistPath: string) {
    this.filePath = persistPath;
    this.load();
    // Persist every 30 seconds if dirty
    this.saveTimer = setInterval(() => { if (this.dirty) this.save(); }, 30_000);
    if (this.saveTimer.unref) this.saveTimer.unref();
  }

  create(data: {
    command: string;
    requestedBy: string;
    requestedByName: string;
    platform: string;
    channelId: string;
    tenantId?: string;
  }): ApprovalRequest {
    const req: ApprovalRequest = {
      id: String(this.nextId++),
      ...data,
      status: 'pending',
      createdAt: new Date().toISOString(),
    };
    this.requests.push(req);
    this.trim();
    this.dirty = true;
    this.save(); // persist immediately for new approvals
    return req;
  }

  list(filter?: { status?: string; tenantId?: string }): ApprovalRequest[] {
    let result = [...this.requests];
    if (filter?.status) result = result.filter(r => r.status === filter.status);
    if (filter?.tenantId) result = result.filter(r => r.tenantId === filter.tenantId);
    return result.reverse(); // newest first
  }

  pendingCount(tenantId?: string): number {
    return this.requests.filter(r =>
      r.status === 'pending' && (!tenantId || r.tenantId === tenantId),
    ).length;
  }

  get(id: string): ApprovalRequest | null {
    return this.requests.find(r => r.id === id) || null;
  }

  approve(id: string, approvedBy: string): ApprovalRequest | null {
    const req = this.get(id);
    if (!req || req.status !== 'pending') return null;
    req.status = 'approved';
    req.resolvedAt = new Date().toISOString();
    req.resolvedBy = approvedBy;
    this.dirty = true;
    return req;
  }

  reject(id: string, rejectedBy: string, reason?: string): ApprovalRequest | null {
    const req = this.get(id);
    if (!req || req.status !== 'pending') return null;
    req.status = 'rejected';
    req.resolvedAt = new Date().toISOString();
    req.resolvedBy = rejectedBy;
    req.rejectReason = reason;
    this.dirty = true;
    this.save();
    return req;
  }

  markExecuted(id: string, result: string): void {
    const req = this.get(id);
    if (!req) return;
    req.status = 'executed';
    req.executionResult = result;
    this.dirty = true;
    this.save();
  }

  markFailed(id: string, error: string): void {
    const req = this.get(id);
    if (!req) return;
    req.status = 'failed';
    req.executionResult = error;
    this.dirty = true;
    this.save();
  }

  private trim(): void {
    if (this.requests.length > MAX_RECORDS) {
      // Remove oldest non-pending entries
      const pending = this.requests.filter(r => r.status === 'pending');
      const completed = this.requests.filter(r => r.status !== 'pending');
      const keep = completed.slice(-MAX_RECORDS + pending.length);
      this.requests = [...keep, ...pending].sort(
        (a, b) => Number(a.id) - Number(b.id),
      );
    }
  }

  private load(): void {
    try {
      if (fs.existsSync(this.filePath)) {
        const data = JSON.parse(fs.readFileSync(this.filePath, 'utf-8'));
        this.requests = data.requests || [];
        this.nextId = data.nextId || (this.requests.length > 0
          ? Math.max(...this.requests.map(r => Number(r.id))) + 1
          : 1);
      }
    } catch (err) {
      console.warn(`[approval-store] Failed to load: ${(err as Error).message}`);
    }
  }

  private save(): void {
    try {
      const dir = path.dirname(this.filePath);
      if (!fs.existsSync(dir)) fs.mkdirSync(dir, { recursive: true });
      const tmpPath = this.filePath + '.tmp.' + Date.now();
      fs.writeFileSync(tmpPath, JSON.stringify({ requests: this.requests, nextId: this.nextId }, null, 2));
      fs.renameSync(tmpPath, this.filePath);
      this.dirty = false;
    } catch (err) {
      console.error(`[approval-store] Failed to save: ${(err as Error).message}`);
    }
  }
}
