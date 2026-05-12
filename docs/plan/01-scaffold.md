# 01 — 项目脚手架与 CI

> 目标：创建 `packages/catcher-rs/` 目录、Cargo project、CI 流水线、开发环境

---

## 1. 初始化 Cargo 项目

### 1.1 创建目录结构

```bash
cd packages/
cargo init --lib --name catcher-rs catcher-rs
```

编辑 `packages/catcher-rs/Cargo.toml`，填入 `docs/arch-rs/01-cargo.md` 中的完整依赖清单。

### 1.2 创建源码目录树

完全按照 `docs/arch-rs/02-module-tree.md` 创建：

```bash
cd packages/catcher-rs/src/
mkdir -p types transport resilience ws scheduler codec observability ffi
touch lib.rs error.rs config.rs
touch types/mod.rs types/http.rs types/ws.rs types/resilience.rs types/scheduler.rs types/observability.rs
touch transport/mod.rs transport/http_client.rs transport/ws_client.rs transport/tls.rs transport/dns.rs
touch resilience/mod.rs resilience/retry.rs resilience/circuit_breaker.rs resilience/backoff.rs resilience/timeout.rs
touch ws/mod.rs ws/reconnect.rs ws/heartbeat.rs ws/multi_endpoint.rs ws/compression.rs
touch scheduler/mod.rs scheduler/priority_queue.rs scheduler/concurrency.rs
touch codec/mod.rs codec/msgpack.rs
touch observability/mod.rs observability/network_quality.rs observability/metrics.rs
touch ffi/mod.rs ffi/http_ffi.rs ffi/ws_ffi.rs ffi/codec_ffi.rs ffi/quality_ffi.rs ffi/types_ffi.rs
```

最终结构应与 `arch-rs/02-module-tree.md` 一致（35 个源文件）。

### 1.3 创建测试目录

```bash
cd packages/catcher-rs/
mkdir -p tests/transport tests/resilience tests/ws tests/scheduler tests/codec tests/integration
```

### 1.4 验证初始化

```bash
cd packages/catcher-rs/
cargo check        # 依赖下载 + 编译检查
cargo test         # 运行空测试（确认框架就绪）
```

---

## 2. feature flags

根据 `arch-rs/01-cargo.md`，定义以下 features：

```toml
[features]
default = ["rustls"]
napi = ["napi", "napi-derive", "tokio/rt-multi-thread"]
flutter = []
hickory-dns = ["hickory-resolver"]
rustls = ["reqwest/rustls-tls", "tokio-tungstenite/rustls-tls-native-roots"]
native-tls = ["reqwest/native-tls", "tokio-tungstenite/native-tls"]
```

| feature | 作用 |
|---------|------|
| `napi` | 启用 napi-rs 绑定，多线程 tokio runtime |
| `flutter` | 为 flutter_rust_bridge codegen 预留 |
| `hickory-dns` | 进程内 DNS 缓存（生产推荐） |
| `rustls` | 纯 Rust TLS（默认，跨平台一致） |
| `native-tls` | 系统原生 TLS（可选替代） |

---

## 3. CI 配置

### 3.1 GitHub Actions 文件

创建 `.github/workflows/catcher-rs.yml`：

```yaml
name: catcher-rs

on:
  push:
    paths: ['packages/catcher-rs/**', '.github/workflows/catcher-rs.yml']
  pull_request:
    paths: ['packages/catcher-rs/**']

env:
  CARGO_TERM_COLOR: always

jobs:
  test:
    name: Test (${{ matrix.os }})
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
    defaults:
      run:
        working-directory: packages/catcher-rs
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt
      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: packages/catcher-rs
      - name: Format check
        run: cargo fmt --all -- --check
      - name: Clippy
        run: cargo clippy --all-targets -- -D warnings
      - name: Test
        run: cargo test --all-targets
      - name: Doc test
        run: cargo test --doc

  lint:
    name: Lint
    runs-on: ubuntu-latest
    defaults:
      run:
        working-directory: packages/catcher-rs
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt
      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: packages/catcher-rs
      - run: cargo fmt --all -- --check
      - run: cargo clippy --all-targets --all-features -- -D warnings
```

### 3.2 关键 CI 检查项

| 检查项 | 命令 | 触发条件 |
|--------|------|---------|
| 格式检查 | `cargo fmt --all -- --check` | 每次 push/PR |
| Clippy | `cargo clippy -- -D warnings` | 每次 push/PR |
| 单元测试 | `cargo test` | 每次 push/PR |
| 集成测试 | `cargo test --test '*'` | 每次 push/PR |
| Doc test | `cargo test --doc` | 每次 push/PR |

---

## 4. 开发工作流

### 4.1 本地开发命令

```bash
# 在 packages/catcher-rs/ 下：

# 持续编译检查（开发模式）
cargo watch -x check

# 运行全部测试
cargo test

# 运行特定模块测试
cargo test transport       # transport 层测试
cargo test resilience       # resilience 层测试
cargo test codec            # codec 层测试

# 带日志的测试
RUST_LOG=debug cargo test -- --nocapture

# Clippy + fmt
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
```

### 4.2 推荐的 VSCode 插件

- `rust-analyzer` — 语言服务器
- `Even Better TOML` — Cargo.toml 语法支持
- `crates` — 依赖版本提示
- `CodeLLDB` — 调试器

### 4.3 提交规范

```
feat(rs): implement HttpTransport with reqwest
fix(rs): handle connection pool idle timeout
test(rs): add resilience retry unit tests
refactor(rs): extract TlsConfig builder
```

---

## 5. 文件创建顺序（跨所有 Phase）

| 步骤 | 文件 | Phase |
|------|------|-------|
| 1 | `Cargo.toml` | 脚手架 |
| 2 | `src/lib.rs` (空, `pub mod error;` 等) | 脚手架 |
| 3 | `src/error.rs` | Phase 1 |
| 4 | `src/config.rs` | Phase 1 |
| 5 | `src/types/*.rs` (5 files) | Phase 1 |
| 6 | `src/codec/*.rs` (2 files) | Phase 1 |
| 7 | `src/transport/*.rs` (4 files) | Phase 2 |
| 8 | `src/resilience/*.rs` (4 files) | Phase 3 |
| 9 | `src/ws/*.rs` (4 files) | Phase 2 |
| 10 | `src/scheduler/*.rs` (2 files) | Phase 4 |
| 11 | `src/observability/*.rs` (2 files) | Phase 4 |
| 12 | `src/ffi/*.rs` (5 files) | Phase 5 |
| 13 | `catcher-rs-napi/` (npm 包) | Phase 5 |
| 14 | `catcher_core/` (pub.dev 包) | Phase 5 |
