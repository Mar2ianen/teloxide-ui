use std::{
    fmt,
    sync::atomic::{AtomicU64, Ordering},
};

/// Identity of one logical server-side UI view.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ViewId(u64);

impl ViewId {
    /// Creates an id from a value that can be persisted by an application.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Allocates a process-local id for examples and in-memory applications.
    #[must_use]
    pub fn fresh() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let value = NEXT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1)
            })
            .expect("ViewId allocator exhausted");
        Self(value)
    }

    /// Returns the numeric representation.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for ViewId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "view:{}", self.0)
    }
}

/// Monotonic logical version of a view state.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Revision(u64);

impl Revision {
    /// The first revision of a newly-created view.
    pub const INITIAL: Self = Self(0);

    /// Creates a revision from a persisted value.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the numeric representation.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Advances the revision, returning `None` on numeric exhaustion.
    #[must_use]
    pub const fn checked_next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
}

impl fmt::Display for Revision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[cfg(test)]
mod tests {
    use super::{Revision, ViewId};

    #[test]
    fn fresh_view_ids_are_distinct() {
        assert_ne!(ViewId::fresh(), ViewId::fresh());
    }

    #[test]
    fn revisions_are_monotonic_until_exhaustion() {
        let initial = Revision::INITIAL;
        assert_eq!(initial.checked_next(), Some(Revision::new(1)));
        assert_eq!(Revision::new(u64::MAX).checked_next(), None);
    }
}
