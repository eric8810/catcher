# Review Round 2: CI/CD + Infra

**Date**: 2026-05-13  
**Scope**: All infrastructure, CI/CD, config, and test configuration files  
**Status**: 11 issues found (2 CRITICAL, 2 HIGH, 4 MEDIUM, 3 LOW)

---

## CRITICAL

### C-1. `pnpm-lock.yaml` is gitignored but CI requires it

**File**: `.gitignore:4`  
**Also**: `.github/workflows/ci.yml:19`, `.github/workflows/ci.yml:33`

`.gitignore` contains `pnpm-lock.yaml`, and `git ls-files pnpm-lock.yaml` confirms the file is NOT tracked in the repository.

Both CI jobs run `pnpm install --frozen-lockfile`, which **fails** when no lockfile is present:

```
ERR_PNPM_LOCKFILE_MISSING  Missing pnpm-lock.yaml file
```

**Impact**: CI will break on any fresh clone or clean runner. Non-deterministic dependency resolution across installs.  
**Fix**: Remove `pnpm-lock.yaml` from `.gitignore`, run `pnpm install`, and commit the generated lockfile.

---

### C-2. `release-please-config.json` contains literal placeholder for `bootstrap-sha`

**File**: `release-please-config.json:3`

```json
"bootstrap-sha": "<TODO: set to main branch latest commit SHA before first release>"
```

This is a literal `<TODO:…>` string, not a valid 40-character hex SHA. Release Please will fail to parse it and error out on every push to `main`.

**Impact**: The entire automated release pipeline is non-functional.  
**Fix**: Replace with the actual latest commit SHA on `main` (e.g. `03753f0…`), or remove the field entirely if no prior releases exist (Release Please will default to scanning from the beginning).

---

## HIGH

### H-1. `.release-please-manifest.json` referenced but does not exist

**File**: `.github/workflows/release-please.yml:19`

```yaml
manifest-file: .release-please-manifest.json
```

This file does not exist in the repository. While Release Please v4 can auto-create it on first successful run, combined with **C-2** (invalid `bootstrap-sha`), the workflow will error before reaching that point.

**Impact**: Release Please workflow cannot succeed.  
**Fix**: Fix C-2 first, then either create an initial empty `{}` manifest, or let Release Please create it after bootstrap-sha is corrected.

---

### H-2. No Rust tests in CI — only `cargo check`

**File**: `.github/workflows/ci.yml:40`

```yaml
- run: cd packages && cargo check
```

The CI only runs `cargo check` (type-checking / compilation verification) but **not** `cargo test`. Any logic bugs in Rust crates (`catcher-core`, `catcher-http`, `catcher-ws`, `catcher-napi-http`, `catcher-napi-ws`, `catcher-uniffi`) will pass CI undetected.

**Impact**: Rust code is compiled but never tested in CI.  
**Fix**: Add `cargo test` step after `cargo check`:
```yaml
- run: cd packages && cargo test
```

---

## MEDIUM

### M-1. No Rust dependency caching in CI

**File**: `.github/workflows/ci.yml:35-40`

The `rust-check` job installs the Rust toolchain and compiles all crates without any caching. Every CI run re-downloads and recompiles all Rust dependencies from scratch.

**Impact**: Slow CI (potentially 3-10 min wasted per run). Wasted GitHub Actions runner minutes.  
**Fix**: Add `Swatinem/rust-cache@v2` after `dtolnay/rust-toolchain@stable`:
```yaml
- uses: Swatinem/rust-cache@v2
  with:
    workspaces: "packages -> target"
```

---

### M-2. GitHub Actions not SHA-pinned (supply-chain risk)

**Files**:
- `.github/workflows/ci.yml:13` — `actions/checkout@v4`
- `.github/workflows/ci.yml:14` — `pnpm/action-setup@v4`
- `.github/workflows/ci.yml:15` — `actions/setup-node@v4`
- `.github/workflows/ci.yml:39` — `dtolnay/rust-toolchain@stable`
- `.github/workflows/release-please.yml:15` — `googleapis/release-please-action@v4`

All actions use mutable tag-based refs. A compromised action maintainer (or tag force-push) could inject malicious code into CI.

**Impact**: Supply-chain attack vector.  
**Fix**: Pin all actions to commit SHAs with version comments:
```yaml
- uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4.2.2
```

---

### M-3. E2E and chaos tests not run in CI

