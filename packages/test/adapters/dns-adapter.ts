/**
 * DNS cache adapter for Rust napi bindings.
 *
 * Since Rust uses hickory-resolver for DNS caching in-process,
 * clearing the DNS cache requires a dedicated napi call.
 *
 * For now, this is a stub — the Rust DNS resolver is managed
 * internally by reqwest's hickory-dns feature.
 */
export function clearDnsCache(): void {
  // TODO: when @eric8810/napi-http exposes a clear_dns_cache() function,
  // call it here. For now, DNS caching in Rust is handled transparently
  // by reqwest's connection pooling.
}
