use crate::{Surface, Ui};

/// Explicit work requested by an application transition.
///
/// This enum is intentionally small in the initial skeleton. Drafter-backed
/// streaming and richer target policies will be added only after the ordinary
/// render path is proven.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect<A> {
    Render(Ui<A>),
    RenderAt { surface: Surface, ui: Ui<A> },
    Send(Ui<A>),
    Delete(Surface),
    Toast(String),
    Alert(String),
}
