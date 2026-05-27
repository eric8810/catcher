# Policy-Based Network Failures & Rate Limiting — Deep Research

> Compiled for Catcher — cross-platform network resilience library
> Date: 2026-07-14

---

## 1. HTTP 429 Rate Limit Headers Across Top APIs

### 1.1 Current State: Two Competing Header Standards

| Standard | Headers | Status |
|----------|---------|--------|
| **De-facto (X- prefixed)** | `X-RateLimit-Limit`, `X-RateLimit-Remaining`, `X-RateLimit-Reset`, `Retry-After` | Widely deployed, all major APIs |
| **IETF Draft (non-X prefixed)** | `RateLimit-Limit`, `RateLimit-Remaining`, `RateLimit-Reset`, `RateLimit-Policy` | `draft-ietf-httpapi-ratelimit-headers-11`, nearing RFC |

**Key difference**: `Retry-After` (seconds until retry, HTTP standard) vs `X-RateLimit-Reset` / `RateLimit-Reset` (UTC epoch seconds when window resets). The `Retry-After` header is the most critical for clients — it's the only one defined in RFC 7231 and has broadest support.

### 1.2 Major API Header Survey

| API | 429 Response Headers | Rate Limit Tier | Retry-After Format |
|-----|---------------------|-----------------|-------------------|
| **GitHub REST** | `X-RateLimit-Limit`, `X-RateLimit-Remaining`, `X-RateLimit-Reset`, `Retry-After` | 60/hr unauthenticated, 5,000/hr authenticated, 15,000/hr Enterprise | Integer seconds |
| **GitHub Secondary** | Same + `Retry-After` | 100 concurrent, 900 pts/min REST, 2,000 pts/min GraphQL, 90s CPU/60s real | Integer seconds |
| **Twitter/X v2** | `x-rate-limit-limit`, `x-rate-limit-remaining`, `x-rate-limit-reset` | Varies by endpoint (e.g., 3,000/15min for tweets) | UTC epoch in `x-rate-limit-reset` |
| **Shopify** | `X-Shopify-Shop-Api-Call-Limit` (format: `used/max`), `Retry-After` | 40 req/sec REST, 2,000 req/sec GraphQL (leaky bucket) | Float seconds (e.g., `2.0`) |
| **Stripe** | `Request-ID`, no standard rate headers | 100 req/sec (exponential backoff recommended) | No `Retry-After`; use backoff |
| **Google APIs** | `X-RateLimit-Limit`, `X-RateLimit-Remaining`, `X-RateLimit-Reset` | Varies by service (quota-based) | UTC epoch |
| **AWS** | `x-amzn-RequestId`, `x-amzn-ErrorType: ThrottlingException` | Token bucket per account/service | No `Retry-After`; SDK handles |
| **Discord** | `X-RateLimit-Limit`, `X-RateLimit-Remaining`, `X-RateLimit-Reset`, `Retry-After` | 50 req/sec per token | Float seconds |
| **Slack** | `Retry-After` | Tiered: 1+/sec methods, 20+/min (tier 2), 50+/min (tier 3) | Integer seconds |
| **Cloudflare API** | `Retry-After` | 1,200 req/5min | Integer seconds |

### 1.3 Catcher Implication

**Catcher must parse both `Retry-After` (HTTP-date or delta-seconds) and `X-RateLimit-Reset` (UTC epoch)**. The `Retry-After` header is the single most important signal. When present, it provides an authoritative backoff duration. When absent, Catcher should fall back to exponential backoff with jitter (base 1s for 429, doubling per retry).

**Header parsing strategy**:
1. Check `Retry-After` → if present, use as minimum wait (support both `delta-seconds` and `HTTP-date` per RFC 7231)
2. Check `X-RateLimit-Remaining` → if `0`, use `X-RateLimit-Reset - now` as minimum wait
3. Fallback: exponential backoff with jitter (1s, 2s, 4s, 8s, ... capped at 60s)

---

## 2. HTTP 403 vs 407 — Failure Rates and Context

### 2.1 Prevalence in the Wild

| Status Code | Typical Cause | Prevalence |
|-------------|--------------|------------|
| **403 Forbidden** | Authorization failure, IP block, WAF rule, geo-restriction, rate-limit escalation | **Very common** — seen in every API that implements auth or anti-abuse |
| **407 Proxy Authentication Required** | Corporate proxy requiring NTLM/Kerberos/Basic auth | **Context-dependent** — only behind authenticated proxies |

