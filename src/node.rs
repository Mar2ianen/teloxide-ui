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

    /// Creates a semantic blockquote/callout.
    #[must_use]
    pub fn blockquote(text: impl Into<String>) -> UiNode<A> {
        UiNode::Blockquote(text.into())
    }

    /// Creates a collapsible semantic details block.
    #[must_use]
    pub fn details(
        summary: impl Into<String>,
        blocks: impl IntoIterator<Item = UiNode<A>>,
    ) -> Details<A> {
        Details {
            summary: summary.into(),
            blocks: blocks.into_iter().collect(),
            is_open: false,
        }
    }

    #[must_use]
    pub fn button(text: impl Into<ButtonLabel>, action: A) -> Button<A> {
        Button::new(text, action)
    }

    #[must_use]
    pub const fn button_row() -> ButtonRow<A> {
        ButtonRow {
            buttons: Vec::new(),
        }
    }

    /// Creates a semantic table.
    #[must_use]
    pub const fn table() -> Table<A> {
        Table::new()
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
    Blockquote(String),
    Details(Details<A>),
    ButtonRow(ButtonRow<A>),
    Table(Table<A>),
    Fragment(Vec<UiNode<A>>),
}

/// A collapsible semantic details block.
#[derive(Clone, Debug, PartialEq)]
pub struct Details<A> {
    /// Always-visible summary text.
    pub summary: String,
    /// Semantic content revealed by the client.
    pub blocks: Vec<UiNode<A>>,
    /// Whether the details block should start expanded.
    pub is_open: bool,
}

impl<A> Details<A> {
    /// Sets whether the details block starts expanded.
    #[must_use]
    pub const fn open(mut self, is_open: bool) -> Self {
        self.is_open = is_open;
        self
    }

    /// Appends a semantic child block.
    #[must_use]
    pub fn push(mut self, block: impl Into<UiNode<A>>) -> Self {
        self.blocks.push(block.into());
        self
    }
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

/// A semantic table whose cells may contain text or interactive buttons.
///
/// Tables are useful for compact, grid-like Rich Message surfaces. They do
/// not promise pixel layout or browser-style cell measurement.
#[derive(Clone, Debug, PartialEq)]
pub struct Table<A> {
    /// Rows and cells in semantic display order.
    pub rows: Vec<Vec<TableCell<A>>>,
    /// Whether Telegram should draw borders around the table.
    pub is_bordered: Option<bool>,
    /// Whether Telegram should apply its striped table treatment.
    pub is_striped: Option<bool>,
    /// Whether Telegram should use its compact table treatment.
    pub is_compact: Option<bool>,
}

impl<A> Table<A> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            rows: Vec::new(),
            is_bordered: None,
            is_striped: None,
            is_compact: None,
        }
    }

    /// Appends one row to the table.
    #[must_use]
    pub fn row(mut self, cells: impl IntoIterator<Item = TableCell<A>>) -> Self {
        self.rows.push(cells.into_iter().collect());
        self
    }

    /// Sets Telegram's bordered-table option.
    #[must_use]
    pub const fn bordered(mut self, bordered: bool) -> Self {
        self.is_bordered = Some(bordered);
        self
    }

    /// Sets Telegram's striped-table option.
    #[must_use]
    pub const fn striped(mut self, striped: bool) -> Self {
        self.is_striped = Some(striped);
        self
    }

    /// Sets Telegram's compact-table option.
    #[must_use]
    pub const fn compact(mut self, compact: bool) -> Self {
        self.is_compact = Some(compact);
        self
    }
}

impl<A> Default for Table<A> {
    fn default() -> Self {
        Self::new()
    }
}

/// Content of a semantic table cell.
#[derive(Clone, Debug, PartialEq)]
pub enum TableCell<A> {
    /// An intentionally invisible cell, useful for coordinate gutters.
    Empty,
    /// Plain text centered by the Rich Message renderer.
    Text(String),
    /// An interactive cell button.
    Button(Button<A>),
}

impl<A> TableCell<A> {
    /// Creates an invisible cell.
    #[must_use]
    pub const fn empty() -> Self {
        Self::Empty
    }

    /// Creates a plain-text cell.
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text(text.into())
    }

    /// Creates an interactive cell button.
    #[must_use]
    pub fn button(text: impl Into<ButtonLabel>, action: A) -> Self {
        Self::Button(Button::new(text, action))
    }

    /// Applies a native Telegram button style when this is a button cell.
    #[must_use]
    pub fn style(self, style: ButtonStyle) -> Self {
        match self {
            Self::Button(button) => Self::Button(button.style(style)),
            cell => cell,
        }
    }

    /// Disables this cell button when this is a button cell.
    #[must_use]
    pub fn disabled(self, disabled: bool) -> Self {
        match self {
            Self::Button(button) => Self::Button(button.disabled(disabled)),
            cell => cell,
        }
    }
}

impl<A> From<ButtonRow<A>> for UiNode<A> {
    fn from(value: ButtonRow<A>) -> Self {
        Self::ButtonRow(value)
    }
}

impl<A> From<Table<A>> for UiNode<A> {
    fn from(value: Table<A>) -> Self {
        Self::Table(value)
    }
}

impl<A> From<Details<A>> for UiNode<A> {
    fn from(value: Details<A>) -> Self {
        Self::Details(value)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Button<A> {
    pub text: ButtonLabel,
    pub action: A,
    pub style: ButtonStyle,
    pub disabled: bool,
}

impl<A> Button<A> {
    #[must_use]
    pub fn new(text: impl Into<ButtonLabel>, action: A) -> Self {
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

/// Semantic content for a button label.
///
/// `CustomEmoji` keeps the Telegram-specific identifier at the rendering
/// boundary while allowing applications to use a text fallback when the
/// target surface does not support custom emoji.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ButtonLabel {
    /// Ordinary text rendered as a rich-text string.
    Plain(String),
    /// A Telegram custom emoji and its required accessibility fallback.
    CustomEmoji {
        custom_emoji_id: String,
        alternative_text: String,
    },
}

impl ButtonLabel {
    /// Creates a custom-emoji label.
    #[must_use]
    pub fn custom_emoji(
        custom_emoji_id: impl Into<String>,
        alternative_text: impl Into<String>,
    ) -> Self {
        Self::CustomEmoji {
            custom_emoji_id: custom_emoji_id.into(),
            alternative_text: alternative_text.into(),
        }
    }

    /// Returns the visible fallback text used for validation and degraded
    /// clients.
    #[must_use]
    pub fn alternative_text(&self) -> &str {
        match self {
            Self::Plain(text) => text,
            Self::CustomEmoji {
                alternative_text, ..
            } => alternative_text,
        }
    }
}

impl From<String> for ButtonLabel {
    fn from(value: String) -> Self {
        Self::Plain(value)
    }
}

impl From<&str> for ButtonLabel {
    fn from(value: &str) -> Self {
        Self::Plain(value.to_owned())
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
