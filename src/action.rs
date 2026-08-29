use std::{
    collections::HashMap,
    fmt,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use teloxide::types::UserId;
use uuid::Uuid;

use crate::{Revision, ViewId};

const TOKEN_PREFIX: &str = "tu1:";
const MAX_CALLBACK_BYTES: usize = 64;

/// Versioned opaque capability sent as Telegram `callback_data`.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ActionToken(String);

impl ActionToken {
    /// Parses and validates a token received from Telegram.
    pub fn new(value: impl Into<String>) -> Result<Self, TokenFormatError> {
        let value = value.into();
        if value.len() > MAX_CALLBACK_BYTES || value.len() < TOKEN_PREFIX.len() + 1 {
            return Err(TokenFormatError::Length);
        }
        if !value.starts_with(TOKEN_PREFIX)
            || !value[TOKEN_PREFIX.len()..]
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(TokenFormatError::Syntax);
        }
        Ok(Self(value))
    }

    /// Returns the exact callback payload.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn generated() -> Self {
        Self(format!("{TOKEN_PREFIX}{}", Uuid::new_v4().simple()))
    }
}

impl AsRef<str> for ActionToken {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for ActionToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl TryFrom<&str> for ActionToken {
    type Error = TokenFormatError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<String> for ActionToken {
    type Error = TokenFormatError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

/// Why a callback token was rejected before dispatch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TokenFormatError {
    /// The token is empty, too long, or exceeds Telegram's callback limit.
    Length,
    /// The token does not use the versioned `tu1:` syntax.
    Syntax,
}

impl fmt::Display for TokenFormatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Length => formatter.write_str("action token has an invalid length"),
            Self::Syntax => formatter.write_str("action token has invalid syntax"),
        }
    }
}

impl std::error::Error for TokenFormatError {}

/// Actor constraint attached to a stateful action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActorPolicy {
    /// Any Telegram user may invoke the action.
    Any,
    /// Only this user may invoke the action.
    User(UserId),
}

/// Policy for an action whose token refers to an older view revision.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum StalePolicy {
    /// Reject the action unless it refers to the current revision.
    #[default]
    Reject,
    /// Resolve the action against the latest state. The transition must make
    /// that interpretation safe for the application.
    ApplyToLatest,
    /// Permit an older token only for an explicitly idempotent action.
    Idempotent,
}

impl StalePolicy {
    fn accepts(self, token_revision: Revision, current_revision: Revision) -> bool {
        match self {
            Self::Reject => token_revision == current_revision,
            Self::ApplyToLatest | Self::Idempotent => token_revision <= current_revision,
        }
    }
}

/// Server-side metadata associated with one callback capability.
#[derive(Clone, Debug, PartialEq)]
pub struct ActionRecord<A> {
    /// The Telegram callback payload.
    pub token: ActionToken,
    /// Logical view to which this action belongs.
    pub view_id: ViewId,
    /// Revision from which the action was rendered.
    pub revision: Revision,
    /// Typed application action.
    pub action: A,
    /// Actor restriction.
    pub actor: ActorPolicy,
    /// Stale-revision behavior.
    pub stale_policy: StalePolicy,
    /// Absolute expiry, if this action has a TTL.
    pub expires_at: Option<Instant>,
}

/// Reason a syntactically valid callback could not be resolved.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionResolveError {
    /// The token is unknown or was removed.
    Unknown,
    /// The token existed but its TTL elapsed.
    Expired,
    /// The callback came from a different actor.
    ActorMismatch { expected: UserId, actual: UserId },
    /// The token targets another logical view.
    ViewMismatch { expected: ViewId, actual: ViewId },
    /// The token is older than the current view and its policy rejects it.
    Stale {
        token_revision: Revision,
        current_revision: Revision,
    },
}

