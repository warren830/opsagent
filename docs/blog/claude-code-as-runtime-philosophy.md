# 为什么我们选 Claude Code CLI 当 SRE Runtime —— 一个架构决策的底层思考

> **系列第一篇**。这篇不谈功能，谈一件更本质的事：**为什么是 "runtime" 而不是 "API"？** 这个词的选择决定了整个系统的形态。第二篇 [《把 Claude Code 当 SRE 用：4 个真实场景》](./claude-code-as-sre-runtime.md) 会用真实截图证明本文的每个论点。

---

## 先说结论

构建 AI 运维平台有三种主流范式：

| 范式 | 核心模式 | 典型产品 |
|------|---------|---------|
| **A. LLM as Chat Wrapper** | 用户打字 → LLM 生成建议文本 → 人类复制粘贴执行 | 大多数 ChatOps Bot |
| **B. LLM as Function Caller** | LLM 输出 JSON → 应用层解析 → 调用预定义 function | AWS DevOps Agent, 多数 Agentic 框架 |
| **C. LLM as Runtime** | LLM 作为持续运行的进程，直接持有工具句柄 | Loops（Claude Code CLI） |

**我们选 C，不是因为它新，而是因为前两种都有根本性缺陷。**

---

## 范式 A 的死穴：AI 不执行，只建议

Chat wrapper 的心智模型是 "AI 很聪明但不被信任"。它输出文本建议，人类决定是否执行。

这在表面上看起来安全，实际上是**把复杂度推给了人类**：

- AI 说 "你可以跑 `kubectl rollout undo deployment/payment-api`"
- 人类：这是对的集群吗？这个命令会不会误伤别的 pod？现在 rollout 到哪一步了？
- 人类：还是算了，我自己查吧

结果：**AI 变成了一个比文档稍强一点的搜索引擎**。凌晨 3 点告警响，你依然要爬起来。

核心问题：**建议没有上下文。** AI 不知道你当前的集群状态、不知道当前的流量分布、不知道你上周刚做的配置变更。它给的建议是"universal"的，而你的问题是"specific"的。

---

## 范式 B 的死穴：function 定义是一层有损压缩

Function calling 框架看起来很优雅：你定义一堆 functions（`list_pods`、`get_logs`、`query_metrics`），LLM 根据用户问题选择调用哪个。

这个模式在 demo 里非常漂亮。到了生产环境，问题暴露：

### 问题 1：function 列表永远不够

你以为 `get_logs(service, time_range)` 够用了，然后用户问："能不能只看 error 级别的日志？"。你加 `log_level` 参数。下一个问题："能不能关联这个 trace ID？"。你加 `trace_id` 参数。下下个问题……

你永远在追加 parameter，或者定义新的 function。**你不是在开发产品，你是在把 kubectl 和 curl 的能力重新实现一遍，用更糟糕的接口。**

### 问题 2：组合爆炸

真实的排查问题几乎永远需要工具串联：`kubectl get pods` → 发现异常 pod → `kubectl describe` → 看到 OOMKilled → 查内存指标 → 发现峰值 → 查最近的 deploy → 找到新版本引入的内存泄漏。

如果每一步都要 LLM 输出 JSON、应用层解析、再调下一个 function，你会遇到：
- **延迟叠加**：每次 function call 都是一次 LLM round-trip
- **上下文丢失**：中间结果要塞回 prompt，塞着塞着就 truncate 了
- **死循环**：LLM 反复调同一个 function，因为它忘了之前的结果

### 问题 3：你在重新发明 Unix

你定义的每一个 function——`list_files`、`grep_text`、`run_command`——操作系统已经提供了。你写的每一行 function 封装代码，都是在给一个早已存在的工具再加一层皮。

**Unix 哲学花了 50 年才演化出 grep、awk、jq 这样的工具。你用 function calling 重写它们是在干什么？**

---

## 范式 C：把 LLM 当 runtime

"Runtime" 这个词来自编程语言领域。Python runtime、JVM runtime——它们的共同特征是：**一个持续运行的进程，持有一组原语能力，代码在其中执行。**

我们把 Claude Code CLI 当成这样的 runtime：

- **它是一个 child process**，不是一次 HTTP 调用
- **它持有真实的工具句柄**——Bash、Read、Write、WebFetch，以及通过 MCP 接入的任何外部工具
- **它有状态**——session ID、工作目录、环境变量、之前调用的结果
- **代码在它里面执行**——Claude 决定调 `kubectl` 还是 `curl`，拼接什么参数，怎么解析输出

