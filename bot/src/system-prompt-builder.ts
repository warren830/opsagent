/**
 * Builds the system prompt for the Anthropic SDK agent loop.
 * Extracted from the old generateClaudeMd() — returns a string instead of writing a file.
 */
import * as fs from 'fs';
import * as path from 'path';
import { loadGlossary, generateGlossaryKnowledge } from './glossary-loader';
import { loadAccounts, generateAccountsKnowledge, generateAlicloudPromptSection } from './accounts-loader';
import { loadSkills } from './skills-loader';
import { scanKnowledgeFiles, generateKnowledgeIndex } from './knowledge-loader';
import { TenantConfig } from './tenant-loader';

const BASE_PROMPT = `你是 OpsAgent，一个多云基础设施查询助手。
支持 AWS、阿里云、Azure、GCP 等多云平台。
使用 run_command 工具执行 aws cli、kubectl、aliyun cli 等命令查询资源。
knowledge/ 目录包含公司的知识库文件。遇到基础设施问题时，先用 search_files 或 read_file 工具查阅知识库。
当用户提到不确定的缩写或项目代号时，使用 lookup_glossary 工具查询术语表。
当需要排障操作指引时，使用 lookup_skill 工具获取技能指引。
始终返回结构化、易读的 Markdown 表格结果。

## CRITICAL: kubectl 使用规则
kubeconfig 已预配置，可直接使用 kubectl。按以下步骤操作：
1. 先用 read_file 工具读取 knowledge/clusters.md 获取所有集群信息和 context 名称
2. 直接用 --context 参数执行 kubectl 命令：
   run_command: kubectl --context <context-name> get pods -A
3. 汇总所有集群的结果到一张表中，表中必须包含「集群」列

重要：除非用户明确指定了某个集群，否则必须查询所有集群并汇总结果。

## kubectl 写操作
kubectl 的读操作（get/describe/logs/top）可直接执行。
写操作（apply/delete/scale/exec 等）需要管理员批准，系统会自动提示。`;

export interface SystemPromptConfig {
  workDir: string;
  glossaryConfigPath: string;
  accountsConfigPath: string;
  skillsConfigPath: string;
  knowledgeDir: string;
  tenantId?: string;
  tenant?: TenantConfig | null;
}

/**
 * Build the system prompt string for a specific query context.
 */
export function buildSystemPrompt(config: SystemPromptConfig): string {
  const parts: string[] = [BASE_PROMPT];

  // Determine tenant-scoped paths
  const tenantDir = config.tenantId
    ? path.join(config.knowledgeDir, '_tenants', config.tenantId)
    : null;
  const tenantConfigDir = tenantDir ? path.join(tenantDir, '_config') : null;
  const glossaryPath = tenantConfigDir && fs.existsSync(path.join(tenantConfigDir, 'glossary.yaml'))
    ? path.join(tenantConfigDir, 'glossary.yaml')
    : config.glossaryConfigPath;
  const knowledgeDir = tenantDir && fs.existsSync(tenantDir) ? tenantDir : config.knowledgeDir;

  // Glossary summary (brief — full details via lookup_glossary tool)
  const glossaryConfig = loadGlossary(glossaryPath);
  if (glossaryConfig?.glossary) {
    const keys = Object.keys(glossaryConfig.glossary);
    if (keys.length > 0) {
      const preview = keys.slice(0, 20).map(k => {
        const e = glossaryConfig.glossary[k];
        return `- **${k}**: ${e.full_name || ''}`;
      }).join('\n');
      parts.push('');
      parts.push('## 公司术语速查');
      parts.push(preview);
      if (keys.length > 20) parts.push(`...共 ${keys.length} 条术语。使用 lookup_glossary 工具查询完整信息。`);
    }
  }

  // Accounts summary (filtered by tenant)
  const accounts = loadAccounts(config.accountsConfigPath);
  if (accounts) {
    const tenant = config.tenant;
    if (tenant?.aws_account_ids) {
      const allowedIds = new Set(tenant.aws_account_ids);
      if (accounts.accounts.extra) {
        accounts.accounts.extra = accounts.accounts.extra.filter(a => allowedIds.has(a.id));
      }
      if (accounts.accounts.overrides) {
        const filtered: Record<string, any> = {};
        for (const [id, override] of Object.entries(accounts.accounts.overrides)) {
          if (allowedIds.has(id)) filtered[id] = override;
        }
        accounts.accounts.overrides = filtered;
      }
    }
    const summary = generateAccountsKnowledge(accounts, knowledgeDir);
    if (summary) {
      parts.push('');
      parts.push('## 已配置的云账号');
      parts.push(summary);
    }
  }

  // Alicloud section
  if (config.tenant?.alicloud && config.tenant.alicloud.length > 0) {
    const aliSection = generateAlicloudPromptSection(config.tenant.alicloud);
    if (aliSection) {
      parts.push('');
      parts.push(aliSection);
    }
  }

  // Tenant scope
  if (config.tenant) {
    parts.push('');
    parts.push(`## 当前租户: ${config.tenant.name} (${config.tenant.id})`);
    if (config.tenant.aws_account_ids?.length) {
      parts.push(`仅查询以下 AWS 账号: ${config.tenant.aws_account_ids.join(', ')}`);
      parts.push(`使用 foreach-account.sh 时必须加 --accounts ${config.tenant.aws_account_ids.join(',')} 参数限制范围。`);
    }
  }

  // Knowledge index (brief — content via read_file tool)
  const knowledgeEntries = scanKnowledgeFiles(knowledgeDir);
  if (knowledgeEntries.length > 0) {
    const index = generateKnowledgeIndex(knowledgeEntries);
    if (index) {
      parts.push('');
      parts.push('## 知识库索引');
      parts.push('以下文件可通过 read_file 工具查看:');
      parts.push(index);
    }
  }

  return parts.join('\n');
}
