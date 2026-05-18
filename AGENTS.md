# AGENTS.md — 版本管理与发布规范

> 本文件记录 catcher 项目的版本管理、CI/CD、发版流程，供开发者和 AI agent 遵循。
> 代码编写规范详见 [RUST_STYLE_GUIDE.md](./RUST_STYLE_GUIDE.md)。

---

## 项目概况

catcher 是一个跨平台网络韧性库，包含以下包：

| 类型 | 包 | 注册表 | 路径 |
|------|-----|--------|------|
| TS types | `@eric8810/catcher-core` | npm | `packages/catcher-core-ts/` |
| TS HTTP | `@eric8810/catcher-http` | npm | `packages/catcher-http-ts/` |
| TS WS | `@eric8810/catcher-ws` | npm | `packages/catcher-ws-ts/` |
| TS Browser | `@eric8810/catcher-web` | npm | `packages/catcher-web/` |
| napi HTTP | `@eric8810/catcher-napi-http` | npm | `packages/catcher-napi-http/` |
| napi WS | `@eric8810/catcher-napi-ws` | npm | `packages/catcher-napi-ws/` |
| Rust core | `catcher-core` | crates.io | `packages/catcher-core/` |
| Rust HTTP | `catcher-http` | crates.io | `packages/catcher-http/` |
| Rust WS | `catcher-ws` | crates.io | `packages/catcher-ws/` |
| Rust FFI | `catcher-ffi` | crates.io | `packages/catcher-ffi/` |
| Rust UniFFI | `catcher-uniffi` | crates.io | `packages/catcher-uniffi/` |
| Dart FFI | `catcher_core` | pub.dev | `packages/catcher_core/` |

---

## 版本号规范

