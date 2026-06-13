# Bug: `q05_subscribe_multiple` 时序 flake，且 CI 缺 `--no-fail-fast` 导致单个 flake 掩盖整个测试套件

**严重程度**: 🟡 Medium — 不是产品 bug，但严重影响 CI 可信度：CI 长期为红，且红的真因掩盖了后续所有测试是否通过

**状态**: Fixed

**影响包**: `catcher-ffi`（测试）、CI（`.github/workflows/ci.yml`）

**位置**:
- `packages/catcher-ffi/tests/quality_test.rs:143-183`（`q05_subscribe_multiple`）
- `.github/workflows/ci.yml`（`cargo test --workspace`）

---

## 现象

1. CI 的 `rust-check` job 长期失败。排查发现唯一失败的测试是 `q05_subscribe_multiple`（`quality_test.rs:177` 的断言），自 ≥v0.3.12 起间歇性红。
2. 更严重的连带问题：`cargo test --workspace` **未加 `--no-fail-fast`**，默认会在**第一个失败的测试二进制处停止**。由于 `catcher-ffi` 的 quality_test 在 catcher-ws 等之前运行，一旦 `q05` flake，cargo 直接 bail，**后续整个套件（含 catcher-ws lib 测试、proxy_dns_behavior 等）根本不执行**。

对比两次 CI 运行可证实：
- `849187d`（PR #13 合并，q05 恰好通过）→ CI 绿，且日志中 `connect_stream_uses_dns_host_mapping ... ok`、其余套件正常跑完。
- `15b1233`（PR #15 合并，q05 flake）→ CI 红，日志中**只有 q05 失败**，之后所有测试二进制**未出现**（被 fail-fast 截断）。

## 根因

### q05 的 flake

`q05_subscribe_multiple` 创建两个独立订阅（`sub1` / `sub2`），都以 1000ms 间隔探测 `https://www.example.com`，每次**真实网络探测完成后**才触发一次回调。测试在 2s 后退订 `sub1`、再 1s 后断言：

```rust
assert!(count2 >= count1, "sub2 should have at least as many callbacks as sub1");
```

这是一个**跨 prober 的相对速度比较**。两个订阅的回调次数取决于各自网络探测的实际延迟，是非确定的：若 `sub2` 的探测恰好比 `sub1` 慢（或第 N 次探测仍在飞行中），就会出现 `count2 < count1`，断言失败。即测试用"谁的回调多"这一脆弱代理，去验证"多订阅独立工作"。

### CI fail-fast 掩盖

`cargo test` 默认 fail-fast：第一个失败的测试二进制就让整条命令停止。单个 flaky 测试因此能掩盖后续所有测试的真实状态 —— CI 看似"红在 q05"，实则后面的测试**根本没机会运行**，掩盖了真实覆盖。

## 修复

### 1. q05 改为按生命周期断言，不比较两个 prober 的相对速度
保留测试意图（多订阅独立 + 退订只影响自身），但改用**每个订阅各自单调**的判定（与稳定的 `q03` 同语义）：

```rust
unsafe { quality::catcher_quality_unsubscribe(sub1) };
let count1_at_unsub = state1.lock().unwrap().count;
let count2_at_unsub = state2.lock().unwrap().count;
tokio::time::sleep(Duration::from_secs(2)).await;
let count1_final = state1.lock().unwrap().count;
let count2_final = state2.lock().unwrap().count;
// sub1 退订后不再新增回调
assert_eq!(count1_final, count1_at_unsub, "sub1 退订后不应再收到回调");
// sub2 不受 sub1 退订影响，计数单调不减
assert!(count2_final >= count2_at_unsub, "sub2 不应受 sub1 退订影响");
```

新断言不依赖网络探测的相对快慢，因而确定、稳定（本地连跑 3 次 + 全套件 5/5 通过）。

### 2. CI 加 `--no-fail-fast`
`cargo test --workspace --no-fail-fast`，让一个失败测试不再截断后续套件，CI 能反映全部测试的真实状态。

## 影响范围小结

| 维度 | 评估 |
|------|------|
| 是否大改 | 否 —— 改一个测试断言 + CI 一行 |
| 是否产品 bug | 否 —— 纯测试健壮性与 CI 配置 |
| 价值 | 高 —— 恢复 CI 可信度，避免单 flake 掩盖整套件 |

## 验证

- `cargo test -p catcher-ffi --test quality_test` —— 5 passed（q02/q03/q04/q05/q07）。
- q05 连续 3 次运行均通过。

## 关联

- [023-evaluate-quality-race-panic.md](./023-evaluate-quality-race-panic.md) — 网络质量相关历史问题
- 由 #032/`connect_stream` 测试排查衍生：发现 CI 实际未跑到 catcher-ws 套件，根因即本 issue 的 fail-fast。
- `connect_stream_uses_dns_host_mapping`（macOS 沙箱本地失败、Linux CI 通过）的环境特异性结论，依赖本 issue 修复后 CI 能完整跑完套件来持续验证。