impl fmt::Display for ActionResolveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown => formatter.write_str("unknown action token"),
            Self::Expired => formatter.write_str("expired action token"),
            Self::ActorMismatch { .. } => {
                formatter.write_str("action token is bound to another actor")
            }
            Self::ViewMismatch { .. } => formatter.write_str("action token targets another view"),
            Self::Stale { .. } => formatter.write_str("action token refers to a stale revision"),
        }
    }
}

impl std::error::Error for ActionResolveError {}

struct RegistryState<A> {
    entries: HashMap<ActionToken, ActionRecord<A>>,
}

/// In-memory registry for opaque callback capabilities.
#[derive(Clone)]
pub struct ActionRegistry<A> {
    state: Arc<Mutex<RegistryState<A>>>,
    default_ttl: Option<Duration>,
}

impl<A> fmt::Debug for ActionRegistry<A> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ActionRegistry")
            .field("default_ttl", &self.default_ttl)
            .finish_non_exhaustive()
    }
}

impl<A> ActionRegistry<A> {
    /// Creates a registry with non-expiring entries.
    #[must_use]
    pub fn new() -> Self {
        Self::with_default_ttl(None)
    }

    /// Creates a registry whose new entries expire after `ttl`.
    #[must_use]
    pub fn with_default_ttl(ttl: Option<Duration>) -> Self {
        Self {
            state: Arc::new(Mutex::new(RegistryState {
                entries: HashMap::new(),
            })),
            default_ttl: ttl,
        }
    }

    /// Registers a typed action and returns its compact callback token.
    pub fn register(
        &self,
        view_id: ViewId,
        revision: Revision,
        action: A,
        actor: ActorPolicy,
        stale_policy: StalePolicy,
    ) -> ActionToken {
        self.register_with_ttl(
            view_id,
            revision,
            action,
            actor,
            stale_policy,
            self.default_ttl,
        )
    }

    /// Registers an action with an entry-specific TTL.
    pub fn register_with_ttl(
        &self,
        view_id: ViewId,
        revision: Revision,
        action: A,
        actor: ActorPolicy,
        stale_policy: StalePolicy,
        ttl: Option<Duration>,
    ) -> ActionToken {
        let expires_at = ttl.map(|duration| Instant::now() + duration);
        let mut state = self.lock();
        let token = loop {
            let token = ActionToken::generated();
            if !state.entries.contains_key(&token) {
                break token;
            }
        };
        state.entries.insert(
            token.clone(),
            ActionRecord {
                token: token.clone(),
                view_id,
                revision,
                action,
                actor,
                stale_policy,
                expires_at,
            },
        );
        token
    }

    /// Resolves a callback after validating actor, view, expiry, and staleness.
    pub fn resolve(
        &self,
        token: &ActionToken,
        actor: UserId,
        view_id: ViewId,
        current_revision: Revision,
    ) -> Result<ActionRecord<A>, ActionResolveError>
    where
        A: Clone,
    {
        let mut state = self.lock();
        let Some(record) = state.entries.get(token).cloned() else {
            return Err(ActionResolveError::Unknown);
        };
        if record
            .expires_at
            .is_some_and(|expires_at| Instant::now() >= expires_at)
        {
            state.entries.remove(token);
            return Err(ActionResolveError::Expired);
        }
        if let ActorPolicy::User(expected) = record.actor {
            if expected != actor {
                return Err(ActionResolveError::ActorMismatch {
                    expected,
                    actual: actor,
                });
            }
        }
        if record.view_id != view_id {
            return Err(ActionResolveError::ViewMismatch {
                expected: record.view_id,
                actual: view_id,
            });
        }
        if !record
            .stale_policy
            .accepts(record.revision, current_revision)
        {
            return Err(ActionResolveError::Stale {
                token_revision: record.revision,
                current_revision,
            });
        }
        Ok(record)
    }

    /// Removes one token, for example after a one-shot action is committed.
    pub fn remove(&self, token: &ActionToken) -> Option<ActionRecord<A>> {
        self.lock().entries.remove(token)
    }