- 遵循 [SemVer](https://semver.org/)：`MAJOR.MINOR.PATCH`
- 所有包**统一版本号**，同一次发版所有包 bump 到相同版本
- 版本号出现在以下文件中（必须全部同步更新）：

### 文件清单

| 文件格式 | 文件 | 说明 |
|---------|------|------|
| `Cargo.toml` | 7 个 Rust crate | `version = "X.Y.Z"` + 依赖中的 `version = "X.Y.Z"` |
| `package.json` | 6 个 TS 包 | `"version": "X.Y.Z"` |
| `pubspec.yaml` | 1 个 Dart 包 | `version: X.Y.Z` |
| `.release-please-manifest.json` | 根目录 | 所有 11 个包的版本声明 |

### Rust `Cargo.toml` 版本位置

每个 Cargo.toml 中有两类 version 字段需更新：

1. **自身版本**：`version = "X.Y.Z"`（文件第一个 version 字段）
2. **依赖版本**：`catcher-core = { path = "...", version = "X.Y.Z" }` 等

依赖顺序（cargo publish 按此顺序）：
```
catcher-core → catcher-http / catcher-ws → catcher-ffi / catcher-uniffi / catcher-napi-http / catcher-napi-ws
```

---

## CI/CD 工作流

### 1. CI（持续集成）

**触发条件**：push 到 `master`/`main`，或 PR 到 `master`/`main`

**文件**：`.github/workflows/ci.yml`

**流程**：
1. `typecheck` — `pnpm typecheck`（所有 TS 包类型检查）
2. `test` — `pnpm build:ts && pnpm test`（构建 + TS 单元测试）
3. `rust-check` — `cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`（Rust 编译 + clippy lint + 测试）

### 2. Release Please（自动版本管理）

**触发条件**：push 到 `master`/`main`

**文件**：`.github/workflows/release-please.yml` + `release-please-config.json`

**流程**：
- 根据 conventional commits 自动创建 Release PR
- Release PR 合并后自动创建 GitHub Release + tag
- 支持多包：`node-workspace` 插件管理 TS 包，`rust` release-type 管理 Rust crate

### 3. Release（发布）

**触发条件**：推送 tag `v*` 或 `catcher_core-v*`

**文件**：`.github/workflows/release.yml`

**两个发布通道**：

#### 通道 A：`v*` tag → npm + crates.io

| Job | 发布目标 | 说明 |
|-----|---------|------|
| `publish-npm-ts` | npm (4 个 TS 包) | `pnpm publish --access public` |
| `publish-napi-http` | npm (napi-http 跨平台) | 8 个 target 构建 `.node` 文件 + 子包分发 |
| `publish-napi-ws` | npm (napi-ws 跨平台) | 8 个 target 构建 `.node` 文件 + 子包分发 |
| `publish-napi-assemble` | npm (napi 组装发布) | `napi create-npm-dir` + `napi prepublish`（发布子包）+ `npm publish`（主包） |
| `publish-rust` | crates.io (5 个 Rust crate) | 按依赖顺序 `cargo publish` |

#### 通道 B：`catcher_core-v*` tag → pub.dev

| Job | 发布目标 | 说明 |
|-----|---------|------|
| `publish-flutter` | pub.dev (`catcher_core`) | `dart pub publish --force`（OIDC 认证） |

### 必需的 GitHub Secrets

| Secret | 用途 |
|--------|------|
| `NPM_TOKEN` | npm 发布 token（Automation 类型） |
| `CARGO_REGISTRY_TOKEN` | crates.io API token |
| `GITHUB_TOKEN` | 自动提供，Release Please 使用 |
| （OIDC） | pub.dev 使用 GitHub OIDC，无需额外 token |

---

## 发版流程

### 方式一：手动发版（推荐用于紧急修复）

```bash
# 1. 确保工作目录干净
git status

# 2. 更新所有版本号（见上方文件清单）
#    - 7 个 Cargo.toml (自身 + 依赖引用)
#    - 6 个 package.json
#    - 1 个 pubspec.yaml
#    - 1 个 .release-please-manifest.json

# 3. 更新 CHANGELOG.md

# 4. 验证构建
cargo check --workspace --all-targets     # Rust 编译
pnpm build:ts                              # TS 构建
pnpm test                                  # TS 测试
cargo test --workspace                     # Rust 测试

# 5. 提交 + 打 tag
git add -A  # 或精确指定文件
git commit -m "release: v0.2.3 — 简要描述"
git tag -a v0.2.3 -m "v0.2.3 — 简要描述"

# 6. 推送（触发 Release workflow）
git push origin master
git push origin v0.2.3

# 7. Dart 包需要单独的 tag
git tag catcher_core-v0.2.3 -m "catcher_core v0.2.3"
git push origin catcher_core-v0.2.3
```

### 方式二：Release Please 自动发版（推荐用于常规迭代）

1. 开发时使用 [Conventional Commits](https://www.conventionalcommits.org/)：
   - `feat: 新增 SSE FFI 符号` → 自动 bump MINOR
   - `fix: 修复重连超时问题` → 自动 bump PATCH
   - `feat!: 重构 API 接口` → 自动 bump MAJOR
2. merge 到 master 后，Release Please 自动创建 Release PR
3. 审查 Release PR 中的 CHANGELOG 和版本号
4. 合并 Release PR → 自动创建 tag + GitHub Release
5. tag 推送触发 `release.yml` 发布到各注册表

### Commit Message 规范

```
feat: 新功能          → MINOR bump
fix: Bug 修复         → PATCH bump
feat!: / fix!: 破坏性变更 → MAJOR bump
docs: 文档更新        → 不 bump
test: 测试补充        → 不 bump
refactor: 代码重构    → 不 bump
ci: CI 配置更新       → 不 bump
style: 代码风格调整    → 不 bump
chore: 杂项           → 不 bump
```

---

## 发版前检查清单

- [ ] **版本号同步**：所有 Cargo.toml / package.json / pubspec.yaml / .release-please-manifest.json 版本号一致
- [ ] **Rust 依赖版本**：Cargo.toml 中 `catcher-core`/`catcher-http`/`catcher-ws` 的依赖 `version` 字段已更新
- [ ] **CHANGELOG.md** 已更新，包含本版本所有变更
- [ ] **编译验证**：`cargo check --workspace --all-targets` 零错误零警告
- [ ] **Clippy 零警告**：`cargo clippy --workspace --all-targets -- -D warnings`
- [ ] **Rust 测试**：`cargo test --workspace` 全部通过
- [ ] **重复代码审查**：无新增跨模块重复函数（参考 RUST_STYLE_GUIDE.md）
- [ ] **TS 构建**：`pnpm build:ts` 成功
- [ ] **TS 测试**：`pnpm test` 全部通过
- [ ] **E2E 测试**（可选）：`pnpm test:e2e` 全部通过
- [ ] **文档一致性**：docs/ 中的版本号引用已更新（如符号数量、文件路径等）
- [ ] **Git 状态干净**：无未提交变更
- [ ] **Tag 创建并推送**

---

## Rust 编码强制规则

> 编写或修改 Rust 代码时必须遵守。完整规范见 [RUST_STYLE_GUIDE.md](./RUST_STYLE_GUIDE.md)。

### 代码结构
- **禁止** `use xxx::*;` 通配符导入，必须显式列出每个导入项。
- `lib.rs` 仅含 `pub mod` + `pub use`；`mod.rs` 仅含 `pub mod` + `pub use`，不含逻辑。
- 禁止在多个 crate 中重复定义相同函数。公共 helper 归入 `catcher-core`。

### 类型定义
- 所有 Config 结构体必须遵循统一模板：`#[serde(default = "default_xxx")]` + 同名默认函数 + `impl Default`。
- `default_true()` / `default_false()` 须从 `catcher-core` 公共模块导入，禁止在各 crate 中重复定义。
- 需 JSON 序列化的 enum 统一使用 `#[serde(tag = "type")]`，禁止手动 `serde_json::json!` 构建。

### 注释
- 公共 API 文档注释 (`///` / `//!`) 统一使用**中文**。内部 `//` 行注释中英文均可，同文件保持一致。
- 区段分隔线统一使用 `// ──`，禁止 `// ═══`。
- **禁止**提交 Phase 追踪注释（`// Phase 3`）、需求编号注释（`// N-03`、`// A-01`）。

### 错误处理
- 所有可恢复错误使用 `catcher_core::CatcherError`。新增变体须同步更新 `category()` 方法。
- 非测试代码**禁止**无注释的 `unwrap()`。
- FFI 层 `CString::new()` 前必须 `replace('\0', "")` 去除 null 字节。

### 并发
- `std::sync::OnceLock` 初始化全局 `tokio::Runtime`，禁止使用其他 once-init 方案。
- `Atomic*` 的 `Ordering`：纯统计计数器 → `Relaxed`；状态机字段 → `AcqRel`。
- 禁止在 `.await` 期间持有 `std::sync::MutexGuard`。

### FFI
- 所有 FFI 函数入口必须检查指针是否为 null。
- `CString::into_raw()` 转移所有权后，必须在文档中标注调用方释放责任。
- **禁止** `Box::from_raw` + `mem::forget` 误用。直接解引用：`*(handle as *const usize)`。
- 读 body 必须检查 `body.is_null() || body_len == 0`。

### 测试
- 单元测试置于 `#[cfg(test)] mod tests { }`。
- 测试命名使用 `snake_case` 描述行为，**禁止**编号式命名（如 `test_14_retry_zero`）。

---

## 常见问题

### Q: npm publish 返回 404
**A**: 本地没有 npm 登录。通过 GitHub Actions 发布，不要本地手动 `pnpm publish`。

### Q: crates.io publish 失败 "already exists"
**A**: 该版本已发布。检查版本号是否正确 bump。crates.io 不允许覆盖已发布版本。

### Q: pub.dev publish 失败
**A**: pub.dev 使用 GitHub OIDC 认证，需要 `catcher_core-v*` 格式的 tag（注意前缀）。不是普通的 `v*` tag。

### Q: Release Please 没有自动创建 PR
**A**: 检查 commit message 是否符合 Conventional Commits 格式。`chore:` 类型的 commit 不会触发版本 bump。

### Q: napi 构建失败
**A**: napi 包需要 Rust toolchain + 目标平台 target。检查 `Cargo.toml` 中的 `crate-type = ["cdylib"]` 配置。跨平台构建矩阵（8 个）：linux-x64-gnu / linux-x64-musl / linux-arm64-gnu / linux-arm64-musl / darwin-x64 / darwin-arm64 / win32-x64-msvc / win32-arm64-msvc。ARM64 Linux 使用 zig 交叉编译。

### Q: Cargo.lock 冲突
**A**: 版本 bump 后需在 `packages/` 目录下运行 `cargo check` 或 `cargo generate-lockfile` 更新 Cargo.lock，然后提交。

### Q: clippy 报重复代码警告
**A**: 检查是否在多个 crate 中定义了相同函数。公共 helper 应抽取到 `catcher-core`。参考 RUST_STYLE_GUIDE.md。
