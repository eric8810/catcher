# 阶段三：硬件与设备场景调研

> 调研日期：2025-07-18
> 范围：终端类型、芯片架构、网络硬件、移动设备特性

---

## C1. 移动设备网络特性

### C1.1 网络类型切换

| 场景 | 描述 | catcher 覆盖 | 建议测试 |
|------|------|:----------:|---------|
| WiFi ↔ Cellular 切换 | IP 地址变化，所有 TCP 连接断开 | ❌ | catcher 是否检测到 socket 错误并重连 |
| 飞行模式开启→关闭 | 短暂断网后恢复 | ❌ | 重连退避重置逻辑（不应从 max_delay 开始） |
| 网络类型降级 | WiFi→4G→3G→2G 逐级退化 | ⚠️ QualityEvaluator | 验证网络质量评估随切换更新 |
| 双 SIM 卡切换 | Android 双卡数据切换 | ❌ | IP 和路由表变化 |
| 热点连接 | 作为热点客户端 | ⚠️ | 高延迟+低带宽+共享链路 |

### C1.2 移动 OS 限制

| 平台 | 限制 | catcher 影响 |
|------|------|-------------|
| iOS 后台 | 30 秒后台执行时间 | 长连接/SSE 可能被挂起，恢复后连接已断开 |
| Android Doze | 网络访问受限（维护窗口） | 维持的连接可能超时 |
| iOS Low Data Mode | 减少网络使用 | 无影响（客户端库不感知） |
| Android Data Saver | 限制后台数据 | 前台应用不受限，但后台 Service 受限 |

| 场景 | 建议 |
|------|------|
| App 进入后台时 SSE 连接 | 需文档说明：后台 SSE 可能被 OS 挂起 |
| App 从后台恢复 | 验证 WS reconnect 逻辑在长时间后台后正确工作 |

---

## C2. 不同 CPU 架构

### C2.1 Rust 编译目标

catcher 当前支持的 Rust 架构（通过 napi-rs）：

| 架构 | 目标 triple | 测试现状 | 风险 |
|------|-----------|---------|------|
| x86_64 Linux | `x86_64-unknown-linux-gnu` | ✅ CI | 低 |
| x86_64 Windows | `x86_64-pc-windows-msvc` | ✅ CI | 低 |
| x86_64 macOS | `x86_64-apple-darwin` | ✅ CI | 低 |
| ARM64 macOS | `aarch64-apple-darwin` | ✅ CI | 低 |
| ARM64 Linux | `aarch64-unknown-linux-gnu` | ❌ 无 CI | 中 — musl 和 glibc 差异 |
| ARM64 Android | `aarch64-linux-android` | ❌ 无 CI | 中 — NDK 链接问题 |
| ARM64 iOS | `aarch64-apple-ios` | ❌ 无 CI | 中 |
| x86_64 Android | `x86_64-linux-android` | ❌ 无 CI | 低 |

### C2.2 字节序

| 场景 | 描述 | catcher 覆盖 | 建议 |
|------|------|:----------:|------|
| Big-endian 系统 | msgpack 字节序 | ✅ msgpack 规范是 big-endian | 验证 unpack 在 big-endian host 上 |
| s390x / PowerPC | 服务器级 big-endian | ❌ | 极少场景，但 Rust 支持这些 target |

---

## C3. 网络硬件特性

| 硬件 | 特性 | catcher 影响 |
|------|------|-------------|
| 企业防火墙 | Deep Packet Inspection, 协议白名单 | WS 可能被误杀（伪装成 HTTP 升级的恶意软件） |
| 硬件负载均衡器 | F5 BIG-IP, Citrix ADC | TCP RST 注入，连接断开无 FIN |
| WiFi AP 漫游 | 同一 SSID，不同 AP 间的 BSS Transition | IP 不变但短暂的丢包/L2 切换 |
| 移动基站切换 | LTE Handover | 50-100ms 中断，TCP 可恢复 |
| 卫星终端 | 高延迟+高抖动+非对称带宽 | 已有部分覆盖 |

---

## 阶段三总结：关键缺失

1. **ARM64 Linux CI** — Android/Linux ARM64 编译未在 CI 覆盖
2. **移动 OS 后台限制** — SSE/WS 长连接在 iOS/Android 后台行为的文档说明
3. **网络类型切换后的重连** — 确保不继承旧网络的退避状态
4. **WiFi 漫游/BSS Transition** — 短暂丢包不应触发重连
