/**
 * Command security sandbox for OpsAgent tool execution.
 * Validates shell commands against an allowlist before execution.
 */

export interface SandboxConfig {
  allowedAwsAccountIds?: string[];
  kubectlReadOnly?: boolean; // default true
}

export interface SandboxResult {
  allowed: boolean;
  reason?: string;
  needsApproval?: boolean; // kubectl write ops need admin approval
}

// Commands that are always allowed (read-only operations)
const ALLOWED_PREFIXES = [
  'aws ', 'kubectl ', 'aliyun ', 'az ', 'gcloud ',
  'jq ', 'grep ', 'cat ', 'head ', 'tail ', 'sort ', 'uniq ',
  'wc ', 'awk ', 'date ', 'echo ', 'printf ', 'which ', 'env ',
  'curl ', 'dig ', 'nslookup ', 'ping ',
  './scripts/',
];

// kubectl subcommands that are read-only
const KUBECTL_READONLY = [
  'get', 'describe', 'logs', 'top', 'explain', 'api-resources',
  'api-versions', 'cluster-info', 'config view', 'version',
  '--context', '-n', '--namespace', '--all-namespaces', '-A',
];

// kubectl subcommands that require approval
const KUBECTL_WRITE = [
  'apply', 'delete', 'edit', 'patch', 'create', 'replace',
  'scale', 'rollout', 'exec', 'run', 'expose', 'label',
  'annotate', 'taint', 'drain', 'cordon', 'uncordon',
];

// Patterns that are always denied
const DENY_PATTERNS: Array<{ pattern: RegExp; reason: string }> = [
  { pattern: /\bgit\b/, reason: 'git operations are not allowed' },
  { pattern: /\brm\s+(-[rRf]+\s+|--recursive)/, reason: 'recursive delete is not allowed' },
  { pattern: /\beval\b/, reason: 'eval is not allowed' },
  { pattern: /\bsudo\b/, reason: 'sudo is not allowed' },
  { pattern: /\bchmod\b/, reason: 'chmod is not allowed' },
  { pattern: /\bchown\b/, reason: 'chown is not allowed' },
  { pattern: /\bdd\b\s/, reason: 'dd is not allowed' },
  { pattern: /\bmkfs\b/, reason: 'mkfs is not allowed' },
  { pattern: /\bfdisk\b/, reason: 'fdisk is not allowed' },
  { pattern: /\bshutdown\b|\breboot\b/, reason: 'system commands are not allowed' },
  { pattern: /\bnc\b\s+-l/, reason: 'netcat listen is not allowed' },
  { pattern: />\s*\/(?!tmp\/)/, reason: 'redirect to non-tmp paths is not allowed' },
  { pattern: /\|\s*(bash|sh|zsh)\b/, reason: 'piping to shell is not allowed' },
  { pattern: /\b(bash|sh|zsh)\s+-c\b/, reason: 'shell -c execution is not allowed' },
];

/**
 * Validate a command against the sandbox rules.
 */
export function validateCommand(command: string, config: SandboxConfig = {}): SandboxResult {
  const trimmed = command.trim();
  if (!trimmed) return { allowed: false, reason: 'empty command' };

  // Check deny patterns first
  for (const { pattern, reason } of DENY_PATTERNS) {
    if (pattern.test(trimmed)) {
      return { allowed: false, reason };
    }
  }

  // Check allowlist
  const isAllowed = ALLOWED_PREFIXES.some(prefix => trimmed.startsWith(prefix));
  if (!isAllowed) {
    return { allowed: false, reason: `command not in allowlist: ${trimmed.split(' ')[0]}` };
  }

  // kubectl write approval check
  if (trimmed.startsWith('kubectl ')) {
    const kubectlReadOnly = config.kubectlReadOnly !== false; // default true
    if (kubectlReadOnly) {
      const isWrite = KUBECTL_WRITE.some(sub => {
        // Match: kubectl <sub>, kubectl --context xxx <sub>, kubectl -n ns <sub>
        const pattern = new RegExp(`\\bkubectl\\b.*\\b${sub}\\b`);
        return pattern.test(trimmed);
      });
      if (isWrite) {
        return {
          allowed: false,
          needsApproval: true,
          reason: `kubectl write operation requires admin approval: ${trimmed.substring(0, 80)}`,
        };
      }
    }
  }

  // AWS account isolation: enforce --accounts on foreach-account.sh
  if (trimmed.includes('foreach-account.sh') && config.allowedAwsAccountIds?.length) {
    if (!trimmed.includes('--accounts')) {
      return {
        allowed: false,
        reason: `foreach-account.sh must include --accounts ${config.allowedAwsAccountIds.join(',')} for tenant isolation`,
      };
    }
    // Verify the accounts in the command match the allowed list
    const match = trimmed.match(/--accounts\s+([\d,]+)/);
    if (match) {
      const requested = match[1].split(',').map(s => s.trim());
      const unauthorized = requested.filter(id => !config.allowedAwsAccountIds!.includes(id));
      if (unauthorized.length > 0) {
        return {
          allowed: false,
          reason: `unauthorized AWS account(s): ${unauthorized.join(', ')}`,
        };
      }
    }
  }

  // AWS STS assume-role isolation
  if (trimmed.includes('sts assume-role') && config.allowedAwsAccountIds?.length) {
    const roleArnMatch = trimmed.match(/--role-arn\s+arn:aws:iam::(\d+):/);
    if (roleArnMatch) {
      const accountId = roleArnMatch[1];
      if (!config.allowedAwsAccountIds.includes(accountId)) {
        return {
          allowed: false,
          reason: `assume-role to unauthorized account: ${accountId}`,
        };
      }
    }
  }

  return { allowed: true };
}
