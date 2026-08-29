use std::marker::PhantomData;

use crate::Ui;

/// Pure server-side view component.
///
/// Implementations should treat `view` as a deterministic projection from
/// state into semantic UI. Side effects belong to the transition/runtime
/// layer, not here.
pub trait Component {
    type State;
    type Action: Clone + Send + Sync + 'static;

    fn view(&self, state: &Self::State, cx: &ViewCx<Self::Action>) -> Ui<Self::Action>;
}

/// Context available while building one view.
///
/// The initial skeleton intentionally carries no mutable runtime handle.
/// Renderer/runtime capabilities should not leak into pure application views.
#[derive(Clone, Copy, Debug, Default)]
pub struct ViewCx<A> {
    _action: PhantomData<fn() -> A>,
}

impl<A> ViewCx<A> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            _action: PhantomData,
        }
    }
}
