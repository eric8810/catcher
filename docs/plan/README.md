# Development Plan

> 基于 `docs/arch-rs/` 架构文档的详细开发方案

## 结构

| 文件 | 内容 |
|------|------|
| `00-overview.md` | 总览：5 阶段策略、测试复用、工作量估算 |
| `01-scaffold.md` | 项目脚手架：`cargo init`、CI 流水线、目录创建 |
| `02-phase1-types-codec.md` | Phase 1：error / config / types / codec |
| `03-phase2-transport.md` | Phase 2：HTTP transport + WS transport + TLS + DNS |
| `04-phase3-resilience.md` | Phase 3：retry + circuit breaker + backoff + timeout |
| `05-phase4-scheduler-observability.md` | Phase 4：priority queue + concurrency + network quality |
| `06-phase5-ffi.md` | Phase 5：C ABI + napi-rs + flutter_rust_bridge |
| `07-test-reuse.md` | TS e2e 测试复用方案（S1-S8 + chaos） |
| `08-release.md` | 对外发包 Release 方案 — CI/CD, release-please, publish 流程 |

## 与 arch-rs 的关系

- `arch-rs/` = **设计文档**（What + Why）
- `plan/` = **开发方案**（How + When）

每个 phase 文档引用对应的 arch 文档章节，指明具体的实现步骤、文件创建顺序、测试编写要点。
