# OpsAgent Test Cases

## 测试环境

### EKS 集群

| 集群 | 版本 | 业务场景 | Nodes |
|------|------|----------|-------|
| `opsagent-test` | 1.31 | 通用测试平台 | 3 (2x t3.medium + 1x t3.large) |
| `ecommerce-prod` | 1.30 | 电商（前后端、支付、PCI） | 2x t3.medium |
| `data-platform` | 1.31 | 数据（Kafka、Flink、ML、Airflow） | 2x t3.large |
| `legacy-migration` | 1.29 | 银行核心迁移（Oracle、大机） | 2x t3.medium |

### Glossary 术语（9 个）

`ics`, `falcon`, `cbs`, `datalake`, `aml`, `storefront`, `pci-zone`, `fraud-engine`

### Knowledge Base 文件

- `ops-agent.md` — 通用查询指南
- `runbook-ecommerce.md` — 电商平台运维手册
- `runbook-data-platform.md` — 数据平台运维手册
- `runbook-legacy-migration.md` — 银行迁移运维手册
- `incident-history.md` — 历史事故记录

---

## 一、Glossary 术语解析（测试 glossary 是否注入 system prompt）

| # | 问题 | 期望行为 | 验证点 |
|---|------|----------|--------|
| 1 | "CBS 是什么系统？" | 回答 Core Banking System 的描述 | glossary 中 `cbs` 术语被识别 |
| 2 | "数据平台部署在哪个集群？" | 回答 data-platform 集群 | `datalake` 的 alias "数据平台" 被识别 |
| 3 | "商城前端用了哪些服务？" | 列出 web-storefront, mobile-bff | `storefront` 的 alias "商城" 被识别 |
| 4 | "反洗钱系统的告警阈值是多少？" | 查 aml-scanner env 或引用 knowledge，回答 0.75 | `aml` alias "反洗钱" + knowledge base 结合 |
| 5 | "PCI 区域有哪些网络限制？" | 描述 payment namespace 的 NetworkPolicy | `pci-zone` 术语 → payment namespace → 查 networkpolicy |
| 6 | "风控引擎用的什么 ML 模型？" | 回答 fraud-detection v1.8.0 | `fraud-engine` alias "风控引擎" → ml-serving namespace |

## 二、Knowledge Base 知识检索（测试 Grep/Read knowledge/ 文件）

| # | 问题 | 期望行为 | 验证点 |
|---|------|----------|--------|
| 7 | "product-catalog 连的是哪个数据库？" | 回答 catalog-prod.cluster-abc123... Aurora PostgreSQL | 查 runbook-ecommerce.md 的 Database Connections 表 |
| 8 | "Flink 的 checkpoint 存在哪里？" | 回答 s3://data-lake-checkpoints/flink | 查 runbook-data-platform.md |
| 9 | "大机连接超时应该怎么排查？" | 给出 mainframe-gateway 排查步骤 | 查 runbook-legacy-migration.md 的 Troubleshooting |
| 10 | "上次支付故障是什么原因？" | 回答 INC-2026-001 Stripe API key 轮换未同步 | 查 incident-history.md |
| 11 | "Oracle 连接池之前出过什么问题？" | 回答 INC-2026-003 pool 从 10 调到 20 | 查 incident-history.md |
| 12 | "数据平台的 ETL 每天几点跑？" | 回答 03:30 UTC daily-etl-aggregate | 查 runbook-data-platform.md Daily Jobs 表 |

## 三、EKS 集群查询（测试 aws cli + kubectl 实际执行）

| # | 问题 | 期望行为 | 验证点 |
|---|------|----------|--------|
| 13 | "列出所有 EKS 集群和版本" | 表格：4 个集群 + 版本号 | `aws eks list-clusters` + describe |
| 14 | "哪些集群版本低于 1.31？" | ecommerce-prod (1.30), legacy-migration (1.29) | 版本比较逻辑 |
| 15 | "legacy-migration 集群有多少个节点？" | 2 个 t3.medium | `kubectl get nodes` on correct context |
| 16 | "所有集群一共有多少个 pod？Running 和 Pending 分别多少？" | ~66 pods, 分状态统计 | 跨集群 kubectl 遍历 |

## 四、K8s 工作负载查询（测试 kubectl 深度查询）

