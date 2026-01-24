use lasso::{RodeoResolver, Spur, ThreadedRodeo};
use std::sync::Arc;

/// Thread-safe string interning pool for cell text values.
/// Uses lasso's ThreadedRodeo for concurrent intern operations,
/// then converts to RodeoResolver for lock-free reads.
pub struct StringPool {
    inner: StringPoolInner,
}

enum StringPoolInner {
    /// Mutable phase - can intern new strings
    Building(ThreadedRodeo),
    /// Immutable phase - lock-free reads only
    Frozen(Arc<RodeoResolver>),
}

impl StringPool {
    /// Create a new string pool in building mode
    pub fn new() -> Self {
        Self {
            inner: StringPoolInner::Building(ThreadedRodeo::default()),
        }
    }

    /// Intern a string, returning its token. Only works in building mode.
    pub fn intern(&self, s: &str) -> Option<Spur> {
        match &self.inner {
            StringPoolInner::Building(rodeo) => Some(rodeo.get_or_intern(s)),
            StringPoolInner::Frozen(_) => None,
        }
    }

    /// Get token for existing string without interning
    pub fn get(&self, s: &str) -> Option<Spur> {
        match &self.inner {
            StringPoolInner::Building(rodeo) => rodeo.get(s),
            StringPoolInner::Frozen(_resolver) => {
                // RodeoResolver doesn't support reverse lookup
                // This is a limitation - consider keeping a separate HashMap if needed
                None
            }
        }
    }

    /// Resolve a token back to its string
    pub fn resolve(&self, key: Spur) -> Option<&str> {
        match &self.inner {
            StringPoolInner::Building(rodeo) => rodeo.try_resolve(&key),
            StringPoolInner::Frozen(resolver) => resolver.try_resolve(&key),
        }
    }

    /// Freeze the pool for read-only access (better performance)
    pub fn freeze(&mut self) {
        if let StringPoolInner::Building(rodeo) = std::mem::replace(
            &mut self.inner,
            StringPoolInner::Building(ThreadedRodeo::default()),
        ) {
            let resolver = rodeo.into_resolver();
            self.inner = StringPoolInner::Frozen(Arc::new(resolver));
        }
    }

    /// Check if pool is frozen
    pub fn is_frozen(&self) -> bool {
        matches!(self.inner, StringPoolInner::Frozen(_))
    }

    /// Number of interned strings
    pub fn len(&self) -> usize {
        match &self.inner {
            StringPoolInner::Building(rodeo) => rodeo.len(),
            StringPoolInner::Frozen(resolver) => resolver.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for StringPool {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for StringPool {
    fn clone(&self) -> Self {
        match &self.inner {
            StringPoolInner::Building(_) => {
                // Can't efficiently clone a ThreadedRodeo, create new empty one
                Self::new()
            }
            StringPoolInner::Frozen(resolver) => Self {
                inner: StringPoolInner::Frozen(Arc::clone(resolver)),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intern_and_resolve() {
        let pool = StringPool::new();
        let key = pool.intern("hello").unwrap();
        assert_eq!(pool.resolve(key), Some("hello"));
    }

    #[test]
    fn test_deduplication() {
        let pool = StringPool::new();
        let k1 = pool.intern("hello").unwrap();
        let k2 = pool.intern("hello").unwrap();
        assert_eq!(k1, k2);
        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn test_freeze() {
        let mut pool = StringPool::new();
        let key = pool.intern("test").unwrap();
        pool.freeze();
        assert!(pool.is_frozen());
        assert_eq!(pool.resolve(key), Some("test"));
        assert!(pool.intern("new").is_none());
    }
}
