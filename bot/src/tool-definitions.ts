/**
 * Anthropic SDK tool definitions for OpsAgent.
 * Each tool maps to a function in tool-executor.ts.
 */
import type Anthropic from '@anthropic-ai/sdk';
import type { TenantConfig } from './tenant-loader';
import { ENRICHED_TOOLS } from './enriched-tools';

export interface ToolContext {
  hasGlossary: boolean;
  hasSkills: boolean;
  skillNames?: string[];
  enableEnrichedTools?: boolean;
}

const RUN_COMMAND: Anthropic.Tool = {
  name: 'run_command',
  description: `执行 shell 命令查询云资源。支持: aws cli, kubectl, aliyun cli, az, gcloud, 以及 ./scripts/ 下的脚本。
命令在安全沙箱中执行，仅允许白名单命令。kubectl 写操作（apply/delete/scale 等）需要管理员批准。
常用命令示例:
- aws ec2 describe-instances --region us-east-1
- kubectl --context prod-cluster get pods -A
- ./scripts/foreach-account.sh --accounts 111,222 aws s3 ls
- aliyun ecs DescribeInstances --RegionId cn-hangzhou`,
  input_schema: {
    type: 'object' as const,
    properties: {
      command: {
        type: 'string',
        description: '要执行的 shell 命令',
      },
      timeout_ms: {
        type: 'number',
        description: '命令超时时间（毫秒），默认 60000，最大 180000',
      },
    },
    required: ['command'],
  },
};

const READ_FILE: Anthropic.Tool = {
  name: 'read_file',
  description: `读取文件内容。用于查看 knowledge/ 目录中的知识库文件。返回带行号的文件内容。`,
  input_schema: {
    type: 'object' as const,
    properties: {
      path: {
        type: 'string',
        description: '文件路径（相对于工作目录），例如: knowledge/clusters.md',
      },
      offset: {
        type: 'number',
        description: '从第几行开始读（从 1 开始）',
      },
      limit: {
        type: 'number',
        description: '最多读多少行（默认 500，上限 2000）',
      },
    },
    required: ['path'],
  },
};

const SEARCH_FILES: Anthropic.Tool = {
  name: 'search_files',
  description: `搜索 knowledge/ 目录下的文件。支持按文件名模式和文件内容搜索。`,
  input_schema: {
    type: 'object' as const,
    properties: {
      pattern: {
        type: 'string',
        description: '文件名 glob 模式，如 "*.md" 或 "runbook-*.md"',
      },
      content_pattern: {
        type: 'string',
        description: '搜索文件内容的关键词或正则表达式',
      },
      directory: {
        type: 'string',
        description: '搜索根目录（相对于工作目录），默认 "knowledge"',
      },
    },
    required: [],
  },
};

const LOOKUP_GLOSSARY: Anthropic.Tool = {
  name: 'lookup_glossary',
  description: `查询公司术语表。当遇到不确定的缩写、项目代号或服务别名时使用。支持精确匹配、别名匹配和模糊搜索。`,
  input_schema: {
    type: 'object' as const,
    properties: {
      term: {
        type: 'string',
        description: '要查询的术语或缩写，例如 "ics" 或 "payment"',
      },
    },
    required: ['term'],
  },
};

const LOOKUP_SKILL: Anthropic.Tool = {
  name: 'lookup_skill',
  description: `获取技能指引（Skill/Runbook）的完整说明。当用户的问题匹配某个技能时调用以获取详细的操作步骤。`,
  input_schema: {
    type: 'object' as const,
    properties: {
      skill_name: {
        type: 'string',
        description: '技能名称',
      },
    },
    required: ['skill_name'],
  },
};

/**
 * Build the tool set based on context (tenant config, available data).
 */
export function buildToolSet(ctx: ToolContext): Anthropic.Tool[] {
  const tools: Anthropic.Tool[] = [
    RUN_COMMAND,
    READ_FILE,
    SEARCH_FILES,
  ];

  if (ctx.hasGlossary) {
    tools.push(LOOKUP_GLOSSARY);
  }

  if (ctx.hasSkills && ctx.skillNames?.length) {
    tools.push({
      ...LOOKUP_SKILL,
      description: LOOKUP_SKILL.description + `\n\n可用技能: ${ctx.skillNames.join(', ')}`,
    });
  }

  // Add enriched cloud tools when enabled
  if (ctx.enableEnrichedTools) {
    tools.push(...ENRICHED_TOOLS);
  }

  return tools;
}