这个设计不是我们发明的。Anthropic 把 Claude Code 做成 CLI 就是这个意图——它本来是给 developer 写代码用的，但它的 runtime 特性让它天然适合做**任何需要工具调用 + 上下文保持 + 流式决策**的场景。

SRE 工作就是这样的场景。

---

## 关键差异：决策权在谁手里

这是三种范式最深层的区别：

```
范式 A：决策权在人类
  └─ AI 建议，人类选择、执行、验证

范式 B：决策权在应用层
  └─ LLM 挑 function，应用层执行，结果塞回 prompt
  └─ 应用层控制"能做什么"、"怎么做"、"什么时候做"

范式 C：决策权在 agent
  └─ Backend 注入上下文和工具清单
  └─ Agent 自主规划、执行、观察、再规划
  └─ Backend 只做审计和流式转发
```

在 Loops 的实现里，Backend 的 RCA handler 只做三件事：

1. 构造 system prompt（注入集群信息、telemetry endpoint、Runbook 索引）
2. Spawn 一个 `claude` child process
3. 把 stdout 的 stream-json 转成 SSE 推给前端

**Backend 不知道 agent 会调 kubectl 还是 curl。它也不关心。** Agent 自己决定。

这是一个哲学选择：**我们相信 agent 比我们更懂在具体场景下该用哪个工具**。我们的职责是把决策所需的上下文准备好，然后让开。

---

## Runtime 模式的三个硬核优势

### 1. 透明性——不是特性，是默认状态

当 LLM 是 runtime 时，每一次工具调用都必然经过 stdout，每一行输出都必然被转发。**透明不是我们额外实现的功能，是 runtime 模型的固有属性。**

对比一下 AWS DevOps Agent：你问它一个问题，它回答。中间调了什么 API、查了什么指标、基于什么证据得出结论？**黑盒。**

在 Loops 的 RCA 界面上，每一次 `kubectl get pods` 的输入输出都在右侧证据面板可见。不是我们刻意做的 "explainable AI"——是这个架构本来就长这样。

凌晨 3 点告警，AI 说 "我建议 rollback canary"。如果你看不到它是怎么得出这个结论的，你敢信吗？

### 2. 可扩展性——工具通过协议接入，不通过代码

MCP (Model Context Protocol) 是 Anthropic 2024 年推出的开放协议。它的核心思想是：**工具和 agent 解耦，通过标准协议通信。**

在 Loops 里，你可以：
- 加一个 Slack MCP server → Claude 会自动获得发送消息的能力
- 加一个 Jira MCP server → Claude 可以创建 ticket
- 加一个你公司内部的监控系统 MCP → Claude 可以查你的私有指标

**这些都不需要改 Loops 的代码**。只需要在 MCP 管理页面加一个 server 配置，Claude Code CLI 会在下次启动时读取这个配置，把对应的工具加入它的调用清单。

对比 function calling 模式：每加一个工具都要在应用层写代码、定义 schema、测试边界情况、发版。MCP 模式：写一个符合协议的 server，即插即用。

### 3. 认知完整性——多轮对话不是附加功能

人类 SRE 排查问题的认知过程是非线性的：查 A → 看到 B → 回头验证 C → 发现 D → 再去看 A 的另一个维度。

在 function calling 模式下，每次调用都是独立请求，中间结果要手动塞回 context。context window 塞满了就 truncate，于是 agent "忘了" 最早的发现。

Runtime 模式下，整个排查过程在**同一个 Claude session** 里完成。Claude 记得 5 分钟前看到 `payment-api-canary-7b4d9f` 这个 pod 名字，10 分钟后在 Loki 日志里看到同一个名字出现 OOM 错误，它会主动关联。

**这不是 "AI 记忆力好"。是 runtime 没有被人为切分。**

---

## 为什么 AWS 没有这么做？

AWS DevOps Agent 显然比 Loops 有更多资源投入。为什么它选了黑盒 frontier agent 模式，而不是暴露 runtime？

我的判断有三层：

**商业层面**：黑盒才能定价。如果你让用户看见每次 API 调用，他们会开始质疑为什么一次查询要 30 秒、为什么一个简单的 RCA 花了 $2。模糊是商业护城河。

**安全层面**：AWS 服务面向所有客户，包括完全不懂云的用户。给他们看 `kubectl` 原始输出可能造成混乱甚至 "为什么显示我的敏感信息" 的投诉。黑盒是默认安全的选择。

