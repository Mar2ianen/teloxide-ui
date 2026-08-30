//! Declarative, server-driven UI primitives for Telegram bots.
//!
//! This crate is experimental. It provides the first state/action/rendering
//! runtime layer while keeping Telegram transport and scheduling in teloxide.
//!
//! The core invariant is:
//!
//! ```text
//! State → Ui<Action>
//! ```
//!
//! Authoritative state lives on the server. Telegram callback data identifies
//! actions; it does not contain authoritative application state.

#![forbid(unsafe_code)]

mod action;
mod component;
mod effect;
mod ids;
mod node;
mod render;
mod store;
mod surface;

pub use action::{
    ActionRecord, ActionRegistry, ActionResolveError, ActionToken, ActorPolicy, StalePolicy,
    TokenFormatError,
};
pub use component::{Component, ViewCx};
pub use effect::Effect;
pub use ids::{Revision, ViewId};
pub use node::{
    Button, ButtonLabel, ButtonRow, ButtonStyle, Details, Table, TableCell, Ui, UiNode,
};
pub use render::{RenderContext, RenderError, RenderedRichMessage, RichRenderer};
pub use store::{InMemoryUiStore, StoreError, UiStore, ViewRecord};
pub use surface::{
    ProjectionError, ProjectionReceipt, QueueRequest, ReplacementError, Surface, SurfaceWorker,
};