### 2.2 Proxy Market Data

| Metric | Value | Source |
|--------|-------|--------|
| **Fortune 500 proxy usage** | >78% use proxy networks | Market research 2023 |
| **Global online users behind proxies** | ~35% (up from 26% in 2018) | Statista 2024 |
| **Proxy server software market** | $4.8B (2025) → $11.2B (2034), 9.8% CAGR | Dataintelo |
| **Enterprise proxy auth methods** | NTLM (legacy Windows), Kerberos (modern SSO), Basic (rare, TLS required) | Industry standard |
| **NTLM status** | Deprecated by Microsoft but still dominant in Windows-heavy enterprises | Microsoft docs |

### 2.3 When 407 Occurs

407 is specific to **forward proxy** scenarios:
- Corporate networks routing all outbound HTTP through a proxy
- The proxy intercepts requests and demands authentication before forwarding
- Common in finance, government, healthcare, and large enterprise
- NTLM requires multi-leg handshake (407 → NTLM Type 1 → 407 → NTLM Type 2 → NTLM Type 3 → 200)

### 2.4 Catcher Implication

**407 is a hard failure that Catcher cannot resolve transparently**. Unlike 429 (retryable with backoff), 407 requires credentials. Catcher should:
- Detect 407 and surface as a distinct, non-retryable error type (`CatcherError::ProxyAuthRequired`)
- Include the `Proxy-Authenticate` header value in the error
- Not retry 407 unless proxy credentials are configured
- For NTLM: multi-leg handshake support would be needed but is out of scope for initial Catcher

