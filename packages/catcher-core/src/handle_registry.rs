//! 泛型 FFI Handle 注册表
//!
//! 为 HTTP / WS / SSE 等 FFI 模块提供统一的 handle 生命周期管理。
//! 使用 `RwLock` 替代 `Mutex`，允许多线程并发读取 handle。

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

/// 泛型 FFI handle 注册表。
///
/// 内部使用 `RwLock<HashMap>` 实现读写分离：
/// - `get()` 使用读锁，多线程可并发
/// - `insert()` / `remove()` 使用写锁
pub struct HandleRegistry<T> {
    map: OnceLock<RwLock<HashMap<usize, Arc<T>>>>,
    next_id: AtomicUsize,
}

impl<T> Default for HandleRegistry<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> HandleRegistry<T> {
    /// 创建空注册表。
    pub const fn new() -> Self {
        Self {
            map: OnceLock::new(),
            next_id: AtomicUsize::new(1),
        }
    }

    fn map(&self) -> &RwLock<HashMap<usize, Arc<T>>> {
        self.map.get_or_init(|| RwLock::new(HashMap::new()))
    }

    /// 分配下一个 ID（不插入）。
    pub fn next_id(&self) -> usize {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    /// 分配新 ID 并插入 handle，返回分配的 ID。
    pub fn insert(&self, value: Arc<T>) -> usize {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.map().write().unwrap().insert(id, value);
        id
    }

    /// 按 ID 插入 handle（使用预先分配的 ID）。
    pub fn insert_with_id(&self, id: usize, value: Arc<T>) {
        self.map().write().unwrap().insert(id, value);
    }

    /// 按 ID 获取 handle 的 Arc 引用。
    pub fn get(&self, id: usize) -> Option<Arc<T>> {
        self.map().read().unwrap().get(&id).cloned()
    }

    /// 按 ID 移除 handle，返回被移除的 Arc（如有）。
    pub fn remove(&self, id: usize) -> Option<Arc<T>> {
        self.map().write().unwrap().remove(&id)
    }
}

// ── 单元测试 ──

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    #[test]
    fn insert_and_get() {
        static REG: HandleRegistry<AtomicU32> = HandleRegistry::new();
        let id = REG.insert(Arc::new(AtomicU32::new(42)));
        let val = REG.get(id).unwrap();
        assert_eq!(val.load(Ordering::Relaxed), 42);
    }

    #[test]
    fn remove_returns_value() {
        static REG: HandleRegistry<AtomicU32> = HandleRegistry::new();
        let id = REG.insert(Arc::new(AtomicU32::new(99)));
        let removed = REG.remove(id).unwrap();
        assert_eq!(removed.load(Ordering::Relaxed), 99);
        assert!(REG.get(id).is_none());
    }

    #[test]
    fn get_unknown_returns_none() {
        static REG: HandleRegistry<AtomicU32> = HandleRegistry::new();
        assert!(REG.get(99999).is_none());
    }

    #[test]
    fn ids_are_unique() {
        static REG: HandleRegistry<AtomicU32> = HandleRegistry::new();
        let id1 = REG.insert(Arc::new(AtomicU32::new(1)));
        let id2 = REG.insert(Arc::new(AtomicU32::new(2)));
        assert_ne!(id1, id2);
    }
}