    /// Drops all expired entries and returns the number removed.
    pub fn prune_expired(&self) -> usize {
        let now = Instant::now();
        let mut state = self.lock();
        let before = state.entries.len();
        state
            .entries
            .retain(|_, record| record.expires_at.is_none_or(|expires_at| expires_at > now));
        before - state.entries.len()
    }

    /// Returns the number of registered entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lock().entries.len()
    }

    /// Returns whether no action tokens are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, RegistryState<A>> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl<A> Default for ActionRegistry<A> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use teloxide::types::UserId;

    use super::{ActionRegistry, ActionResolveError, ActorPolicy, StalePolicy};
    use crate::{Revision, ViewId};

    #[test]
    fn generated_tokens_are_versioned_and_within_telegram_limit() {
        let registry = ActionRegistry::new();
        let token = registry.register(
            ViewId::new(1),
            Revision::new(2),
            "increment",
            ActorPolicy::Any,
            StalePolicy::Reject,
        );
        assert!(token.as_str().starts_with("tu1:"));
        assert!(token.as_str().len() <= 64);
        assert!(super::ActionToken::try_from(token.as_str()).is_ok());
    }

    #[test]
    fn malformed_tokens_are_rejected_before_registry_lookup() {
        assert_eq!(
            super::ActionToken::try_from("callback:raw"),
            Err(super::TokenFormatError::Syntax)
        );
        assert_eq!(
            super::ActionToken::try_from("tu1:contains.dot"),
            Err(super::TokenFormatError::Syntax)
        );
        assert_eq!(
            super::ActionToken::try_from("tu1:"),
            Err(super::TokenFormatError::Length)
        );
    }

    #[test]
    fn actor_and_view_are_validated_before_dispatch() {
        let registry = ActionRegistry::new();
        let token = registry.register(
            ViewId::new(7),
            Revision::INITIAL,
            42,
            ActorPolicy::User(UserId(11)),
            StalePolicy::Reject,
        );
        assert!(matches!(
            registry.resolve(&token, UserId(12), ViewId::new(7), Revision::INITIAL),
            Err(ActionResolveError::ActorMismatch { .. })
        ));
        assert!(matches!(
            registry.resolve(&token, UserId(11), ViewId::new(8), Revision::INITIAL),
            Err(ActionResolveError::ViewMismatch { .. })
        ));
    }

    #[test]
    fn stale_reject_is_default_but_apply_to_latest_is_explicit() {
        let registry = ActionRegistry::new();
        let rejected = registry.register(
            ViewId::new(1),
            Revision::new(1),
            1,
            ActorPolicy::Any,
            StalePolicy::Reject,
        );
        assert!(matches!(
            registry.resolve(&rejected, UserId(1), ViewId::new(1), Revision::new(2)),
            Err(ActionResolveError::Stale { .. })
        ));

        let accepted = registry.register(
            ViewId::new(1),
            Revision::new(1),
            2,
            ActorPolicy::Any,
            StalePolicy::ApplyToLatest,
        );
        assert_eq!(
            registry
                .resolve(&accepted, UserId(1), ViewId::new(1), Revision::new(2))
                .unwrap()
                .action,
            2
        );

        let future = registry.register(
            ViewId::new(1),
            Revision::new(3),
            3,
            ActorPolicy::Any,
            StalePolicy::ApplyToLatest,
        );
        assert_eq!(
            registry.resolve(&future, UserId(1), ViewId::new(1), Revision::new(2)),
            Err(ActionResolveError::Stale {
                token_revision: Revision::new(3),
                current_revision: Revision::new(2),
            })
        );
    }

    #[test]
    fn expired_token_is_removed_on_resolution() {
        let registry = ActionRegistry::new();
        let token = registry.register_with_ttl(
            ViewId::new(1),
            Revision::INITIAL,
            (),
            ActorPolicy::Any,
            StalePolicy::Reject,
            Some(Duration::ZERO),
        );
        assert_eq!(
            registry.resolve(&token, UserId(1), ViewId::new(1), Revision::INITIAL),
            Err(ActionResolveError::Expired)
        );
        assert!(registry.is_empty());
    }
}
