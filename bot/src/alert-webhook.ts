/**
 * Alert Webhook parser for OpsAgent.
 * Normalizes incoming alerts from CloudWatch, Datadog, and generic webhooks
 * into a standard AlertPayload format for issue creation + RCA triggering.
 */

export interface AlertPayload {
  source: 'cloudwatch' | 'datadog' | 'webhook';
  title: string;
  severity: 'critical' | 'high' | 'medium' | 'low';
  description: string;
  resource_id: string;
  resource_type?: string;
  metric_data?: Record<string, any>;
  raw: any;
}

// ── CloudWatch SNS ────────────────────────────────────────────

export function parseCloudWatchAlert(payload: any): AlertPayload | null {
  if (!payload || payload.Type !== 'Notification') return null;

  let message: any;
  try {
    message = typeof payload.Message === 'string' ? JSON.parse(payload.Message) : payload.Message;
  } catch {
    return null;
  }

  if (!message?.AlarmName || message.NewStateValue !== 'ALARM') return null;

  // Extract resource ID from dimensions
  let resourceId = '';
  const dims = message.Trigger?.Dimensions || [];
  for (const d of dims) {
    if (d.value || d.Value) {
      resourceId = d.value || d.Value;
      break;
    }
  }

  return {
    source: 'cloudwatch',
    title: message.AlarmName,
    severity: 'high', // CloudWatch alarms default to high
    description: message.NewStateReason || '',
    resource_id: resourceId,
    resource_type: inferResourceType(message.Trigger?.Namespace),
    metric_data: {
      metric_name: message.Trigger?.MetricName,
      namespace: message.Trigger?.Namespace,
    },
    raw: message,
  };
}

function inferResourceType(namespace?: string): string | undefined {
  if (!namespace) return undefined;
  const map: Record<string, string> = {
    'AWS/EC2': 'EC2',
    'AWS/RDS': 'RDS',
    'AWS/Lambda': 'Lambda',
    'AWS/ECS': 'ECS',
    'AWS/EKS': 'EKS',
    'AWS/ELB': 'ELB',
    'AWS/NATGateway': 'NATGateway',
    'AWS/DynamoDB': 'DynamoDB',
    'AWS/S3': 'S3',
    'AWS/ElastiCache': 'ElastiCache',
  };
  return map[namespace];
}

// ── Datadog ───────────────────────────────────────────────────

export function parseDatadogAlert(payload: any): AlertPayload | null {
  if (!payload?.title) return null;
  // Presence of alert_type indicates Datadog format
  if (!payload.alert_type && !payload.priority) return null;

  const severity = mapDatadogSeverity(payload.priority, payload.alert_type);

  // Try to extract resource ID from tags
  let resourceId = '';
  if (payload.tags) {
    const tags = typeof payload.tags === 'string' ? payload.tags.split(',') : payload.tags;
    for (const tag of tags) {
      const [key, val] = String(tag).split(':');
      if (['instance', 'host', 'resource_id', 'pod'].includes(key?.trim())) {
        resourceId = val?.trim() || '';
        break;
      }
    }
  }

  return {
    source: 'datadog',
    title: payload.title,
    severity,
    description: payload.body || payload.text || '',
    resource_id: resourceId,
    metric_data: { alert_type: payload.alert_type, priority: payload.priority },
    raw: payload,
  };
}

function mapDatadogSeverity(priority?: string, alertType?: string): 'critical' | 'high' | 'medium' | 'low' {
  if (priority === 'P1') return 'critical';
  if (priority === 'P2') return 'high';
  if (priority === 'P3') return 'medium';
  if (priority === 'P4') return 'low';
  if (alertType === 'error') return 'critical';
  if (alertType === 'warning') return 'high';
  return 'medium';
}

// ── Generic Webhook ───────────────────────────────────────────

export function parseGenericAlert(payload: any): AlertPayload | null {
  if (!payload?.title) return null;

  return {
    source: 'webhook',
    title: payload.title,
    severity: payload.severity || 'medium',
    description: payload.description || payload.body || '',
    resource_id: payload.resource_id || payload.host || '',
    resource_type: payload.resource_type,
    metric_data: payload.metric_data || payload.data,
    raw: payload,
  };
}

// ── Auto-Detect & Normalize ───────────────────────────────────

/**
 * Auto-detect alert format and normalize to AlertPayload.
 * Tries: CloudWatch SNS → Datadog → Generic → null
 */
export function normalizeAlert(payload: any): AlertPayload | null {
  if (!payload) return null;

  // CloudWatch SNS format
  if (payload.Type === 'Notification' && payload.Message) {
    return parseCloudWatchAlert(payload);
  }

  // Datadog format (has alert_type)
  if (payload.alert_type) {
    return parseDatadogAlert(payload);
  }

  // Generic format (has title)
  if (payload.title) {
    return parseGenericAlert(payload);
  }

  return null;
}
