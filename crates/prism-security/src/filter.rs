use bytes::Bytes;

/// Content filter trait. Implemented by clipboard/notification filters in Phase 3.
/// Defined in Phase 1 so the SecurityContext can hold `Arc<dyn ContentFilter>`.
pub trait ContentFilter: Send + Sync {
    fn filter(&self, data: &[u8]) -> FilterResult;
    fn description(&self) -> &str;
}

#[derive(Debug, Clone)]
pub enum FilterResult {
    Allow,
    Redact(Bytes),
    Block,
    Confirm(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct TestFilter;

    impl ContentFilter for TestFilter {
        fn filter(&self, data: &[u8]) -> FilterResult {
            if data.windows(6).any(|w| w == b"secret") {
                FilterResult::Block
            } else {
                FilterResult::Allow
            }
        }
        fn description(&self) -> &str { "test filter" }
    }

    #[test]
    fn content_filter_trait_is_object_safe() {
        let filter: Arc<dyn ContentFilter> = Arc::new(TestFilter);
        assert!(matches!(filter.filter(b"hello"), FilterResult::Allow));
        assert!(matches!(filter.filter(b"my secret"), FilterResult::Block));
        assert_eq!(filter.description(), "test filter");
    }
}
