use std::{
    collections::HashMap,
    fmt,
    sync::{Arc, Mutex},
};

use crate::{Revision, Surface, ViewId};

/// Authoritative server-side state and its Telegram projection target.
#[derive(Clone, Debug, PartialEq)]
pub struct ViewRecord<S> {
    /// Logical view identity.
    pub id: ViewId,
    /// Monotonic state revision.
    pub revision: Revision,
    /// Application-owned state.
    pub state: S,
    /// Projection target associated with the view.
    pub surface: Surface,
}

/// Storage errors that preserve the optimistic-concurrency failure class.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StoreError {
    /// The view id is already present.
    AlreadyExists(ViewId),
    /// The view id is not present.
    NotFound(ViewId),
    /// Another action committed a newer revision first.
    Conflict {
        view_id: ViewId,
        expected: Revision,
        actual: Revision,
    },
    /// The monotonic revision counter cannot advance further.
    RevisionExhausted(ViewId),
}

impl fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyExists(view_id) => write!(formatter, "view {view_id} already exists"),
            Self::NotFound(view_id) => write!(formatter, "view {view_id} was not found"),
            Self::Conflict {
                view_id,
                expected,
                actual,
            } => write!(
                formatter,
                "view {view_id} revision conflict: expected {expected}, actual {actual}"
            ),
            Self::RevisionExhausted(view_id) => {
                write!(formatter, "view {view_id} revision exhausted")
            }
        }
    }
}

impl std::error::Error for StoreError {}

/// Minimal synchronous store contract used by the MVP runtime.
pub trait UiStore<S>: Send + Sync {
    /// Inserts a new view record.
    fn insert(&self, record: ViewRecord<S>) -> Result<(), StoreError>;

    /// Loads a snapshot without holding a lock across caller work.
    fn load(&self, view_id: ViewId) -> Result<Option<ViewRecord<S>>, StoreError>;

    /// Commits `new_state` only if `expected_revision` is still current.
    fn compare_and_set(
        &self,
        view_id: ViewId,
        expected_revision: Revision,
        new_state: S,
    ) -> Result<ViewRecord<S>, StoreError>;

    /// Removes a view record.
    fn remove(&self, view_id: ViewId) -> Result<Option<ViewRecord<S>>, StoreError>;
}

/// Thread-safe in-memory `UiStore` for the first runtime and tests.
#[derive(Clone, Debug)]
pub struct InMemoryUiStore<S> {
    records: Arc<Mutex<HashMap<ViewId, ViewRecord<S>>>>,
}

impl<S> Default for InMemoryUiStore<S> {
    fn default() -> Self {
        Self {
            records: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl<S> InMemoryUiStore<S> {
    /// Creates an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<ViewId, ViewRecord<S>>> {
        self.records
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl<S> UiStore<S> for InMemoryUiStore<S>
where
    S: Clone + Send + Sync,
{
    fn insert(&self, record: ViewRecord<S>) -> Result<(), StoreError> {
        let mut records = self.lock();
        if records.contains_key(&record.id) {
            return Err(StoreError::AlreadyExists(record.id));
        }
        records.insert(record.id, record);
        Ok(())
    }

    fn load(&self, view_id: ViewId) -> Result<Option<ViewRecord<S>>, StoreError> {
        Ok(self.lock().get(&view_id).cloned())
    }

    fn compare_and_set(
        &self,
        view_id: ViewId,
        expected_revision: Revision,
        new_state: S,
    ) -> Result<ViewRecord<S>, StoreError> {
        let mut records = self.lock();
        let Some(record) = records.get_mut(&view_id) else {
            return Err(StoreError::NotFound(view_id));
        };
        if record.revision != expected_revision {
            return Err(StoreError::Conflict {
                view_id,
                expected: expected_revision,
                actual: record.revision,
            });
        }
        let Some(next_revision) = record.revision.checked_next() else {
            return Err(StoreError::RevisionExhausted(view_id));
        };
        record.revision = next_revision;
        record.state = new_state;
        Ok(record.clone())
    }

    fn remove(&self, view_id: ViewId) -> Result<Option<ViewRecord<S>>, StoreError> {
        Ok(self.lock().remove(&view_id))
    }
}

#[cfg(test)]
mod tests {
    use super::{InMemoryUiStore, StoreError, UiStore, ViewRecord};
    use crate::{Revision, Surface, ViewId};
    use teloxide::types::{ChatId, MessageId};

    fn record(value: i32) -> ViewRecord<i32> {
        ViewRecord {
            id: ViewId::new(1),
            revision: Revision::INITIAL,
            state: value,
            surface: Surface::Message {
                chat_id: ChatId(1),
                message_id: MessageId(2),
            },
        }
    }

    #[test]
    fn compare_and_set_advances_revision_and_preserves_surface() {
        let store = InMemoryUiStore::new();
        store.insert(record(0)).unwrap();
        let updated = store
            .compare_and_set(ViewId::new(1), Revision::INITIAL, 1)
            .unwrap();
        assert_eq!(updated.revision, Revision::new(1));
        assert_eq!(updated.state, 1);
        assert_eq!(store.load(ViewId::new(1)).unwrap(), Some(updated));
    }

    #[test]
    fn stale_compare_and_set_is_a_conflict() {
        let store = InMemoryUiStore::new();
        store.insert(record(0)).unwrap();
        store
            .compare_and_set(ViewId::new(1), Revision::INITIAL, 1)
            .unwrap();
        assert_eq!(
            store.compare_and_set(ViewId::new(1), Revision::INITIAL, 2),
            Err(StoreError::Conflict {
                view_id: ViewId::new(1),
                expected: Revision::INITIAL,
                actual: Revision::new(1),
            })
        );
    }
}