**战略层面**：AWS 不想让用户知道他们的 agent 在内部是什么模型、什么 prompt。一旦暴露，竞品可以复制；一旦开源化，用户会问 "为什么我不自己跑？"

Loops 做的是相反的决策：**我们假设用户是工程师，透明是资产不是负担。** 当你信任用户能看懂 `kubectl` 输出，你就可以给他们看。

---

## 一个具体例子：为什么 Backend 不直接调 AWS API

最初我们有过一个诱惑：Backend 层自己封装 `/api/kubectl/:cluster/pods`，`/api/aws/:account/s3/list` 这样的 REST endpoint，然后让 Claude 调这些 endpoint。

这看起来更 "架构干净"。为什么我们没这么做？

**因为那样 Claude 就不是 runtime 了，变成了 function caller。** 而我们的 Backend 会很快变成一个 "kubectl 和 aws cli 的 HTTP 皮肤"——不断追加 endpoint，不断加参数，永远追不上真实 CLI 的能力。

现在的设计是：Backend 通过环境变量把 AWS credential、kubeconfig 注入 Claude Code 的 child process，然后让 Claude 直接调 `aws` 和 `kubectl` CLI。

- Claude 可以用任何 `kubectl` flag，我们不需要提前枚举
- Claude 可以用 `jq` pipe 过滤结果，我们不需要实现 filter API
- Claude 可以用 `grep | awk | sort | uniq` 组合查询，我们不需要提供组合工具

**Unix 哲学 50 年前就解决的问题，不需要我们在 HTTP 层重新解决一遍。**

---

## 这意味着什么——SRE 这个职业的转变

把 LLM 当 runtime 不只是架构选择，它暗含了一个职业判断：

> **SRE 的核心能力，不是"记住命令"，是"理解系统"。**

当 AI agent 能自主执行工具链，SRE 不再需要：
- 记忆 `kubectl` 的 100 个子命令
- 在凌晨 3 点被叫醒去敲熟悉的命令
- 维护一堆 100 行 bash 的 Runbook

SRE 需要做的事：
- **定义系统上下文** —— 让 agent 知道你有哪些集群、哪些服务、哪些已知 quirk
- **审阅 agent 的证据链** —— 它调了什么？看到什么？推理路径是否合理？
- **处理 agent 处理不了的问题** —— 架构决策、容量规划、合规判断

这是把 SRE 从 "执行者" 升级到 "架构师 + 审计员"。AI agent 处理 80% 的机械排查，SRE 把时间花在那 20% 真正需要判断的问题上。

---

## 下一步我们在做什么

Runtime 模式给了我们一个很强的基础。接下来要解决的问题是：

**如何让 agent 从 "被动响应" 变成 "主动守护"？**

目前 Loops 还是用户驱动——告警来了点 RCA，用户问了才回答。下一步：

- **常驻巡检** — agent 周期性检查关键指标，发现异常主动创建 issue
- **Runbook 沉淀** — 每次 RCA 的调查链自动提取为可复用的 skill
- **多 agent 协作** — K8s agent、Cost agent、Security agent 之间互相调用对方的能力

这些都不需要重新设计架构——runtime 模式原生支持。我们只是在它上面加更多的 skill 和 MCP server。

---

## 写在最后

**"把 LLM 当 runtime" 这句话听起来像营销话术，其实是一个非常具体的技术决策。**

它决定了：你的 Backend 长什么样、你的工具扩展机制长什么样、你的审计链长什么样、你的用户凌晨 3 点敢不敢让 AI 动手。

Chat wrapper 做的是 "把 AI 塞进已有系统"。
Function caller 做的是 "用 AI 装饰已有 API"。
Runtime 模式做的是 "围绕 AI 的能力重新设计系统"。

这三者不是替代关系——它们适合不同的场景。但如果你要做的是**给 SRE 用的、能真正接管运维流程的 AI 平台**，我们的判断是：只有 runtime 模式能走通。

这一篇解释了为什么这么设计。下一篇用真实截图展示它长什么样、能做什么。如果你有兴趣深入看代码实现或者架构细节，源码在 GitHub。欢迎 challenge 我们的判断。

---

*系列文章：*
- *第一篇（本文）：为什么我们选 Claude Code CLI 当 SRE Runtime*
- *第二篇：[把 Claude Code 当 SRE 用：4 个真实场景](./claude-code-as-sre-runtime.md)*
- *第三篇（WIP）：MCP 生态——让 AI 运维平台真正可扩展*

*项目地址：[GitHub - Loops](https://github.com/loops-labs/loops) · License: MIT*
