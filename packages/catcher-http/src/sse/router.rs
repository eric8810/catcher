/// Line routing result
pub enum RouteAction {
    /// Content line — yield to user
    Yield(String),
    /// Control line — silently consumed (heartbeat / empty line)
    Silent,
    /// `id:` line — record last_event_id for reconnect
    SetLastEventId(String),
    /// `retry:` line — adjust reconnect interval
    SetRetry(u64),
}

/// Route a single SSE text line.
pub fn route_line(line: &str) -> RouteAction {
    if line.is_empty() {
        return RouteAction::Silent;
    }
    if line.starts_with(':') {
        return RouteAction::Silent;
    }
    if let Some(id) = line.strip_prefix("id:") {
        return RouteAction::SetLastEventId(id.trim_start().to_string());
    }
    if let Some(retry) = line.strip_prefix("retry:") {
        if let Ok(ms) = retry.trim().parse::<u64>() {
            return RouteAction::SetRetry(ms);
        }
    }
    // All other lines (data:, event:, etc.) are yielded as-is
    RouteAction::Yield(line.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── 1.1 控制行 → Silent ────────────────────────────────

    #[test]
    fn test_01_empty_line_silent() {
        assert!(matches!(route_line(""), RouteAction::Silent));
    }

    #[test]
    fn test_02_keepalive_silent() {
        assert!(matches!(route_line(": keepalive"), RouteAction::Silent));
    }

    #[test]
    fn test_03_comment_silent() {
        assert!(matches!(route_line(": this is a comment"), RouteAction::Silent));
    }

    #[test]
    fn test_04_colon_only_silent() {
        assert!(matches!(route_line(":"), RouteAction::Silent));
    }

    // ── 1.2 id: 行 → SetLastEventId ────────────────────────

    #[test]
    fn test_05_id_standard() {
        assert!(matches!(route_line("id: msg_001"), RouteAction::SetLastEventId(id) if id == "msg_001"));
    }

    #[test]
    fn test_06_id_no_space() {
        assert!(matches!(route_line("id:msg_002"), RouteAction::SetLastEventId(id) if id == "msg_002"));
    }

    #[test]
    fn test_07_id_multi_space() {
        assert!(matches!(route_line("id:  multi  space"), RouteAction::SetLastEventId(id) if id == "multi  space"));
    }

    #[test]
    fn test_08_id_empty() {
        assert!(matches!(route_line("id:"), RouteAction::SetLastEventId(id) if id == ""));
    }

    #[test]
    fn test_09_id_numeric() {
        assert!(matches!(route_line("id: 42"), RouteAction::SetLastEventId(id) if id == "42"));
    }

    // ── 1.3 retry: 行 → SetRetry or Yield ──────────────────

    #[test]
    fn test_10_retry_standard() {
        assert!(matches!(route_line("retry: 5000"), RouteAction::SetRetry(5000)));
    }

    #[test]
    fn test_11_retry_no_space() {
        assert!(matches!(route_line("retry:1000"), RouteAction::SetRetry(1000)));
    }

    #[test]
    fn test_12_retry_non_numeric() {
        assert!(matches!(route_line("retry: abc"), RouteAction::Yield(_)));
    }

    #[test]
    fn test_13_retry_negative() {
        // u64 can't parse negative, so it yields
        assert!(matches!(route_line("retry: -1"), RouteAction::Yield(_)));
    }

    #[test]
    fn test_14_retry_zero() {
        assert!(matches!(route_line("retry: 0"), RouteAction::SetRetry(0)));
    }

    // ── 1.4 内容行 → Yield 原样 ────────────────────────────

    #[test]
    fn test_15_data_hello() {
        assert!(matches!(route_line("data: Hello"), RouteAction::Yield(l) if l == "data: Hello"));
    }

    #[test]
    fn test_16_data_json() {
        assert!(matches!(route_line(r#"data: {"type":"start"}"#), RouteAction::Yield(l) if l == r#"data: {"type":"start"}"#));
    }

    #[test]
    fn test_17_data_double_space() {
        assert!(matches!(route_line("data:  world"), RouteAction::Yield(l) if l == "data:  world"));
    }

    #[test]
    fn test_18_data_done() {
        assert!(matches!(route_line("data: [DONE]"), RouteAction::Yield(l) if l == "data: [DONE]"));
    }

    #[test]
    fn test_19_event() {
        assert!(matches!(route_line("event: message_start"), RouteAction::Yield(l) if l == "event: message_start"));
    }

    #[test]
    fn test_20_data_empty() {
        assert!(matches!(route_line("data:"), RouteAction::Yield(l) if l == "data:"));
    }

    #[test]
    fn test_21_custom_prefix() {
        assert!(matches!(route_line("custom: value"), RouteAction::Yield(l) if l == "custom: value"));
    }

    #[test]
    fn test_22_just_text() {
        assert!(matches!(route_line("just text"), RouteAction::Yield(l) if l == "just text"));
    }

    #[test]
    fn test_23_uppercase_id() {
        assert!(matches!(route_line("ID: uppercase"), RouteAction::Yield(l) if l == "ID: uppercase"));
    }

    #[test]
    fn test_24_space_only() {
        assert!(matches!(route_line(" "), RouteAction::Yield(l) if l == " "));
    }
}
