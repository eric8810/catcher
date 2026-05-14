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