**403 handling**: Distinguish between permanent 403 (auth failure, don't retry) and temporary 403 (rate-limit escalation, WAF challenge — potentially retryable with backoff and user interaction).

---

## 3. CDN Rate Limiting Internals

### 3.1 Cloudflare

| Aspect | Detail |
|--------|--------|
| **Algorithm** | Sliding window (approximate) with per-IP counters |
| **Architecture** | Distributed memcache per PoP, consistent hashing for sharding |
| **Counter storage** | Two integers per counter (current window count + previous window count) |
| **Rate calculation** | `rate = prev_count * ((window - elapsed)/window) + current_count` |
| **Accuracy** | 0.003% misclassified (false positive + false negative), ~6% avg rate deviation |
| **Scale** | Millions of domains, per-PoP isolation (anycast ensures same IP hits same PoP) |
| **Actions** | 429 response → JS challenge → CAPTCHA → Block |
| **Escalation** | Rate limit threshold → "I'm Under Attack" mode (JS fingerprint challenge) → IP reputation degradation |
| **Rule types** | Custom expressions, per-path, per-header, burst + average rate |
| **Response** | 429 with optional custom error page, no consistent `Retry-After` (varies by config) |

### 3.2 Fastly

| Aspect | Detail |
|--------|--------|
| **Primitives** | `ratecounter` (client request counting) + `penaltybox` (client blocking) |
| **Capacity** | Up to 200,000 entries per ratecounter |
| **Blocking** | Penalty box with configurable duration (seconds to hours) |
| **Architecture** | VCL (Varnish Config Language) or Compute@Edge (WASM) |
| **Escalation** | Rate counter threshold → penalty box → HTTP 429/403 |
| **Flexibility** | Fully programmable: custom rate keys, custom actions, custom response codes |

### 3.3 AWS WAF Rate-Based Rules

| Aspect | Detail |
|--------|--------|
| **Evaluation windows** | 60s, 120s, 300s (default), 600s |
| **Aggregation keys** | Source IP (default), header values, composite keys (IP + header) |
| **Actions** | Block, Count (monitor), CAPTCHA, Challenge |
| **Algorithm** | Token bucket model with burst capacity |
| **Minimum threshold** | 100 requests per evaluation window |
| **Scope** | Per-WebACL, per-region |

### 3.4 Catcher Implication

**CDN rate limiting operates at L7, invisible to the origin**. Catcher clients will receive:
- **429** from Cloudflare/Fastly rate limiting — treat as standard rate limit
- **403** with HTML challenge page — detectable as non-JSON response
- **503** from origin overload — treat as transient

Catcher should detect non-JSON/non-API responses (HTML CDN challenge pages) as a distinct error type (`CatcherError::CdnChallenge`).

---

## 4. Retry Budget Implementations

### 4.1 AWS SDK Token Bucket

| Parameter | Standard Mode (2026 update) | Previous Standard |
|-----------|---------------------------|-------------------|
| **Bucket capacity** | 500 tokens | 500 tokens |
| **Transient error cost** | 14 tokens per retry | 5 tokens per retry |
| **Throttling error cost** | 5 tokens per retry | 5 tokens per retry |
| **Replenish rate** | Token per successful request | Same |
| **Transient base delay** | 50 ms | Varies (10ms–1000ms) |
| **Throttling base delay** | 1,000 ms | Same as transient |
| **Max attempts default** | 3 (4 for DynamoDB) | Varies by SDK |
| **Retry modes** | standard, adaptive, legacy | Same |

**Key insight**: The 2026 update makes throttling MORE aggressive — transient errors cost 14 tokens instead of 5, so the quota depletes faster during sustained failures. This is by design to let services recover faster.

**Adaptive mode** adds a client-side rate limiter that can **delay initial requests** (not just retries) based on throttling feedback. Per-client-instance, so all requests from one client share the limiter.

### 4.2 Google SRE Adaptive Throttling

| Parameter | Value |
|-----------|-------|
| **Window** | 2 minutes of history |
| **Metrics** | `requests` (attempted), `accepts` (succeeded) |
| **Rejection probability** | `P = max(0, (requests - K*accepts) / (requests + 1))` |
| **K (multiplier)** | Default 2.0 (allows 2× requests vs accepts before throttling) |
| **Criticality** | CRITICAL_PLUS, CRITICAL, SHEDDABLE_PLUS, SHEDDABLE (separate stats per level) |

**The formula in practice**:
- Normal: `requests = accepts` → P = 0 (no rejection)
- Overloaded start: `requests = 1.5 × accepts` → P ≈ 0.33 (33% of new requests dropped locally)
- Severe: `requests = 5 × accepts` → P ≈ 0.8 (80% dropped)

**Key difference from AWS**: Google's approach is **probabilistic** and **stateless** (no token bucket, just a ratio decision per request). AWS's approach is **deterministic** (token count).

### 4.3 Catcher Implication

**Catcher should implement a retry budget system modeled on AWS's token bucket**:
- **Capacity**: Configurable (default 500 tokens for HTTP, 200 for WebSocket reconnect)
- **Cost per retry**: 10 tokens (429), 5 tokens (5xx transient)
- **Replenish**: 1 token per second of successful operation (recover ~60 tokens/min)
- **When depleted**: Fail fast with `CatcherError::RetryBudgetExhausted`
- **Progress bar**: Expose `retry_budget_remaining` as a metric

This prevents the "retry storm" problem where many concurrent connections all retry simultaneously during an outage.

---

## 5. Escalation Patterns When Clients Ignore Retry-After

### 5.1 Observed Escalation Ladder

```
Phase 1: 429 + Retry-After: N seconds
    ↓ (client ignores, continues at full rate)
Phase 2: 429 + Retry-After: N×2 seconds (escalated backoff)
    ↓ (client continues ignoring)
Phase 3: 403 Forbidden (temporary) — "rate limit exceeded too many times"
    ↓ (client persists)
Phase 4: 403 Forbidden (extended ban, minutes to hours)
    ↓ (client uses new IP or continues)
Phase 5: IP-level block / WAF block / permanent ban
```

### 5.2 Platform-Specific Patterns

| Platform | Escalation Pattern |
|----------|-------------------|
| **Cloudflare** | 429 → JS challenge → CAPTCHA → I'm Under Attack mode (JS fingerprint + redirect) → IP block |
| **Fastly** | Rate counter threshold (429) → penalty box (403, configurable duration) → permanent block in extreme cases |
| **GitHub** | Primary 429 (Retry-After: 60s) → secondary rate limit (Retry-After: longer) → 403 temporary → account flagging |
| **AWS** | ThrottlingException → RequestLimitExceeded → SdkException (retries exhausted) |
| **Twitter/X** | 429 with x-rate-limit-reset → 403 for continued violations → app suspension |
| **Reddit** | 429 "you are doing that too much" → 403 → shadow ban (quota silently set to 0) |

### 5.3 Catcher Implication

**Catcher must NEVER ignore Retry-After**. Key rules:
1. `Retry-After` is a **minimum** wait, not a suggestion
2. After 3 consecutive 429s on the same endpoint, **escalate to longer backoff** even if Retry-After remains short
3. Track per-endpoint 429 frequency to detect "soft bans"
4. On 403 following a series of 429s, treat as escalated rate limit (not auth failure)
5. Implement a **circuit breaker**: after N 429s in time window T, pause ALL requests to that host for duration D

---

## 6. Android Data Saver & iOS Low Data Mode

### 6.1 Feature Comparison

| Aspect | Android Data Saver | iOS Low Data Mode |
|--------|-------------------|-------------------|
| **Introduced** | Android 7.0 (2016) | iOS 13 (2019) |
| **Scope** | Device-wide (all apps) | Per-network (cellular + per-WiFi SSID) |
| **User control** | On/Off toggle in Settings | Per-network toggle |
| **Background data** | Blocked entirely | Reduced/paused |
| **Foreground data** | Apps should reduce quality | Apps should reduce quality |
| **App detection** | `ConnectivityManager.isActiveNetworkMetered` + `RestrictBackgroundStatus` | `URLSessionConfiguration.allowsCellularAccess` + `isConstrained` |
| **HTTP/HTTPS effect** | Background requests blocked | Background requests throttled |

### 6.2 Adoption Estimates

| Market | Android Data Saver | iOS Low Data Mode |
|--------|-------------------|-------------------|
| **Global estimate** | ~15–25% of Android users (no official Google stat) | ~5–15% of iOS users (no official Apple stat) |
| **Developing markets** | 30–40% (small data plans, 500MB–2GB/month) | 20–30% |
| **Developed markets** | 5–10% (unlimited plans common) | 3–8% |
| **Mobile OS market share** | Android 67.35%, iOS 32.55% (StatCounter 2026) | |

**Key note**: No official adoption statistics are published by Google or Apple. These are estimated from industry surveys, mobile data plan distributions, and app developer telemetry. The actual figures are closely guarded.

### 6.3 Impact on HTTP Traffic

| Behavior | Android Data Saver | iOS Low Data Mode |
|----------|-------------------|-------------------|
| **Background fetch** | Blocked | Paused/deferred |
| **WebSocket connections** | Dropped when app backgrounds | May be throttled |
| **Streaming** | Reduced quality | Reduced quality |
| **Prefetch/preload** | Disabled | Reduced |
| **Automatic downloads** | Blocked | Paused |
| **TCP keepalive** | Normal | May be reduced |
| **DNS resolution** | Normal | Normal |

### 6.4 Catcher Implication

**Catcher running on mobile (via FFI/UniFFI) must handle**:
1. **WebSocket disconnection on app background** — not a network error, but a platform policy
2. **Increased latency** — Low Data Mode may throttle TCP windows
3. **Connection establishment delays** — background restrictions may defer DNS/connect
4. **Detection API**: Expose `is_metered_network()` and `is_low_data_mode()` via FFI

For non-mobile platforms, Data Saver/Low Data Mode is irrelevant.

---

## 7. Enterprise Proxy Authentication (407)

### 7.1 When 407 Appears

407 `Proxy Authentication Required` occurs when:
1. Client is behind a **forward proxy** (explicit proxy, not transparent)
2. Proxy requires authentication before forwarding the request
3. `Proxy-Authenticate` header in response specifies the auth scheme

### 7.2 Auth Schemes

| Scheme | Handshake | Prevalence |
|--------|-----------|------------|
| **Basic** | Single request (credentials in `Proxy-Authorization` header) | Legacy, requires TLS |
| **Digest** | Challenge-response, no plaintext password | Rare, mostly in older systems |
| **NTLM** | Multi-leg (3 messages: Type 1/2/3) | Dominant in Windows enterprise (despite deprecation) |
| **Negotiate** (Kerberos) | SPNEGO token exchange | Modern Windows enterprise, SSO integration |
| **Negotiate** (NTLM fallback) | Kerberos preferred, NTLM fallback | Most common enterprise setup |

### 7.3 NTLM Handshake Detail

```
Client → Proxy: GET /api/endpoint HTTP/1.1
Proxy → Client: HTTP/1.1 407 Proxy Authentication Required
                 Proxy-Authenticate: NTLM
                 
Client → Proxy: GET /api/endpoint HTTP/1.1
                 Proxy-Authorization: NTLM <Type-1: negotiate>
                 
Proxy → Client: HTTP/1.1 407 Proxy Authentication Required
                 Proxy-Authenticate: NTLM <Type-2: challenge>
                 
Client → Proxy: GET /api/endpoint HTTP/1.1
                 Proxy-Authorization: NTLM <Type-3: authenticate>
                 
Proxy → Origin: GET /api/endpoint HTTP/1.1  (forwarded)
Origin → Proxy → Client: HTTP/1.1 200 OK
```

**Connection reuse**: After successful NTLM auth, the connection is authenticated. Subsequent requests on the same connection skip the handshake. New connections must re-authenticate (but Kerberos can use cached tickets).

### 7.4 Real-World Proxy Penetration

- **35%** of global online users are behind proxies (2024, up from 26% in 2018)
- **>78%** of Fortune 500 companies use proxy networks
- Enterprise proxy market growing at 9.8% CAGR ($4.8B → $11.2B by 2034)
- **NTLM remains unexpectedly common** due to legacy Windows infrastructure, despite Microsoft's deprecation push

### 7.5 Catcher Implication

**407 is a hard, non-retryable failure** — but only if Catcher lacks proxy auth support. For Catcher's scope:
1. **Detect 407** and surface as `CatcherError::ProxyAuthRequired` with the `Proxy-Authenticate` header
2. **Do not retry** unless proxy credentials are explicitly configured
3. **Future consideration**: NTLM/Kerberos support would require multi-leg HTTP handshake state machine — significant complexity, low ROI for an initial release
4. **Practical approach**: Catcher should allow users to configure a proxy (host:port + auth scheme + credentials) via `CatcherConfig`, and handle the 407 handshake transparently when configured
5. **Platform proxy detection**: On Windows, use system proxy settings; on macOS, use System Configuration framework proxy settings; on Linux, respect `HTTP_PROXY` / `HTTPS_PROXY` environment variables

---

## 8. Captive Portal Detection Across OS

### 8.1 Detection Endpoints

| Platform | Probe URL(s) | Expected Response | Method |
|----------|-------------|-------------------|--------|
| **iOS** | `http://captive.apple.com/hotspot-detect.html` | `Success` (plain text) | HTTP GET via CNA Helper |
| **macOS** | `http://captive.apple.com/hotspot-detect.html` | `Success` | HTTP GET |
| **Android** (Google) | `http://clients3.google.com/generate_204` | HTTP 204 No Content | HTTP GET |
| **Android** (Google alt) | `http://connectivitycheck.gstatic.com/generate_204` | HTTP 204 | HTTP GET |
| **Android** (Samsung) | `http://connectivitycheck.samsung.com/generate_204` | HTTP 204 | HTTP GET |
| **Windows** | `http://www.msftncsi.com/ncsi.txt` | `Microsoft NCSI` | HTTP GET |
| **Windows** (DNS) | DNS lookup of `dns.msftncsi.com` | Must resolve to `131.107.255.255` | DNS A record |
| **Chrome** (browser) | Uses OS-level detection + `http://clients3.google.com/generate_204` | HTTP 204 | Fallback check |

### 8.2 Detection Sequence (All Platforms)

```
1. Device associates with Wi-Fi AP
2. Device obtains IP via DHCP
3. Device sends HTTP GET to probe URL
4. If expected response received → no portal, full internet access
5. If redirected (HTTP 302/307) or HTML/JSON response → captive portal detected
6. OS displays captive portal UI:
   - iOS: CPMB (Captive Portal Mini-Browser) as a WebSheet overlay
   - Android: Push notification → opens Chrome Custom Tab
   - macOS: Mini-browser popup (Safari-based)
   - Windows: Manual browser redirect (no built-in mini-browser)
7. User authenticates with portal → portal allows traffic → OS re-probes
8. Probe succeeds → OS closes CPMB, connection fully established
```

### 8.3 Key Behaviors Relevant to Catcher

| Behavior | Detail |
|----------|--------|
| **CPMB cookies** | Destroyed on close — no persistent state |
| **External services** | Not accessible from CPMB (sandboxed) |
| **Focus loss** | On iOS, switching to another app disconnects from captive Wi-Fi |
| **Re-detection** | After authentication, OS re-probes on each URL navigation (iOS), or periodically |
| **Known SSID bypass** | iOS may disable captive detection for previously-used open networks |
| **VPN interference** | VPN can block captive detection probes → no CPMB appears |
| **HTTPS portals** | Broken for most OS implementations (probe is HTTP, redirect to HTTPS fails detection) |
| **Timeout** | Typically 10–30 seconds before OS marks connection as "no internet" |
| **Fallback** | If probe URL is blocked/rerouted, OS may incorrectly detect captive portal |

### 8.4 Catcher Implication

**Captive portals cause a unique failure mode**: the connection succeeds at the TCP/TLS level but HTTP requests are intercepted and redirected. Catcher will see:
- **302 redirect** to a captive portal login page instead of the expected API response
- **Non-API response body** (HTML login form instead of JSON)
- **Connection succeeds, requests fail semantically**

Catcher should:
1. **Detect captive portal redirects**: If a 302 redirect goes to a non-API host with HTML content, surface as `CatcherError::CaptivePortal`
2. **Detect probe URL spoofing**: If connected to Wi-Fi but HTTP requests return unexpected HTML, flag as possible captive portal
3. **Platform-specific**: On mobile (FFI/UniFFI targets), can call OS captive portal detection API directly
4. **Do NOT retry**: Captive portals require user action (authentication), not automatic retry

---

## 9. Consolidated Implications for Catcher

### 9.1 New Error Types to Add

```rust
pub enum CatcherError {
    // Existing variants...
    
    // ── Policy / Rate Limit Errors ──
    /// Rate limit exceeded (HTTP 429). Contains Retry-After duration if available.
    RateLimited { retry_after: Option<Duration>, limit_reset: Option<SystemTime> },
    
    /// Retry budget exhausted. Too many retries in a short window.
    RetryBudgetExhausted { budget_remaining: f64, time_until_refill: Duration },
    
    /// CDN/WAF challenge page detected (non-API HTML response)
    CdnChallenge { body_preview: String },
    
    /// Proxy authentication required (HTTP 407)
    ProxyAuthRequired { auth_schemes: Vec<String> },
    
    /// Captive portal detected (HTTP 302/200 with non-API content)
    CaptivePortal { redirect_url: Option<String> },
    
    /// Rate limit escalation — 403 after repeated 429s
    RateLimitBan { reason: String },
}
```

### 9.2 Retry Budget Parameters (Recommended Defaults)

| Parameter | Value | Rationale |
|-----------|-------|-----------|
| `retry_budget_capacity` | 500 tokens | Matches AWS SDK default |
| `retry_budget_cost_429` | 10 tokens | Between AWS transient (14) and throttle (5) |
| `retry_budget_cost_5xx` | 5 tokens | Same as AWS throttle cost |
| `retry_budget_cost_timeout` | 8 tokens | Mid-range — timeouts suggest real issues |
| `retry_budget_replenish_rate` | 1 token/sec | Recovers fully in ~8 minutes |
| `retry_budget_cooldown_after_exhaust` | 30 seconds | Pause all retries when budget empty |

### 9.3 Rate Limit Response Protocol

```
On HTTP 429:
  1. Parse Retry-After header → minimum_wait
  2. Parse X-RateLimit-Reset / RateLimit-Reset → window_reset
  3. If minimum_wait exists: wait_at_least(minimum_wait)
  4. If window_reset exists: wait_until(window_reset)
  5. If neither exists: exponential_backoff(attempt_number, base=1s, cap=60s)
  6. Deduct retry_budget_cost_429 from budget
  7. If budget depleted: return RetryBudgetExhausted

On HTTP 403:
  1. Check if preceded by 429s (escalation) → treat as RateLimitBan
  2. Otherwise → treat as auth failure (non-retryable)

On HTTP 407:
  1. If proxy credentials configured → perform proxy auth handshake
  2. Otherwise → return ProxyAuthRequired (non-retryable)

On non-JSON response (HTML CDN challenge):
  1. Check content-type header
  2. Check for known CDN challenge patterns (Cloudflare, Fastly, AWS WAF)
  3. Return CdnChallenge error
```

### 9.4 Mobile-Specific Considerations

```
Platform detection APIs to expose via FFI/UniFFI:
  - is_metered_network() → bool (Android: ConnectivityManager.isActiveNetworkMetered)
  - is_low_data_mode() → bool (iOS: URLSessionConfiguration.isConstrained)
  - is_captive_portal() → bool (OS-level detection)
  - network_type() → enum { Wifi, Cellular, Ethernet, Unknown }

WebSocket behavior:
  - On mobile background: gracefully close with code 1001 (Going Away)
  - Auto-reconnect when app foregrounds (if configured)
  - Respect Data Saver / Low Data Mode: reduce reconnect frequency
```

### 9.5 Header Parsing Priority

| Priority | Header(s) | Action |
|----------|-----------|--------|
| **1** | `Retry-After` | Authoritative wait duration (seconds or HTTP-date) |
| **2** | `X-RateLimit-Remaining: 0` + `X-RateLimit-Reset` | Compute wait = reset - now |
| **3** | `RateLimit-Remaining: 0` + `RateLimit-Reset` | Same as above (IETF standard) |
| **4** | None | Exponential backoff with full jitter |

### 9.6 CDN Challenge Pattern Detection

```
Detect when 403/429 response body is HTML instead of JSON:
  1. Content-Type: text/html (not application/json)
  2. Body contains known strings:
     - Cloudflare: "cf-browser-verify", "_cf_chl_opt", "challenge-platform"
     - Fastly: "Fastly edge", "rate limited"
     - AWS WAF: "Request blocked", "WAF"
     - Generic: "<html" + "captcha" or "challenge"
  3. Surface as CdnChallenge, do not retry automatically
```

### 9.7 Key Numbers Summary

| Metric | Value | Source |
|--------|-------|--------|
| Global users behind proxies | 35% | Statista 2024 |
| Fortune 500 proxy usage | >78% | Market research |
| Proxy market CAGR | 9.8% | Dataintelo |
| Android global share | 67.35% | StatCounter 2026 |
| iOS global share | 32.55% | StatCounter 2026 |
| Est. Data Saver adoption (Android) | 15–25% | Industry estimate |
| Est. Low Data Mode adoption (iOS) | 5–15% | Industry estimate |
| AWS retry budget capacity | 500 tokens | AWS SDK docs |
| AWS 429 base delay | 1,000 ms | AWS SDK 2026 update |
| AWS transient base delay | 50 ms | AWS SDK 2026 update |
| Google adaptive throttling K | 2.0 | Google SRE book |
| Cloudflare rate limit accuracy | 99.997% | Cloudflare blog |
| GitHub primary rate limit | 5,000/hr (auth), 60/hr (unauth) | GitHub docs |
| GitHub secondary concurrent limit | 100 concurrent | GitHub docs |
| GitHub secondary REST points | 900 pts/min | GitHub docs |
| AWS WAF eval windows | 60s/120s/300s/600s | AWS docs |
| AWS WAF min threshold | 100 req/window | AWS docs |
| Fastly ratecounter capacity | 200,000 entries | Fastly docs |
| CDN rate limit response | 429 (most common) or 403 | Observed |
| Captive portal probe timeout | 10–30 seconds | WBA |
| 407 NTLM handshake | 3-leg (2 round-trips) | Protocol spec |

---

## 10. References

- IETF RateLimit Headers draft: https://datatracker.ietf.org/doc/draft-ietf-httpapi-ratelimit-headers/
- Cloudflare Rate Limiting architecture: https://blog.cloudflare.com/counting-things-a-lot-of-different-things/
- AWS SDK Retry behavior: https://docs.aws.amazon.com/sdkref/latest/guide/feature-retry-behavior.html
- AWS SDK 2026 retry update: https://aws.amazon.com/blogs/developer/announcing-updated-retry-behavior-for-aws-sdks-and-tools/
- Google SRE Handling Overload: https://sre.google/sre-book/handling-overload/
- GitHub REST API rate limits: https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api
- Fastly Edge Rate Limiting: https://docs.fastly.com/products/edge-rate-limiting
- AWS WAF rate-based rules: https://docs.aws.amazon.com/waf/latest/developerguide/waf-rule-statement-type-rate-based.html
- WBA Captive Portal Behavior: https://captivebehavior.wballiance.com/
- Captive portal detection reference: https://www.purple.ai/en-gb/guides/apple-cna-android-connectivity-check-microsoft-ncsi-how-captive-portal-detection-actually-works
- Android Data Saver: https://source.android.com/docs/core/data/data-saver
- iOS Low Data Mode: https://support.apple.com/en-us/102433
