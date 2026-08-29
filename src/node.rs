/// Semantic UI tree.
///
/// The tree models Telegram-level semantics. It is not a DOM and does not
/// promise arbitrary subtree patching or pixel layout.
#[derive(Clone, Debug, PartialEq)]
pub struct Ui<A> {
    pub nodes: Vec<UiNode<A>>,
}

impl<A> Ui<A> {
    #[must_use]
    pub const fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    #[must_use]
    pub fn column() -> Self {
        Self::new()
    }

    #[must_use]
    pub fn push(mut self, node: impl Into<UiNode<A>>) -> Self {
        self.nodes.push(node.into());
        self
    }

    #[must_use]
    pub fn text(text: impl Into<String>) -> UiNode<A> {
        UiNode::Text(text.into())
    }

    #[must_use]
    pub fn paragraph(text: impl Into<String>) -> UiNode<A> {
        UiNode::Paragraph(text.into())
    }

    #[must_use]
    pub fn heading(text: impl Into<String>) -> UiNode<A> {
        UiNode::Heading(text.into())
    }

    #[must_use]
    pub fn button(text: impl Into<String>, action: A) -> Button<A> {
        Button::new(text, action)
    }

    #[must_use]
    pub const fn button_row() -> ButtonRow<A> {
        ButtonRow {
            buttons: Vec::new(),
        }
    }

    /// Creates a fragment whose children are flattened by a renderer.
    #[must_use]
    pub fn fragment(nodes: impl IntoIterator<Item = UiNode<A>>) -> UiNode<A> {
        UiNode::Fragment(nodes.into_iter().collect())
    }
}

impl<A> Default for Ui<A> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum UiNode<A> {
    Text(String),
    Paragraph(String),
    Heading(String),
    ButtonRow(ButtonRow<A>),
    Fragment(Vec<UiNode<A>>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ButtonRow<A> {
    pub buttons: Vec<Button<A>>,
}

impl<A> ButtonRow<A> {
    #[must_use]
    pub fn push(mut self, button: Button<A>) -> Self {
        self.buttons.push(button);
        self
    }
}

impl<A> From<ButtonRow<A>> for UiNode<A> {
    fn from(value: ButtonRow<A>) -> Self {
        Self::ButtonRow(value)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Button<A> {
    pub text: String,
    pub action: A,
    pub style: ButtonStyle,
    pub disabled: bool,
}

impl<A> Button<A> {
    #[must_use]
    pub fn new(text: impl Into<String>, action: A) -> Self {
        Self {
            text: text.into(),
            action,
            style: ButtonStyle::Default,
            disabled: false,
        }
    }

    #[must_use]
    pub const fn style(mut self, style: ButtonStyle) -> Self {
        self.style = style;
        self
    }

    #[must_use]
    pub const fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ButtonStyle {
    #[default]
    Default,
    Primary,
    Success,
    Danger,
    Link,
}
