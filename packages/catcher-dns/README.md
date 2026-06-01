# catcher-dns

[![crates.io](https://img.shields.io/crates/v/catcher-dns.svg)](https://crates.io/crates/catcher-dns)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

Shared DNS resolver for the [catcher](https://github.com/eric8810/catcher) toolkit.

`catcher-dns` is used by `catcher-http` and `catcher-ws` so both clients share the same DNS config shape and behavior.

## Features

- DNS cache with configurable cache size and TTL
- Negative cache TTL
- Stale fallback when DNS refresh fails
- Custom nameservers
- Host mapping for fixed hostname to IP rules

## Usage

```toml
[dependencies]
catcher-dns = "0.3.11"
```

```rust
use catcher_dns::{build_stale_aware_resolver, DnsConfig};

let config = DnsConfig {
    cache_size: 512,
    cache_ttl_secs: 300,
    negative_ttl_secs: 60,
    stale_ttl_secs: 3600,
    stale_on_error: true,
    nameservers: vec!["8.8.8.8:53".into()],
    host_mapping: Default::default(),
};

let resolver = build_stale_aware_resolver(&config)?;
let addrs = resolver.resolve_socket_addrs("example.com", 443).await?;
```

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `hickory-dns` | Yes | Use hickory-resolver with cache and stale fallback |

When `hickory-dns` is disabled, the resolver uses system DNS plus `host_mapping`. Cache and stale fallback are not enabled in that mode.

## License

MIT