**File**: `.github/workflows/ci.yml:33`

CI only runs `pnpm test` → `vitest run` (integration tests via `vitest.config.ts`). The e2e (`vitest.e2e.config.ts`) and chaos (`vitest.chaos.config.ts`) test suites are never executed in CI despite having test files:
- `packages/test/e2e/scenarios.test.ts`
- `packages/test/e2e/rust-vs-vanilla.test.ts`
- `packages/test/chaos/chaos.test.ts`
- `packages/test/chaos/extreme-scenarios.test.ts`

**Impact**: E2E and chaos regressions can be merged to `main` without detection.  
**Fix**: Add CI jobs or matrix entries for `pnpm test:e2e` and `pnpm test:chaos`.

---

### M-4. No `pnpm build` verification in CI

**File**: `.github/workflows/ci.yml`

The CI pipeline has `typecheck` and `test` jobs for TypeScript, but never runs `pnpm build`. A broken build (e.g., misconfigured `tsconfig`, broken `vite` config) can be merged without detection.

**Impact**: Build failures discovered only at release time or by developers pulling `main`.  
**Fix**: Add a build step to the CI, either as a separate job or after `typecheck`:
```yaml
- run: pnpm build
```

---

## LOW

### L-1. `test:integration` script is identical to `test`

**File**: `package.json:8-9`

```json
"test": "vitest run",
"test:integration": "vitest run",
```

Both scripts run the exact same command. `test:integration` is misleading — it implies a distinct scope but produces identical behavior.

**Fix**: Either remove `test:integration` or make it explicit: `"test:integration": "vitest run --config vitest.config.ts"`.

---

### L-2. No actual linter — `lint` script is just `typecheck`

**File**: `package.json:16`

```json
"lint": "pnpm typecheck"
```

No ESLint, Prettier, or other static analysis tool is configured. The `lint` script is an alias for typecheck, providing no code-style or quality enforcement beyond type checking.

**Impact**: Inconsistent code style, no static analysis beyond the TypeScript compiler.  
**Fix**: Add ESLint and/or Prettier with appropriate configs.

---

### L-3. `release-please-config.json` excludes `napi-*` packages

**File**: `release-please-config.json:7-19`

Only 4 packages are listed: `catcher-core-ts`, `catcher-http-ts`, `catcher-ws-ts`, `catcher-web`. The npm-publishable `catcher-napi-http` and `catcher-napi-ws` packages are excluded from release automation.

If this is intentional (e.g., napi packages are versioned/released manually or via a different pipeline), it should be documented.

**Impact**: Napi packages won't get automated changelogs or version bumps.  
**Fix**: Add napi packages to the config, or add a comment in the config documenting the exclusion rationale.

---

## Files Reviewed — No Issues

| File | Verdict |
|------|---------|
| `vitest.e2e.config.ts` | ✅ Correct — valid glob, test files exist |
| `vitest.chaos.config.ts` | ✅ Correct — valid glob, test files exist |
| `vitest.config.ts` | ✅ Correct — valid globs, proper exclude for napi.test.ts |
| `vitest.napi.config.ts` | ✅ Correct — targets napi.test.ts specifically |
| `packages/Cargo.toml` | ✅ Correct — all 6 workspace members have matching `Cargo.toml` files |
| `packages/catcher-uniffi/build.rs` | ✅ Correct — empty `main()` is valid for uniffi proc-macro mode |
| `pnpm-workspace.yaml` | ✅ Correct — `packages/*` matches all workspace packages |
| `CHANGELOG.md` | ✅ Present, placeholder for release-please automation |
| `LICENSE` | ✅ Present, valid MIT license |
| `.gitignore` | ⚠️ See C-1 (`pnpm-lock.yaml`), otherwise correct |

---

## Summary

| Severity | Count | Key Issues |
|----------|-------|------------|
| CRITICAL | 2 | `pnpm-lock.yaml` gitignored (breaks CI), placeholder `bootstrap-sha` (breaks releases) |
| HIGH | 2 | Missing release manifest, no `cargo test` in CI |
| MEDIUM | 4 | No Rust caching, actions not SHA-pinned, e2e/chaos not in CI, no build verification |
| LOW | 3 | Redundant script, no real linter, napi packages excluded from release |

**Priority fixes**: C-1 and C-2 should be resolved before next push to `main`, as they block CI and releases respectively.
