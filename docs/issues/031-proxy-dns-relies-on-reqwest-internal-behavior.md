# Bug: 代理下「目标域名不被本地解析」的正确性依赖 reqwest 未文档化的内部行为，仅靠测试兜底

**严重程度**: 🟢 Low-Medium — 当前行为正确，但属于隐性脆弱点：reqwest 升级若改变内部行为，目标域名可能被本地解析并以 IP 泄漏给代理，破坏 Clash/VPN 的域名分流，且无防御性代码、唯一护栏是 feature-gated 的集成测试

**状态**: Fixed — 采用方案 A：在 HTTP/WS resolver 注入处、`Cargo.toml`、CI 与 arch 文档显式记录对 reqwest 内部行为的依赖与「升级须重跑 proxy_dns_behavior_test」规则（CI 默认 feature 已覆盖该测试）

**影响包**: `catcher-http`、`catcher-ws`

**位置**:
- `packages/catcher-http/src/transport/http_client.rs:198-205`（resolver 注入）+ `:207-222`（proxy 应用）
- `packages/catcher-ws/src/transport/ws_client.rs:378-399`（同上）
- `packages/catcher-http/tests/proxy_dns_behavior_test.rs`、`packages/catcher-ws/tests/proxy_dns_behavior_test.rs`（唯一护栏）

---

## 现象

PR #15 的核心正确性保证是：**当配置了代理时，目标域名必须交给代理远端解析，绝不能在本地解析成 IP 后再交给代理**（否则 Clash 的 fake-ip / 域名分流会失效，正是移动端故障的根因）。

这个保证目前由两件事共同维持：
1. `socks5://` → `socks5h://` 归一化（`ProxyConfig::transport_url`，确实强制远端解析）；
2. **reqwest 在请求走代理时不会调用自定义 `dns_resolver`** —— 因此即便 Catcher DNS（`DnsMode::Catcher`）的自定义 resolver 被全局注入，代理路径上的目标域名也不会被它本地解析。

第 2 点是隐式依赖：代码把 resolver 无条件注入到 `ClientBuilder`，然后另外应用 proxy，**没有任何地方显式声明"代理时不要用这个 resolver 解析目标 host"**。它能工作，纯粹因为 reqwest 当前的内部实现恰好如此。

## 根因

resolver 必须为**直连 / `no_proxy` 命中**的 host 服务（这些走本地解析是对的），因此**不能简单地"配了代理就不注入 resolver"** —— 那会破坏 no_proxy 旁路。于是代码选择了"全局注入，依赖 reqwest 对代理路径自动跳过 resolver"的隐式策略。

风险点：
- 这一行为**未见于 reqwest 的稳定 API 文档承诺**，是实现细节。reqwest 0.13 → 未来版本若调整代理与自定义 resolver 的交互顺序，目标域名可能被本地解析，静默回归到"IP 泄漏给代理"的故障。
- 唯一的回归护栏是 `proxy_dns_behavior_test`（协议级探针断言代理收到的是域名而非 IP）。但这些测试 **gated on `hickory-dns` feature**，若 CI 的默认 feature set 不含它，升级 reqwest 时不会自动触发该测试。

## 修复方案与工作量

### 方案 A（推荐，小）：测试 + CI + 文档加固
1. 确保 `proxy_dns_behavior_test`（HTTP 与 WS）在 **CI 的默认/相关 feature set 下实际运行**（含 `hickory-dns`），而非仅本地手动 `--ignored`。
2. 在 reqwest 依赖处用注释 + 文档（如 `docs/arch-rs/04-transport.md`）**显式记录**："代理路径目标域名不本地解析"依赖 reqwest 内部行为，**升级 reqwest 必须重跑 proxy_dns_behavior_test**。
3. 可选：在 `Cargo.toml` 对 reqwest 采用谨慎的版本约束，升级走显式评审。

- **工作量**：小（CI 配置 + 注释/文档 + 可能的版本约束）。
- **影响范围**：无运行时改动，纯流程/测试加固。

### 方案 B（中-大）：显式自定义 Connector，不依赖隐式行为
实现一个包裹层，在请求分流时显式区分"走代理（交域名给代理）"与"直连（用 Catcher resolver）"，把正确性从 reqwest 内部行为中解耦出来。

- **工作量**：中-大（需介入 reqwest 连接层或自建 connector，与现有 TLS/pinning/proxy 逻辑交织）。
- **影响范围**：连接层改动，回归风险显著高于收益。
- **权衡**：当前行为已正确，方案 B 主要买"对 reqwest 升级的免疫"，收益有限，**不建议**除非未来 reqwest 真的破坏了该行为。

## 推荐

**方案 A**。这是一个"加固护栏"而非"修 bug"的工作：把隐式依赖显式化（文档 + CI 必跑的测试），让 reqwest 升级时回归能被立即捕获。方案 B 的连接层重构在没有实际破坏发生前不划算。

## 影响范围小结

| 维度 | 评估 |
|------|------|
| 是否大改 | 否（方案 A）；是（方案 B 连接层重构） |
| 当前是否有 bug | 否 —— 行为正确，问题是「脆弱 + 缺护栏」 |
| 跨语言绑定 | 无（纯 Rust + CI） |
| 触发条件 | reqwest 升级改变代理 × 自定义 resolver 交互；或 CI 不跑 hickory-dns 测试时的静默回归 |

## 验证建议

- 确认 `proxy_dns_behavior_test` 在 CI workflow 中以含 `hickory-dns` 的 feature 运行，并在 reqwest 升级 PR 上强制通过。
- 测试本身质量已达标：`catcher-test-support` 的 `Socks5Probe` 在协议字节层区分 `Domain(0x03)` vs `Ip(0x01)`，`HttpProxyProbe` 区分 `CONNECT authority` vs 转发 URI，能真正证明远端解析。复用即可。

## 关联

- PR #15 `fix: support proxy dns behavior across http and ws`
- [026-mobile-proxy-vpn-clash.md](./026-mobile-proxy-vpn-clash.md)、[027-proxy-vpn-network-compatibility-research.md](./027-proxy-vpn-network-compatibility-research.md)
- `packages/catcher-test-support/src/socks5.rs`、`http_proxy.rs` — 协议级探针
- `ProxyConfig::transport_url`（`catcher-core/src/types/network.rs:106-115`）— socks5→socks5h 归一化
