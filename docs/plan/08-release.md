# 08 — 对外发包 Release 方案

> 目标：catcher 首次对外 release，先发 TS 包，NAPI 原生包后续再发
> 版本策略：v0.1.0 起步，语义化版本，release-please 自动管理

---

## 一、发布范围与策略

### 首期发布（Phase 1）

| 包名 | 路径 | 类型 |
|------|------|------|
| `@eric8810/catcher-core` | `packages/catcher-core-ts` | TS 纯类型定义 |
| `@eric8810/catcher-http` | `packages/catcher-http-ts` | TS HTTP 客户端 |
| `@eric8810/catcher-ws` | `packages/catcher-ws-ts` | TS WebSocket 客户端 |
| `@eric8810/catcher-web` | `packages/catcher-web` | Browser HTTP 客户端 |

### 后续发布（Phase 2）

| 包名 | 路径 | 说明 |
|------|------|------|
| `@eric8810/catcher-napi-http` | `packages/catcher-napi-http` | Rust 原生 HTTP，需多平台 prebuild |
| `@eric8810/catcher-napi-ws` | `packages/catcher-napi-ws` | Rust 原生 WS，需多平台 prebuild |
| `catcher-core` / `catcher-http` / `catcher-ws` | `packages/` (Rust) | 发布到 crates.io |

### 版本号

首版 `0.1.0`。采用语义化版本（semver），0.x 阶段 API 可能变动，不承诺向后兼容。
进入 1.0 后严格遵守 semver。

---

## 二、当前问题诊断

### 问题 1：package.json main/exports 指向源码

```
当前:  "main": "./src/index.ts",  "exports": { ".": "./src/index.ts" }
期望:  发布时指向编译产物 dist/
```

现有配置适合 monorepo 内部 workspace 引用（直接引 TS 源码，无需构建），
但发布到 npm 后，消费者拿到的包里不应有 `.ts` 源码作为入口。

### 问题 2：@eric8810/catcher-web 缺少 build 脚本

其他三个包有 `build: tsc`，@eric8810/catcher-web 只有 `typecheck`。

### 问题 3：files 字段未包含 dist

`files: ["src"]` → 只发布源码。改为 `files: ["dist"]`，仅发布编译产物。

### 问题 4：无 CI/CD

没有 `.github/workflows`，无自动化测试、构建、发版流水线。

---

## 三、修复方案

### 3.1 双模出包策略

保留 `main`/`exports` 指向 `./src/index.ts` 用于 monorepo 内部引用，
通过 `publishConfig` 覆盖，npm publish 时自动切换为 `dist/` 入口。

```json
{
  "main": "./src/index.ts",
  "types": "./src/index.ts",
  "exports": {
    ".": {
      "import": "./src/index.ts",
      "types": "./src/index.ts"
    }
  },
  "files": ["dist"],
  "publishConfig": {
    "main": "./dist/index.js",
    "types": "./dist/index.d.ts",
    "exports": {
      ".": {
        "import": "./dist/index.js",
        "types": "./dist/index.d.ts"
      }
    }
  }
}
```

### 3.2 四个包改动清单

| 改动项 | @eric8810/catcher-core | @eric8810/catcher-http | @eric8810/catcher-ws | @eric8810/catcher-web |
|--------|:---:|:---:|:---:|:---:|
| `files` 改为 `["dist"]` | ✅ | ✅ | ✅ | ✅ |
| 添加 `publishConfig` | ✅ | ✅ | ✅ | ✅ |
| 添加 tsconfig.json | — | — | — | ✅ |
| 添加 `build: tsc` | — | — | — | ✅ |
| 更新 devDependencies 加 typescript | — | — | — | ✅ |
| 确认 tsconfig outDir: ./dist | ✅ | ✅ | ✅ | ✅ |

### 3.3 文件结构（以 @eric8810/catcher-web 为例）

```
packages/catcher-web/
├── package.json          # main→src, publishConfig→dist
├── tsconfig.json         # outDir: ./dist, rootDir: ./src
├── src/
│   ├── index.ts
│   ├── http/
│   │   ├── client.ts
│   │   └── interceptors.ts
│   └── ws/
│       ├── index.ts
│       └── client.ts
└── dist/                 # tsc 产出，进 npm 包
    ├── index.js
    ├── index.d.ts
    └── ...
```

---

## 四、release-please 接入方案

### 4.1 选型理由

| 工具 | 优势 | 劣势 |
|------|------|------|
| **release-please** | Google 出品，基于 conventional commits，自动 CHANGELOG + GitHub Release，支持 monorepo | 要求严格 conventional commits |
| changesets | pnpm 生态首选，交互式 | 需要额外手写 changeset 文件 |
| 手动管理 | 零依赖 | 易出错，无 CHANGELOG 自动生成 |

选择 release-please：自动化程度最高，与 conventional commits 规范天然匹配。

### 4.2 配置文件