| # | 问题 | 期望行为 | 验证点 |
|---|------|----------|--------|
| 17 | "ecommerce-prod 的 production 和 staging 版本有什么差异？" | 对比 web-storefront v4.2.1 vs v4.3.0-rc2 | 跨 namespace 对比 |
| 18 | "哪些 deployment 的镜像版本是 nginx:1.21（比较旧）？" | payment-service, loan-service, mainframe-gateway | 跨集群 image 扫描 |
| 19 | "哪些 pod 是 Pending 状态？为什么？" | 列出 Pending pods + describe 原因（资源不足） | `kubectl describe pod` |
| 20 | "data-platform 有哪些 CronJob？下次运行时间？" | daily-etl-aggregate, model-retrain-weekly | `kubectl get cronjobs` |
| 21 | "banking-api-gw 的 mTLS 是否开启？" | 回答 MTLS_REQUIRED=true | 查 env var |
| 22 | "transaction-service 的双写模式开着吗？写到哪两个数据库？" | DUAL_WRITE=true, Aurora + Oracle | 查 env + configmap + knowledge |

## 五、Glossary + Knowledge + Live Query 组合（测试三者联动）

| # | 问题 | 期望行为 | 验证点 |
|---|------|----------|--------|
| 23 | "CBS 的迁移进度怎么样？哪些服务还没开始？" | glossary 解析 CBS → knowledge 查 migration-status configmap + runbook | 三层联动：glossary → knowledge → kubectl |
| 24 | "上次数据平台出故障是什么问题？Flink 现在的内存配置改了吗？" | incident-history → INC-2026-002 OOM → 实际查 flink-taskmanager env 确认 4GB | knowledge + live query 对比验证 |
| 25 | "商城支付失败应该怎么排查？当前 payment-gateway 状态正常吗？" | runbook 排查步骤 + 实际查 pod 状态 | knowledge 指导 + 实时验证 |

## 六、Admin API / 配置管理（通过 Admin UI 或 curl 测试）

| # | 方式 | 操作 | 验证点 |
|---|------|------|--------|
| 26 | GET | 读取 glossary: `GET /admin/api/glossary` | 返回 9 个术语 |
| 27 | PUT | 新增术语: `PUT /admin/api/glossary` 加 "ops-team" | 术语可动态添加 |
| 28 | PUT | 上传知识库文件: `PUT /admin/api/knowledge/sla-policy.md` | 文件可通过 API 管理 |
| 29 | GET | 列出知识库: `GET /admin/api/knowledge` | 返回 5+ 个文件 |
| 30 | DELETE | 删除知识库文件: `DELETE /admin/api/knowledge/sla-policy.md` | 文件可删除 |

---

## Admin API 验证命令

```bash
ALB="opsagent-alb-473647280.us-east-1.elb.amazonaws.com"

# 26. 读 glossary
curl -s http://$ALB/admin/api/glossary | jq '.glossary | keys'

# 27. 加术语
curl -s -X PUT http://$ALB/admin/api/glossary \
  -H 'Content-Type: application/json' \
  -d '{"glossary":{"ops-team":{"full_name":"Operations Team","description":"运维团队"}}}'

# 28. 上传知识库
curl -s -X PUT http://$ALB/admin/api/knowledge/sla-policy.md \
  -H 'Content-Type: application/json' \
  -d '{"content":"# SLA Policy\n\n- P1: 15 min response\n- P2: 1 hour response"}'

# 29. 列出文件
curl -s http://$ALB/admin/api/knowledge | jq '.files[].name'

# 30. 删除
curl -s -X DELETE http://$ALB/admin/api/knowledge/sla-policy.md
```

## 推荐测试顺序

1. **先测 Admin API（26-30）** — 确认配置管理正常
2. **再测术语解析（1-6）** — 确认 glossary 注入 system prompt
3. **然后测知识检索（7-12）** — 确认 knowledge base 可被搜索
4. **接着测实时查询（13-22）** — 确认 aws/kubectl 执行能力
5. **最后测组合场景（23-25）** — 这是最能体现价值的 demo 场景

## 测试结果记录

| # | 通过 | 备注 |
|---|------|------|
| 1 | | |
| 2 | | |
| 3 | | |
| ... | | |
| 30 | | |
