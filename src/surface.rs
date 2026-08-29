use std::{
    collections::HashMap,
    fmt,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use teloxide::{
    errors::AsResponseParameters,
    payloads::{
        EditEphemeralMessageTextSetters, EditMessageTextInlineSetters, EditMessageTextSetters,
    },
    requests::{HasPayload, Payload, Request, Requester},
    types::{ChatId, InputRichMessage, MessageId, UserId},
};

use crate::Revision;
use teloxide::outbound::{
    OutboundAcquireError, OutboundCompletion, OutboundLane, OutboundMetadata, OutboundPayload,
    OutboundQueue,
};

/// Independently addressable Telegram UI projection target.
///
/// A surface is also the network-ordering boundary: mutations targeting the
/// same surface must be serialized by the runtime.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Surface {
    Message {
        chat_id: ChatId,
        message_id: MessageId,
    },
    Inline {
        inline_message_id: String,
    },
    Ephemeral {
        chat_id: ChatId,
        receiver_user_id: UserId,
        ephemeral_message_id: i32,
    },
}

/// Result of a successful projection admission and Telegram request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectionReceipt {
    /// Surface that was updated.
    pub surface: Surface,
    /// Revision sent to Telegram.
    pub revision: Revision,
}

/// Failure from queue admission or the underlying Telegram request.
#[derive(Debug)]
pub enum ProjectionError<E> {
    /// The shared teloxide scheduler did not grant the projection.
    Queue(OutboundAcquireError),
    /// Telegram rejected or failed the request after admission.
    Transport(E),
}

impl<E: fmt::Display> fmt::Display for ProjectionError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Queue(error) => write!(formatter, "surface projection was not admitted: {error}"),
            Self::Transport(error) => write!(formatter, "surface projection failed: {error}"),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for ProjectionError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Queue(error) => Some(error),
            Self::Transport(error) => Some(error),
        }
    }
}

/// A request shape that can be admitted through teloxide's shared queue.
///
/// This is implemented for teloxide requests automatically. It deliberately
/// exposes no UI concepts and exists only to keep the worker's generic bounds
/// readable.
pub trait QueueRequest: Request + HasPayload + Clone + Send + 'static {
    /// Computes queue metadata from the final request payload.
    fn outbound_metadata(&self) -> OutboundMetadata;

    /// Extracts a Telegram retry penalty without exposing the error type to
    /// the worker implementation.
    fn retry_after_duration(error: &Self::Err) -> Option<Duration>;
}

impl<R> QueueRequest for R
where
    R: Request + HasPayload + Clone + Send + 'static,
    R::Err: AsResponseParameters,
    R::Payload: Payload<Output: Send> + OutboundPayload,
{
    fn outbound_metadata(&self) -> OutboundMetadata {
        let hint = self.payload_ref().outbound_hint();
        OutboundMetadata {
            scope: hint.scope,
            class: hint.class,
            priority: hint.priority,
            weight: hint.weight,
        }
    }

    fn retry_after_duration(error: &Self::Err) -> Option<Duration> {
        error.retry_after().map(|seconds| seconds.duration())
    }
}

#[derive(Debug)]
struct SurfaceLane {
    lane: OutboundLane,
    coalesce_key: u64,
}

/// Serial per-surface renderer backed by teloxide's `OutboundQueue`.
///
/// Each surface gets one scheduler lane. A pending render for that surface is
/// latest-wins; once a Telegram request has started, it is allowed to finish
/// and the next successful revision follows it through the same lane.
#[derive(Clone, Debug)]
pub struct SurfaceWorker<B> {
    bot: B,
    queue: OutboundQueue,
    lanes: Arc<Mutex<HashMap<Surface, SurfaceLane>>>,
    next_coalesce_key: Arc<AtomicU64>,
}

