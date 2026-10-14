//! 仅测试编译启用的原生 Sink 闸门，保持真实握手与底层传输。
use futures_util::task::AtomicWaker;
use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    OnceLock,
};
use std::task::Context;

#[derive(Default)]
struct Gate {
    paused: AtomicBool,
    pending: AtomicBool,
    cancelled: AtomicBool,
    calls_after_cancel: AtomicUsize,
    waker: AtomicWaker,
}

fn gate() -> &'static Gate {
    static GATE: OnceLock<Gate> = OnceLock::new();
    GATE.get_or_init(Gate::default)
}

/// 测试开始时暂停 flush；测试结束时释放并清理闸门。
pub fn set_paused(paused: bool) {
    let state = gate();
    if paused {
        state.calls_after_cancel.store(0, Ordering::Relaxed);
        state.cancelled.store(false, Ordering::Release);
        state.pending.store(false, Ordering::Release);
        state.paused.store(true, Ordering::Release);
    } else {
        state.paused.store(false, Ordering::Release);
        state.waker.wake();
        state.pending.store(false, Ordering::Release);
        state.cancelled.store(false, Ordering::Release);
    }
}

/// 释放 flush，但保留观测值以检查取消后的迟到调用。
pub fn release_flush() {
    gate().paused.store(false, Ordering::Release);
    gate().waker.wake();
}

/// 在同步 close 返回后记录关闭请求；网络断言同时覆盖方法调用期间的窗口。
pub fn mark_close_requested() {
    let state = gate();
    if state.paused.load(Ordering::Acquire) && state.pending.load(Ordering::Acquire) {
        state.cancelled.store(true, Ordering::Release);
    }
}

pub(crate) fn record_sink_call() {
    let state = gate();
    if state.cancelled.load(Ordering::Acquire) {
        state.calls_after_cancel.fetch_add(1, Ordering::Relaxed);
    }
}

pub(crate) fn flush_is_pending(cx: &Context<'_>) -> bool {
    let state = gate();
    state.waker.register(cx.waker());
    if state.paused.load(Ordering::Acquire) {
        state.pending.store(true, Ordering::Release);
        true
    } else {
        false
    }
}

/// 确认原生 flush 已返回 Pending，并读取取消后的 Sink 调用计数。
#[derive(Serialize)]
pub struct Snapshot {
    /// 是否已观察到原生 flush 返回 Pending。
    pub pending: bool,
    /// 是否已收到关闭请求。
    pub cancelled: bool,
    /// 取消后再次 poll/start_send 的总次数，必须为零。
    pub calls_after_cancel: usize,
}

/// 获取测试观测值，不改变闸门状态。
pub fn snapshot() -> Snapshot {
    let state = gate();
    Snapshot {
        pending: state.pending.load(Ordering::Acquire),
        cancelled: state.cancelled.load(Ordering::Acquire),
        calls_after_cancel: state.calls_after_cancel.load(Ordering::Relaxed),
    }
}