仓库根目录 `release-please-config.json`：

```json
{
  "$schema": "https://raw.githubusercontent.com/googleapis/release-please/main/schemas/config.json",
  "bootstrap-sha": "<main 分支最新 commit SHA>",
  "release-type": "node",
  "include-v-in-tag": true,
  "plugins": ["node-workspace"],
  "packages": {
    "packages/catcher-core-ts": {
      "package-name": "@eric8810/catcher-core"
    },
    "packages/catcher-http-ts": {
      "package-name": "@eric8810/catcher-http"
    },
    "packages/catcher-ws-ts": {
      "package-name": "@eric8810/catcher-ws"
    },
    "packages/catcher-web": {
      "package-name": "@eric8810/catcher-web"
    }
  },
  "changelog-sections": [
    { "type": "feat", "section": "Features", "hidden": false },
    { "type": "fix", "section": "Bug Fixes", "hidden": false },
    { "type": "perf", "section": "Performance Improvements", "hidden": false },
    { "type": "docs", "section": "Documentation", "hidden": true },
    { "type": "chore", "section": "Miscellaneous", "hidden": true },
    { "type": "refactor", "section": "Code Refactoring", "hidden": true },
    { "type": "test", "section": "Tests", "hidden": true },
    { "type": "ci", "section": "CI/CD", "hidden": true }
  ]
}
```

### 4.3 工作流

`.github/workflows/release-please.yml`：

```yaml
name: Release Please

on:
  push:
    branches:
      - main

permissions:
  contents: write
  pull-requests: write

jobs:
  release-please:
    runs-on: ubuntu-latest
    steps:
      - uses: googleapis/release-please-action@v4
        with:
          token: ${{ secrets.GITHUB_TOKEN }}
          config-file: release-please-config.json
          manifest-file: .release-please-manifest.json
```

### 4.4 发版流程

```
developer commit (feat: / fix:)
        │
        ▼
   PR merge → main
        │
        ▼
   release-please 扫描 commit
        │
        ├── 无 feat/fix → 什么都不做
        │
        └── 有 feat/fix → 创建/更新 Release PR
                │
                ▼
         Release PR 自动 bump 版本号 + 写 CHANGELOG.md
                │
                ▼
         Reviewer 合并 Release PR
                │
                ▼
         release-please 打 git tag + 创建 GitHub Release
                │
                ▼
         (后续) 触发 npm publish workflow
```

### 4.5 Commit 规范

开发者必须遵循 [Conventional Commits](https://www.conventionalcommits.org/)：

```
feat(http): add dynamic interceptor support
fix(ws): reconnect backoff not resetting on successful connect
perf(codec): reduce msgpack encode allocations
docs: update quick start examples
chore: bump dependencies
```

| 前缀 | 触发发版 | CHANGELOG 展示 |
|------|:---:|:---:|
| `feat:` | ✅ minor bump | Features |
| `fix:` | ✅ patch bump | Bug Fixes |
| `perf:` | ✅ patch bump | Performance |
| `docs:` | ❌ | 隐藏 |
| `chore:` | ❌ | 隐藏 |
| `refactor:` | ❌ | 隐藏 |
| `test:` | ❌ | 隐藏 |
| `ci:` | ❌ | 隐藏 |

### 4.6 首次接入步骤

1. 确保所有历史 commit 都（或大部分）遵循 conventional commits
2. 运行 release-please bootstrap 扫描现有 commit 生成 manifest
3. 首次手动 release 从 `v0.1.0` 开始
4. 后续自动流转

---

## 五、CI 质量门

### 5.1 CI workflow

`.github/workflows/ci.yml`：

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  typecheck:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: 'pnpm'
      - run: pnpm install --frozen-lockfile
      - run: pnpm typecheck

  test:
    needs: typecheck
    runs-on: ubuntu-latest
    strategy:
      matrix:
        node-version: [18, 20, 22]
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v4
      - uses: actions/setup-node@v4
        with:
          node-version: ${{ matrix.node-version }}
          cache: 'pnpm'
      - run: pnpm install --frozen-lockfile
      - run: pnpm test

  build:
    needs: test
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: 'pnpm'
      - run: pnpm install --frozen-lockfile
      - run: pnpm build
      - name: Verify dist output
        run: |
          for pkg in catcher-core-ts catcher-http-ts catcher-ws-ts catcher-web; do
            if [ ! -f "packages/$pkg/dist/index.js" ]; then
              echo "ERROR: packages/$pkg/dist/index.js missing" && exit 1
            fi
            if [ ! -f "packages/$pkg/dist/index.d.ts" ]; then
              echo "ERROR: packages/$pkg/dist/index.d.ts missing" && exit 1
            fi
          done
          echo "All dist outputs verified."
```

### 5.2 NPM Publish workflow（可选，建议手动首发）

首次发布建议手动在本地执行，后续再接入 CI 自动 publish：

```yaml
name: Publish

on:
  workflow_dispatch:
    inputs:
      package:
        description: 'Package to publish (all / core / http / ws / web)'
        required: true
        default: 'all'

jobs:
  publish:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      id-token: write    # npm provenance
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 20
          registry-url: 'https://registry.npmjs.org'
          cache: 'pnpm'
      - run: pnpm install --frozen-lockfile
      - run: pnpm -r build
      - run: pnpm -r publish --access public --no-git-checks
        env:
          NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}
