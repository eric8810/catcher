# 跨平台 Rust AI SDK 调研与演进方案

> 调研日期：2026-05-24
> 前置条件：catcher 已有 Rust core + NAPI + Dart FFI + UniFFI 跨平台管线
> 核心动机：Vercel AI SDK 的业务逻辑（provider 抽象、streaming、tool calling、structured output）被 TypeScript 锁死在 JS 生态，无法复用到 Dart/Swift/Kotlin 等平台

---

## 一、问题定义

Vercel AI SDK 解决了 AI 应用开发中最繁琐的问题：多 provider 兼容。但它是纯 TypeScript 实现，意味着：

- **Dart/Flutter** 应用需要从零对接每家 provider
- **iOS/Android 原生**应用同样如此
- 每个平台各自维护一套 provider 适配逻辑，重复且容易 drift

如果用 Rust 实现核心业务层，通过 FFI 输出到各平台，就能**一次实现、处处复用**。

---

## 二、现有 Rust 生态调研

| 项目 | Stars | 定位 | 跨平台 FFI | 成熟度 |
|------|-------|------|-----------|--------|
| [aisdk.rs](https://github.com/lazy-hq/aisdk) | 低 | Vercel AI SDK 直接移植，73+ providers | 无，纯 Rust | 早期（2025年底发布） |
| [Rig](https://github.com/0xPlaygrounds/rig) | ~6.4K | 最流行的 Rust LLM 框架，20+ providers | 仅 WASM | 较成熟，已有生产用户 |
| [liter-llm](https://github.com/kreuzberg-dev/liter-llm) | 极新 | 142+ providers，14 种语言绑定 | 最激进的 FFI 方案 | 非常早期 |
| [genai](https://github.com/jeremychone/rust-genai) | ~750 | 轻量级多 provider 客户端 | 无 | 中等 |
| [langchain-rust](https://github.com/Abraxas-365/langchain-rust) | ~1.2K | LangChain 移植，不同范式 | 无 | 中等 |

**结论**：没有项目同时满足"完整 AI SDK 业务层 + 跨平台 FFI 输出"。最接近的 liter-llm 架构方向对但太新。aisdk.rs 业务层最接近但无 FFI。

---

## 三、与 catcher 的关系

新项目应独立于 catcher，但在网络层**依赖 catcher**：

```
ai-sdk-rs (新项目)
├── ai-core        — provider trait、streaming 抽象、tool calling、structured output
├── ai-openai      — OpenAI provider 实现
├── ai-anthropic   — Anthropic provider 实现
├── ai-google      — Google provider 实现
├── ai-napi        — Node.js 绑定 (NAPI-RS)
├── ai-dart-ffi    — Dart FFI 绑定
└── ai-uniffi      — Swift/Kotlin 绑定 (UniFFI)

依赖:
  ai-core  →  catcher-http (底层 HTTP 请求 + 韧性)
  ai-core  →  catcher-ws   (SSE/WS streaming)
```

复用 catcher 的价值：
- HTTP 请求栈（retry、circuit breaker、超时）不用重写
- SSE/WS 连接管理已经过生产验证
- NAPI / Dart FFI 的构建管线和 CI 矩阵可以直接照搬

---

## 四、Agent 驱动的自动化演进方案

### 核心理念

人工投入集中在初始阶段建立模式，之后由 agent 做增量演进，人只做 PR review。

### 信号源（两层跟踪）

| 层 | 来源 | 关注点 |
|----|------|--------|
| AI SDK 层 | Vercel AI SDK releases / commits | 新 provider 接入、API 变更、新能力（如 structured output 的 schema 变化） |
| Provider 层 | OpenAI / Anthropic / Google 的 API changelog、SDK releases | 新模型、新参数、breaking changes、新端点 |

AI SDK 层是主信号源（它已经做了兼容性的脏活），Provider 层做补充（覆盖 AI SDK 尚未跟进的变化）。

### 三层 Agent 架构

#### 第一层：Watcher（信号检测）

运行方式：GitHub Actions cron，每日一次。

职责：
- 拉取 vercel/ai 最近 24h 的 releases 和有意义的 commits
- 拉取各 provider SDK（openai-python、anthropic-sdk-python 等）的 releases
- 对比本地 tracking 文件（`tracking/versions.json`），过滤已处理的版本
- 有新变化时触发 Analyzer Agent

```
tracking/versions.json 示例:
{
  "vercel-ai-sdk": "5.2.0",
  "openai-api": "2026-05-01",
  "anthropic-sdk": "0.52.0",
  "google-genai": "1.5.0"
}
```

#### 第二层：Analyzer（变更分析）

运行方式：被 Watcher 触发，或手动触发。

职责：
- 读取变更内容（release notes、commit diff、API changelog）
- 对照 Rust 项目现状，判断影响面
- 创建结构化 GitHub Issue

Issue 输出格式：
```markdown
## Provider: anthropic
## Change Type: new_parameter
## Priority: medium
## Source: vercel/ai@5.2.1 + anthropic SDK 0.53.0

### 变更内容
Anthropic 新增 `thinking` 参数支持 extended thinking 模式

### AI SDK 参考实现
- 文件: packages/anthropic/src/anthropic-messages-language-model.ts
- 具体变更: [diff link]

### Rust 侧影响
- 需更新: ai-anthropic/src/config.rs (新增 thinking 字段)
- 需更新: ai-anthropic/src/request.rs (构建请求时传递参数)
- 需更新: ai-core/src/provider.rs (如果是通用能力)
```

#### 第三层：Implementer（代码实现）

运行方式：Issue 被打上 `agent-implement` 标签时触发。

职责：
- 读取 Issue 中的结构化 spec
- checkout 新分支
- 参考 AI SDK 的 TS 实现理解业务逻辑
- 生成/更新 Rust provider 代码
- 更新 NAPI / Dart FFI 绑定
- `cargo check` + `cargo test` + `cargo clippy`
- 开 PR 并关联 Issue

### 人工介入策略

| 变更类型 | 自动化程度 | 人工介入 |
|----------|-----------|---------|
| 新模型 ID | 全自动，auto-merge | 无 |
| 新参数（非 breaking） | 自动开 PR | review 后 merge |
| 新 provider | 自动生成脚手架 | 人工补充细节 + review |
| Breaking change | 自动创建 Issue + 影响分析 | 人工决策 + 实现 |
| 新能力（如 image generation） | 自动创建 Issue | 人工设计 trait + agent 辅助实现 |

---

## 五、Provider 规范层（关键设计）

为了让 agent 能可靠地生成代码，需要一个中间规范层，而不是让 agent 直接翻译 TypeScript。

```
provider-specs/
├── openai.toml
├── anthropic.toml
└── google.toml
```

规范文件示例：
```toml
[provider]
name = "openai"
base_url = "https://api.openai.com/v1"
auth = "bearer"

[capabilities]
text_generation = true
streaming = true
tool_calling = true
structured_output = true
image_generation = true
embeddings = true

[[models]]
id = "gpt-4o"
context_window = 128000
max_output = 16384
supports_vision = true
supports_tools = true

[endpoints.chat]
path = "/chat/completions"
method = "POST"
streaming_format = "sse"

[endpoints.chat.parameters]
temperature = { type = "f64", range = [0.0, 2.0], optional = true }
max_tokens = { type = "u32", optional = true }
top_p = { type = "f64", range = [0.0, 1.0], optional = true }
```

Agent 的工作流变为：
1. Analyzer 更新 spec 文件
2. Implementer 根据 spec 生成代码（有确定性的 spec → 代码映射）
3. 减少 agent 的"创造性"，提高可靠性

---

## 六、冷启动路径

### Phase 1：骨架 + OpenAI（人工主导，agent 辅助）

目标：建立项目结构、provider trait 设计、FFI 绑定模式。

- 定义 `ProviderModel` / `TextGenerationModel` / `StreamingModel` 等核心 trait
- 实现 OpenAI provider（最常用、文档最全）
- 搭建 NAPI + Dart FFI 绑定（参考 catcher 的模式）
- 建立测试策略：mock server + fixture 录制回放
- 编写 AGENTS.md（为后续 agent 提供编码规范）

### Phase 2：Anthropic + Google（agent 主导，人 review）

目标：验证模式可复制性。

- Agent 参考 OpenAI 的实现模式，生成 Anthropic 和 Google provider
- 人工 review PR，修正 agent 的错误，反馈到 AGENTS.md
- 迭代 provider spec 格式

### Phase 3：Watcher + Analyzer 接入

目标：开始自动化跟踪。

- 部署 GitHub Actions cron workflow
- Watcher 开始监控 Vercel AI SDK 和 provider releases
- Analyzer 开始自动创建结构化 Issue
- 人工处理 Issue（验证 agent 分析质量）

### Phase 4：Implementer 接入

目标：全自动演进。

- Issue → PR 的全自动管线
- 按变更类型配置 auto-merge 策略
- 人工只关注 breaking changes 和新能力设计

---

## 七、测试策略

| 层 | 方式 | 自动化 |
|----|------|--------|
| 类型检查 | `cargo check` + `cargo clippy` | CI |
| 单元测试 | mock server，fixture 录制回放 | CI |
| 集成测试 | 真实 API 调用（受限频率） | 定期 cron / release 前 |
| FFI 绑定测试 | NAPI + Dart FFI 端到端 | CI |
| 兼容性测试 | 对比 AI SDK TS 版的输出 | 可选，验证行为一致性 |

Fixture 录制回放是核心：首次手动跑真实 API 录制 request/response，后续 CI 回放。新模型上线时 agent 自动录制新 fixture。

---

## 八、待打磨的问题

- [ ] provider spec 格式的具体设计——toml 是否够用，还是需要更结构化的 DSL？
- [ ] streaming 的跨平台抽象——Rust 的 async Stream 如何映射到 Dart Stream / Node.js ReadableStream？
- [ ] tool calling 的 schema 定义——跨平台如何表达 JSON Schema？每个平台有自己的 schema 库
- [ ] agent 生成代码的质量保障——除了 CI 检查，是否需要 LLM-as-judge 审查？
- [ ] 优先支持哪些 provider？OpenAI + Anthropic + Google 作为 MVP 是否足够？
- [ ] catcher 依赖方式——git submodule、crates.io 发布、还是 monorepo？
- [ ] 项目命名和定位——是 catcher 生态的一部分，还是完全独立的项目？
