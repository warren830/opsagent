export interface AuditEntry {
  platform: string;
  userId: string;
  userName: string;
  query: string;
  responseLength: number;
  durationMs: number;
  success: boolean;
  error?: string;
  sessionId?: string;
}

export class AuditLogger {
  /**
   * Log an audit entry as structured JSON to stdout.
   * CloudWatch Logs picks up stdout automatically.
   * Also emits CloudWatch Embedded Metric Format (EMF) for auto-extracted metrics.
   */
  log(entry: AuditEntry): void {
    const timestamp = new Date().toISOString();

    // Structured audit log
    const auditRecord = {
      type: 'AUDIT',
      timestamp,
      platform: entry.platform,
      userId: entry.userId,
      userName: entry.userName,
      query: entry.query.substring(0, 200),
      responseLength: entry.responseLength,
      durationMs: entry.durationMs,
      success: entry.success,
      ...(entry.error ? { error: entry.error } : {}),
      ...(entry.sessionId ? { sessionId: entry.sessionId } : {}),
    };

    console.log(JSON.stringify(auditRecord));

    // CloudWatch Embedded Metric Format (EMF)
    // Metrics are auto-extracted by CloudWatch without needing the SDK.
    const metrics: Array<{ Name: string; Unit: string }> = [
      { Name: 'QueryCount', Unit: 'Count' },
      { Name: 'QueryDuration', Unit: 'Milliseconds' },
    ];

    const metricValues: Record<string, number> = {
      QueryCount: 1,
      QueryDuration: entry.durationMs,
    };

    if (!entry.success) {
      metrics.push({ Name: 'QueryErrors', Unit: 'Count' });
      metricValues.QueryErrors = 1;
    }

    const emfLog = {
      _aws: {
        Timestamp: Date.now(),
        CloudWatchMetrics: [
          {
            Namespace: 'OpsAgent',
            Dimensions: [['Platform']],
            Metrics: metrics,
          },
        ],
      },
      Platform: entry.platform,
      ...metricValues,
    };

    console.log(JSON.stringify(emfLog));
  }
}