```

---

## 六、发布前检查清单

### 本地验证

- [ ] `pnpm install --frozen-lockfile` 通过
- [ ] `pnpm typecheck` 全部通过
- [ ] `pnpm test` 全部通过
- [ ] `pnpm -r build` 全部包产出 `dist/`
- [ ] 四个包 `dist/index.js` 和 `dist/index.d.ts` 存在
- [ ] `pnpm pack --dry-run` 确认每个包只包含 dist（不含 src/）
- [ ] 在另一个空项目中 `npm install ../catcher/packages/catcher-core-ts/catcher-core-0.1.0.tgz` 后能正常 import

### 配置检查

- [ ] 四个包 `package.json` → `publishConfig` 正确指向 dist
- [ ] 四个包 `package.json` → `files: ["dist"]`
- [ ] `release-please-config.json` → `bootstrap-sha` 已设置
- [ ] `.gitignore` 中 `dist/` 已存在（确认不提交 dist 到 git）

### 文档检查

- [ ] `README.md` 中 install 命令正确（`npm install @eric8810/catcher-http`）
- [ ] root `README.md` 中的 Quick Start 代码可运行
- [ ] 每个包的 `description` 字段准确

### 账号准备

- [ ] 在 [npmjs.com](https://www.npmjs.com) 注册账号
- [ ] npm 账号已验证邮箱
- [ ] 本地 `npm login` 已登录
- [ ] `@catcher` scope 在 npm 上可用（未被占用）

---

## 七、首次发布操作步骤

### Step 1：修复四个包的构建配置

按照 3.2 改动清单，逐一修改 `package.json`、补充 `tsconfig.json`。

### Step 2：本地构建验证

```bash
pnpm install
pnpm typecheck
pnpm test
pnpm -r build
```

### Step 3：dry-run 验证发布内容

```bash
cd packages/catcher-core-ts && pnpm pack --dry-run
cd packages/catcher-http-ts && pnpm pack --dry-run
cd packages/catcher-ws-ts && pnpm pack --dry-run
cd packages/catcher-web && pnpm pack --dry-run
```

确认每个包的 tarball 中只有 `dist/` 目录和 `package.json`、`README.md`，没有 `src/`。

### Step 4：首次发布

```bash
# 按依赖顺序发布
cd packages/catcher-core-ts && pnpm publish --access public
cd packages/catcher-http-ts && pnpm publish --access public
cd packages/catcher-ws-ts && pnpm publish --access public
cd packages/catcher-web && pnpm publish --access public
```

### Step 5：接入 release-please

1. 配置 `release-please-config.json` + `.github/workflows/release-please.yml`
2. 还需要创建 `.github/workflows/ci.yml`
3. Push → main → release-please 扫描历史 commit → 生成 Release PR
4. 合并 Release PR → 自动创建 GitHub Release + tag

---

## 八、NAPI 包后续发布（Phase 2 展望）

### 核心挑战

`@eric8810/catcher-napi-http` 和 `@eric8810/catcher-napi-ws` 包含 Rust 编译的 `.node` 原生二进制。
发布到 npm 需要为每个目标平台预编译：

| 平台 | arch |
|------|------|
| linux | x64, arm64 |
| macOS | x64, arm64 (Apple Silicon) |
| Windows | x64 |

### 方案

使用 napi-rs 的 prebuild 机制：

1. GitHub Actions 多平台矩阵构建（`ubuntu-latest`, `macos-latest`, `windows-latest`）
2. 每个平台 `cargo build --release` → `napi artifacts`
3. 将所有 .node 文件上传到 GitHub Release assets
4. npm install 时通过 `@napi-rs/cli` 自动下载对应平台的 .node

### 配置要点

- `package.json` 中 `napi.triples` 定义目标平台
- CI 中需要安装 Rust toolchain + napi-rs CLI
- release-please 需要单独配置这两个包的路径

### 时机

建议在 TS 包稳定（至少发布 2-3 个小版本）后再启动 NAPI 包的发布。

---

## 九、文档索引

| 编号 | 文件 | 内容 |
|------|------|------|
| 00 | `00-overview.md` | 开发总览、阶段划分 |
| 01 | `01-scaffold.md` | 项目脚手架 |
| ... | ... | ... |
| 07 | `07-test-reuse.md` | TS e2e 测试复用方案 |
| **08** | **`08-release.md`** | **对外发包 Release 方案** |
