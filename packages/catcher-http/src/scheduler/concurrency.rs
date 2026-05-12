use catcher_core::types::observability::NetworkQualityLevel;

/// 根据网络质量返回建议的并发数
pub fn concurrency_for_quality(level: NetworkQualityLevel) -> usize {
    match level {
        NetworkQualityLevel::Excellent => 50,
        NetworkQualityLevel::Good => 25,
        NetworkQualityLevel::Fair => 10,
        NetworkQualityLevel::Poor => 5,
        NetworkQualityLevel::Bad => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excellent_returns_50() {
        assert_eq!(concurrency_for_quality(NetworkQualityLevel::Excellent), 50);
    }

    #[test]
    fn bad_returns_2() {
        assert_eq!(concurrency_for_quality(NetworkQualityLevel::Bad), 2);
    }

    #[test]
    fn all_levels_have_non_zero_concurrency() {
        for level in [
            NetworkQualityLevel::Excellent,
            NetworkQualityLevel::Good,
            NetworkQualityLevel::Fair,
            NetworkQualityLevel::Poor,
            NetworkQualityLevel::Bad,
        ] {
            assert!(concurrency_for_quality(level) > 0);
        }
    }
}