impl<B> SurfaceWorker<B>
where
    B: Requester + Clone + Send + Sync + 'static,
{
    /// Creates a worker using an already-running shared outbound queue.
    #[must_use]
    pub fn new(bot: B, queue: OutboundQueue) -> Self {
        Self {
            bot,
            queue,
            lanes: Arc::new(Mutex::new(HashMap::new())),
            next_coalesce_key: Arc::new(AtomicU64::new(1)),
        }
    }

    /// Projects a complete Rich Message representation onto one surface.
    ///
    /// `Superseded` is reported as a queue error for an older pending render;
    /// callers may safely ignore that result because a newer render owns the
    /// desired projection.
    pub async fn project(
        &self,
        surface: Surface,
        revision: Revision,
        rich_message: InputRichMessage,
    ) -> Result<ProjectionReceipt, ProjectionError<B::Err>>
    where
        B::EditMessageText: QueueRequest<Err = B::Err>,
        B::EditMessageTextInline: QueueRequest<Err = B::Err>,
        B::EditEphemeralMessageText: QueueRequest<Err = B::Err>,
    {
        let (lane, coalesce_key) = self.lane_for(&surface);
        match surface.clone() {
            Surface::Message {
                chat_id,
                message_id,
            } => {
                let request = self
                    .bot
                    .edit_message_text(chat_id, message_id, String::new())
                    .rich_message(rich_message);
                execute_latest(request, lane, coalesce_key).await?;
            }
            Surface::Inline { inline_message_id } => {
                let request = self
                    .bot
                    .edit_message_text_inline(inline_message_id, String::new())
                    .rich_message(rich_message);
                execute_latest(request, lane, coalesce_key).await?;
            }
            Surface::Ephemeral {
                chat_id,
                receiver_user_id,
                ephemeral_message_id,
            } => {
                let request = self
                    .bot
                    .edit_ephemeral_message_text(chat_id, receiver_user_id, ephemeral_message_id)
                    .rich_message(rich_message);
                execute_latest(request, lane, coalesce_key).await?;
            }
        }
        Ok(ProjectionReceipt { surface, revision })
    }

    fn lane_for(&self, surface: &Surface) -> (OutboundLane, u64) {
        let mut lanes = self
            .lanes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let entry = lanes.entry(surface.clone()).or_insert_with(|| SurfaceLane {
            lane: self.queue.handle().serial_lane(),
            coalesce_key: self.next_coalesce_key.fetch_add(1, Ordering::Relaxed),
        });
        (entry.lane.clone(), entry.coalesce_key)
    }
}

async fn execute_latest<R>(
    request: R,
    lane: OutboundLane,
    coalesce_key: u64,
) -> Result<(), ProjectionError<R::Err>>
where
    R: QueueRequest,
{
    let metadata = request.outbound_metadata();
    let scope = metadata.scope.clone();
    let mut permit = lane
        .acquire_latest_wins(metadata, coalesce_key)
        .await
        .map_err(ProjectionError::Queue)?;
    permit.start();
    let result = request.send().await;
    match &result {
        Ok(_) => permit.complete_and_await(OutboundCompletion::Success).await,
        Err(error) => {
            if let Some(duration) = R::retry_after_duration(error) {
                permit
                    .complete_and_await(OutboundCompletion::RetryAfter { scope, duration })
                    .await;
            } else {
                permit.complete_and_await(OutboundCompletion::Failed).await;
            }
        }
    }
    result.map_err(ProjectionError::Transport).map(|_| ())
}

#[cfg(test)]
mod tests {
    use teloxide::{
        outbound::{OutboundQueue, OutboundSettings},
        Bot,
    };

    use super::{Surface, SurfaceWorker};

    #[tokio::test]
    async fn one_surface_has_one_serial_lane_and_coalesce_slot() {
        let queue = OutboundQueue::new_spawn(OutboundSettings::default()).unwrap();
        let worker = SurfaceWorker::new(Bot::new("token"), queue);
        let surface = Surface::Message {
            chat_id: teloxide::types::ChatId(1),
            message_id: teloxide::types::MessageId(2),
        };
        let (_, first_key) = worker.lane_for(&surface);
        let (_, second_key) = worker.lane_for(&surface);
        assert_eq!(first_key, second_key);
        assert_eq!(worker.lanes.lock().unwrap().len(), 1);

        let other = Surface::Inline {
            inline_message_id: "inline".to_owned(),
        };
        let (_, other_key) = worker.lane_for(&other);
        assert_ne!(first_key, other_key);
        assert_eq!(worker.lanes.lock().unwrap().len(), 2);
    }
}
