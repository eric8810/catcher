# Review Round 3 — CI/CD + Infrastructure

**Date:** Round 3 (after 36 issues found and fixed in rounds 1-2)

## HIGH

### 1. Release Please only triggers on `main` branch
- **File:** `.github/workflows/release-please.yml:5`
- Triggers on `push: branches: [main]` but the repo uses `master` branch.
  Release Please will never run on this repo.
- **Fix:** Change to `branches: [main, master]` to match CI workflow.

## MEDIUM

### 2. `Cargo.lock` in `.gitignore` — non-reproducible Rust builds
- **File:** `.gitignore:3`
- `Cargo.lock` is gitignored. For binary/application crates (which this is — builds shared
  libraries for N-API/UniFFI), `Cargo.lock` should be committed for reproducible builds.
- **Fix:** Remove `Cargo.lock` from `.gitignore`.

## LOW

### 3. `pnpm-lock.yaml` still not committed
- `pnpm-lock.yaml` was removed from `.gitignore` in round 2 but never generated/committed.
  CI uses `pnpm install` (not `--frozen-lockfile`) so builds aren't reproducible.
- **Note:** Will be generated on next `pnpm install` and should be committed.

## Total: 3 issues (1 HIGH, 1 MEDIUM, 1 LOW)
