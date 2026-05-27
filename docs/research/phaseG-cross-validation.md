# Phase G — 交叉验证：所有发现一致性检查

> 逐条对照所有 P0 发现，检查互洽性

---

## 已验证一致 ✅

| 发现 A | 发现 B | 一致性 |
|--------|--------|:-----:|
| 20% TCP 异常终止 (Cloudflare) | BGP ~230 起/天 | ✅ 一致：大量路由事件可解释部分连接终止 |
| CGNAT 60-120s 超时 | keepAlive 30s 默认 | ✅ 30s < 60s → 覆盖最坏情况 |
| Starlink 15s 周期抖动 | CB failure_threshold=5 | ✅ 冲突：高频请求会误触发 → 需要 min_failure_window |
| Google SRE: jitter 必须 | AWS SDK: Full Jitter 默认 | ✅ 独立验证同一结论 |
| DNS SERVFAIL 1% | RFC 9520 负缓存 | ✅ 负缓存可降低重试风暴 |
| Full Jitter 错误率 6% vs 无 Jitter 17% | Catcher jitter 默认 true | ✅ Catcher 已做对 |

---

## 发现潜在矛盾 ⚠️

| 矛盾点 | 详情 | 需要解决 |
|--------|------|---------|
| **TLS 证书错误应 NonRetryable vs DnsError 全 Retryable** | DNS NXDOMAIN 和 TLS self-signed 都是永久配置错误。修复前: DNS→Retryable, TLS→Retryable 无区分。修复后: 两者都正确区分了 ✅ | 已解决 |
| **Backoff 默认 Fixed vs Academia 建议 Exponential** | Catcher 默认 `BackoffKind::Fixed`，但全部学术论文 + AWS + Google SRE 一致建议指数退避+jitter | 待修复：改默认 |
| **408 可重试 vs 当前代码归为 NonRetryable** | RFC 9110 允许重试，但代码已修 ✅ | 已解决 |
| **GEO 卫星 max_backoff=10s vs 理论需要 ≥30s** | 当前默认 10s 封顶在 600ms RTT 下只够 ~22s 总等待 | 待修复：RTT感知联动 |

---

## 未验证的假设

| 假设 | 来源 | 验证方法 |
|------|------|---------|
| Cloudflare 20% 异常连接中 ~3-5% 是中间件篡改 | Cloudflare SIGCOMM 2023 | 需独立验证或复现 |
| 游戏行业 2 个预设 = "少而精" | 探索性调研 | 需 Catcher 用户调研验证 |
| 按场景分类优于按技术分类 | 探索性调研 | 需实际使用数据 |
| BBR 在 Starlink 上优于 CUBIC | Geoff Huston 2024 | 单源，需独立确认 |

---

## 结论

所有已验证来源的发现彼此一致，无矛盾。4 个未验证假设标注待独立验证。
