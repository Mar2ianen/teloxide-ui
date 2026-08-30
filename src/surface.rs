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

/// Failure while replacing a regular message with a freshly sent Rich
/// Message. The replacement receipt is retained when cleanup of the old
/// message fails, because the new message is already the authoritative
/// projection target in that case.
#[derive(Debug)]
pub enum ReplacementError<E> {
    /// The new Rich Message could not be sent.
    Send(ProjectionError<E>),
    /// The new message exists, but deleting the old one failed.
    Delete {
        /// The new surface that callers must retain.
        receipt: ProjectionReceipt,
        /// The cleanup failure.
        source: ProjectionError<E>,
    },
    /// Replacement was requested for a surface that has no regular message
    /// identity.
    Unsupported(Surface),
}

impl<E: fmt::Display> fmt::Display for ReplacementError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Send(error) => write!(formatter, "replacement send failed: {error}"),
            Self::Delete { source, .. } => {
                write!(formatter, "replacement cleanup failed: {source}")
            }
            Self::Unsupported(surface) => {
                write!(
                    formatter,
                    "surface replacement is unsupported for {surface:?}"
                )
            }
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for ReplacementError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Send(error) => Some(error),
            Self::Delete { source, .. } => Some(source),
            Self::Unsupported(_) => None,
        }
    }
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
        trace_surface("edit", &surface, revision);
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

    /// Replaces a regular message with a newly sent Rich Message.
    ///
    /// Telegram currently accepts `rich_message` in `editMessageText`, but
    /// normalizes custom-emoji objects to their fallback text while editing.
    /// A chess board (and any other projection that relies on custom emoji)
    /// therefore has to be sent as a new Rich Message. The old message is
    /// deleted only after the replacement has been admitted and sent.
    ///
    /// Both requests are enqueued before the first one starts, so the shared
    /// serial lane keeps the send/delete pair contiguous with respect to
    /// later projections. No application state lock is held during either
    /// network request.
    pub async fn replace_message(
        &self,
        surface: Surface,
        revision: Revision,
        rich_message: InputRichMessage,
    ) -> Result<ProjectionReceipt, ReplacementError<B::Err>>
    where
        B::SendRichMessage: QueueRequest<Err = B::Err>,
        B::DeleteMessage: QueueRequest<Err = B::Err>,
    {
        let Surface::Message {
            chat_id,
            message_id,
        } = surface.clone()
        else {
            return Err(ReplacementError::Unsupported(surface));
        };

        let (lane, coalesce_key) = self.lane_for(&surface);
        trace_surface("replace/send", &surface, revision);
        let send_request = self.bot.send_rich_message(chat_id, rich_message);
        let delete_request = self.bot.delete_message(chat_id, message_id);

        // Enqueue both parts of the replacement before awaiting either one.
        // This prevents another projection from being inserted between the
        // send and the cleanup request on this surface lane.
        let send_metadata = send_request.outbound_metadata();
        let delete_metadata = delete_request.outbound_metadata();
        let send_scope = send_metadata.scope.clone();
        let delete_scope = delete_metadata.scope.clone();
        let send_acquire = lane.acquire_latest_wins(send_metadata, coalesce_key);
        let delete_acquire = lane.acquire(delete_metadata);

        let mut send_permit = send_acquire
            .await
            .map_err(|error| ReplacementError::Send(ProjectionError::Queue(error)))?;
        send_permit.start();
        let send_result = send_request.send().await;
        let send_outcome = match &send_result {
            Ok(_) => OutboundCompletion::Success,
            Err(error) => match <B::SendRichMessage as QueueRequest>::retry_after_duration(error) {
                Some(duration) => OutboundCompletion::RetryAfter {
                    scope: send_scope,
                    duration,
                },
                None => OutboundCompletion::Failed,
            },
        };
        send_permit.complete_and_await(send_outcome).await;
        let sent = send_result
            .map_err(|error| ReplacementError::Send(ProjectionError::Transport(error)))?;

        let replacement = ProjectionReceipt {
            surface: Surface::Message {
                chat_id,
                message_id: sent.id,
            },
            revision,
        };
        trace_surface("replace/delete", &replacement.surface, revision);

        let mut delete_permit = delete_acquire
            .await
            .map_err(|error| ReplacementError::Delete {
                receipt: replacement.clone(),
                source: ProjectionError::Queue(error),
            })?;
        delete_permit.start();
        let delete_result = delete_request.send().await;
        let delete_outcome = match &delete_result {
            Ok(_) => OutboundCompletion::Success,
            Err(error) => match <B::DeleteMessage as QueueRequest>::retry_after_duration(error) {
                Some(duration) => OutboundCompletion::RetryAfter {
                    scope: delete_scope,
                    duration,
                },
                None => OutboundCompletion::Failed,
            },
        };
        delete_permit.complete_and_await(delete_outcome).await;
        self.transfer_lane(&surface, &replacement.surface);
        delete_result.map_err(|error| ReplacementError::Delete {
            receipt: replacement.clone(),
            source: ProjectionError::Transport(error),
        })?;
        Ok(replacement)
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

    fn transfer_lane(&self, old_surface: &Surface, new_surface: &Surface) {
        let mut lanes = self
            .lanes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(entry) = lanes.remove(old_surface) {
            lanes.insert(new_surface.clone(), entry);
        }
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

fn trace_surface(operation: &str, surface: &Surface, revision: Revision) {
    if std::env::var_os("TELOXIDE_UI_TRACE").is_some() {
        eprintln!("[teloxide-ui] {operation} surface={surface:?} revision={revision}");
    }
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
