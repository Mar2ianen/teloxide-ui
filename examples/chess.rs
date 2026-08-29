use std::{
    collections::HashMap,
    error::Error,
    sync::{Arc, Mutex},
    time::Duration,
};

use teloxide::{
    dispatching::UpdateFilterExt,
    outbound::{Outbound, OutboundQueue, OutboundSettings},
    prelude::Dispatcher,
    requests::Requester,
    types::{CallbackQuery, Message, Update, UserId},
    Bot,
};
use teloxide_ui::{
    ActionRegistry, ActorPolicy, ButtonLabel, ButtonStyle, InMemoryUiStore, RenderContext,
    Revision, RichRenderer, StalePolicy, Surface, SurfaceWorker, Ui, UiStore, ViewId, ViewRecord,
};

type HandlerError = Box<dyn Error + Send + Sync>;

const ACTION_TTL: Duration = Duration::from_secs(30 * 60);

#[derive(Clone, Default)]
struct ChessEmojiPalette {
    pieces: HashMap<Piece, ButtonLabel>,
    cells: [Option<ButtonLabel>; 2],
}

impl ChessEmojiPalette {
    fn cell_label(&self, ordinal: usize) -> Option<ButtonLabel> {
        self.cells[ordinal % self.cells.len()].clone()
    }
}

fn reference_emoji_palette() -> ChessEmojiPalette {
    // Captured once from the supplied empty Rich Message board. The two
    // labels alternate in the same order as the 8x8 button rows.
    ChessEmojiPalette {
        pieces: HashMap::new(),
        cells: [
            Some(ButtonLabel::custom_emoji("5877410604225924969", "🔄")),
            Some(ButtonLabel::custom_emoji("5843799474362652262", "🔄")),
        ],
    }
}

#[derive(Clone)]
struct ChessApp {
    outbound: Outbound<Bot>,
    registry: ActionRegistry<ChessAction>,
    store: InMemoryUiStore<ChessState>,
    worker: SurfaceWorker<Bot>,
    views: Arc<Mutex<HashMap<Surface, ViewId>>>,
    emoji_palette: Arc<Mutex<ChessEmojiPalette>>,
}

impl ChessApp {
    fn new(bot: Bot) -> Result<Self, HandlerError> {
        let queue = OutboundQueue::new_spawn(OutboundSettings::default())?;
        let outbound = bot.clone().outbound(queue.clone());
        Ok(Self {
            outbound,
            registry: ActionRegistry::with_default_ttl(Some(ACTION_TTL)),
            store: InMemoryUiStore::new(),
            worker: SurfaceWorker::new(bot, queue),
            views: Arc::new(Mutex::new(HashMap::new())),
            emoji_palette: Arc::new(Mutex::new(reference_emoji_palette())),
        })
    }

    fn emoji_palette(&self) -> ChessEmojiPalette {
        self.emoji_palette
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    async fn start_game(&self, message: &Message) -> Result<(), HandlerError> {
        let view_id = ViewId::fresh();
        let state = ChessState::new(message.from.as_ref().map(|user| user.id));
        let palette = self.emoji_palette();
        let rendered = render_game(&self.registry, view_id, Revision::INITIAL, &state, &palette)?;

        // The initial send must use the same shared outbound queue as later
        // edits. The view/store record is inserted only after Telegram gives
        // us the concrete message surface.
        let sent = self
            .outbound
            .send_rich_message(message.chat.id, rendered.rich_message)
            .await?;
        let surface = Surface::Message {
            chat_id: message.chat.id,
            message_id: sent.id,
        };
        self.store.insert(ViewRecord {
            id: view_id,
            revision: Revision::INITIAL,
            state,
            surface: surface.clone(),
        })?;
        self.views
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(surface, view_id);
        Ok(())
    }

    async fn handle_callback(&self, query: CallbackQuery) -> Result<(), HandlerError> {
        // ACK is deliberately first and independent of the render path.
        self.outbound
            .answer_callback_query(query.id.clone())
            .await?;

        let Some(data) = query.data.as_deref() else {
            return Ok(());
        };
        let Ok(token) = teloxide_ui::ActionToken::try_from(data) else {
            return Ok(());
        };
        let Some(message) = query.regular_message() else {
            return Ok(());
        };
        let surface = Surface::Message {
            chat_id: message.chat.id,
            message_id: message.id,
        };
        let Some(view_id) = self
            .views
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&surface)
            .copied()
        else {
            return Ok(());
        };
        let Some(record) = self.store.load(view_id)? else {
            return Ok(());
        };
        let action = match self
            .registry
            .resolve(&token, query.from.id, view_id, record.revision)
        {
            Ok(record) => record.action,
            Err(_) => return Ok(()),
        };
        let next_state = match transition(record.state, action) {
            Ok(state) => state,
            Err(_) => return Ok(()),
        };

        // CAS makes two concurrent clicks on one revision deterministic. The
        // loser observes a conflict and leaves the winner's projection alone.
        let updated = match self
            .store
            .compare_and_set(view_id, record.revision, next_state)
        {
            Ok(record) => record,
            Err(_) => return Ok(()),
        };
        let palette = self.emoji_palette();
        let rendered = render_game(
            &self.registry,
            updated.id,
            updated.revision,
            &updated.state,
            &palette,
        )?;
        self.worker
            .project(updated.surface, updated.revision, rendered.rich_message)
            .await?;
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), HandlerError> {
    let bot = Bot::from_env();
    let app = Arc::new(ChessApp::new(bot.clone())?);
    app.outbound.get_me().await?;
    println!("teloxide-ui chess demo is running; send /chess to the bot");
    let handler = teloxide::dptree::entry()
        .branch(Update::filter_message().endpoint(message_handler))
        .branch(Update::filter_callback_query().endpoint(callback_handler));

    Dispatcher::builder(bot, handler)
        .dependencies(teloxide::dptree::deps![app])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
    Ok(())
}

async fn message_handler(app: Arc<ChessApp>, message: Message) -> Result<(), HandlerError> {
    let Some(text) = message.text() else {
        return Ok(());
    };
    let command = text.split_whitespace().next().unwrap_or_default();
    if command == "/chess" || command.starts_with("/chess@") {
        app.start_game(&message).await?;
    }
    Ok(())
}

async fn callback_handler(app: Arc<ChessApp>, query: CallbackQuery) -> Result<(), HandlerError> {
    app.handle_callback(query).await
}

fn render_game(
    registry: &ActionRegistry<ChessAction>,
    view_id: ViewId,
    revision: Revision,
    state: &ChessState,
    palette: &ChessEmojiPalette,
) -> Result<teloxide_ui::RenderedRichMessage, teloxide_ui::RenderError> {
    let renderer = RichRenderer::new(registry.clone()).action_ttl(Some(ACTION_TTL));
    renderer.render(
        &view(state, palette),
        RenderContext::new(view_id, revision)
            .actor(actor_policy(state.owner))
            .stale_policy(StalePolicy::Reject),
    )
}

fn actor_policy(owner: Option<UserId>) -> ActorPolicy {
    owner.map_or(ActorPolicy::Any, ActorPolicy::User)
}

fn view(state: &ChessState, palette: &ChessEmojiPalette) -> Ui<ChessAction> {
    let turn = state.turn.label();
    let status = match state.selected {
        Some(square) => format!(
            "{turn} to move · choose a destination for {}",
            square.coordinate()
        ),
        None => format!("{turn} to move · select a piece"),
    };
    let mut ui = Ui::column()
        .push(Ui::heading("♟ Rich Message Chess"))
        .push(Ui::paragraph(status));

    let mut ordinal = 0;
    for rank in (0..8).rev() {
        let mut row = Ui::button_row();
        for file in 0..8 {
            let square = Square::new(file, rank).expect("board coordinate");
            let style = if state.selected == Some(square) {
                ButtonStyle::Primary
            } else if state
                .selected
                .is_some_and(|from| legal_move(&state.board, from, square))
            {
                ButtonStyle::Success
            } else {
                ButtonStyle::Default
            };
            let label = state.board[square.index()].map_or_else(
                || {
                    palette
                        .cell_label(ordinal)
                        .unwrap_or_else(|| ButtonLabel::from("·"))
                },
                |piece| {
                    palette
                        .pieces
                        .get(&piece)
                        .cloned()
                        .unwrap_or_else(|| ButtonLabel::from(piece.label()))
                },
            );
            row = row.push(Ui::button(label, ChessAction::Square(square.0)).style(style));
            ordinal += 1;
        }
        ui = ui.push(row);
    }

    ui.push(
        Ui::button_row()
            .push(Ui::button("New game", ChessAction::Reset).style(ButtonStyle::Danger)),
    )
}

fn transition(mut state: ChessState, action: ChessAction) -> Result<ChessState, &'static str> {
    match action {
        ChessAction::Reset => {
            let owner = state.owner;
            state = ChessState::new(owner);
        }
        ChessAction::Square(raw) => {
            let square = Square::from_raw(raw).ok_or("invalid square")?;
            match state.selected {
                None => {
                    if state.board[square.index()].is_some_and(|piece| piece.color == state.turn) {
                        state.selected = Some(square);
                    } else {
                        return Err("not your piece");
                    }
                }
                Some(from) if from == square => state.selected = None,
                Some(_from)
                    if state.board[square.index()]
                        .is_some_and(|piece| piece.color == state.turn) =>
                {
                    state.selected = Some(square);
                }
                Some(from) if legal_move(&state.board, from, square) => {
                    let mut piece = state.board[from.index()].take().ok_or("missing piece")?;
                    if piece.kind == Kind::Pawn && (square.rank() == 0 || square.rank() == 7) {
                        piece.kind = Kind::Queen;
                    }
                    state.board[square.index()] = Some(piece);
                    state.selected = None;
                    state.turn = state.turn.other();
                }
                Some(_) => return Err("illegal move"),
            }
        }
    }
    Ok(state)
}

fn legal_move(board: &[Option<Piece>; 64], from: Square, to: Square) -> bool {
    if from == to {
        return false;
    }
    let Some(piece) = board[from.index()] else {
        return false;
    };
    if board[to.index()]
        .is_some_and(|target| target.color == piece.color || target.kind == Kind::King)
    {
        return false;
    }
    let df = to.file() - from.file();
    let dr = to.rank() - from.rank();
    let abs_file = df.unsigned_abs();
    let abs_rank = dr.unsigned_abs();
    match piece.kind {
        Kind::Pawn => {
            let direction = if piece.color == Color::White { 1 } else { -1 };
            let start_rank = if piece.color == Color::White { 1 } else { 6 };
            if df == 0 && board[to.index()].is_none() && dr == direction {
                return true;
            }
            if df == 0
                && from.rank() == start_rank
                && dr == 2 * direction
                && board[to.index()].is_none()
                && Square::new(from.file(), from.rank() + direction)
                    .is_some_and(|middle| board[middle.index()].is_none())
            {
                return true;
            }
            abs_file == 1 && dr == direction && board[to.index()].is_some()
        }
        Kind::Knight => (abs_file == 1 && abs_rank == 2) || (abs_file == 2 && abs_rank == 1),
        Kind::Bishop => abs_file == abs_rank && path_is_clear(board, from, to),
        Kind::Rook => (df == 0 || dr == 0) && path_is_clear(board, from, to),
        Kind::Queen => {
            (df == 0 || dr == 0 || abs_file == abs_rank) && path_is_clear(board, from, to)
        }
        Kind::King => abs_file <= 1 && abs_rank <= 1,
    }
}

fn path_is_clear(board: &[Option<Piece>; 64], from: Square, to: Square) -> bool {
    let step_file = (to.file() - from.file()).signum();
    let step_rank = (to.rank() - from.rank()).signum();
    let mut file = from.file() + step_file;
    let mut rank = from.rank() + step_rank;
    while file != to.file() || rank != to.rank() {
        let Some(square) = Square::new(file, rank) else {
            return false;
        };
        if board[square.index()].is_some() {
            return false;
        }
        file += step_file;
        rank += step_rank;
    }
    true
}

#[derive(Clone, Debug, PartialEq)]
enum ChessAction {
    Square(u8),
    Reset,
}

#[derive(Clone, Debug, PartialEq)]
struct ChessState {
    board: [Option<Piece>; 64],
    turn: Color,
    selected: Option<Square>,
    owner: Option<UserId>,
}

impl ChessState {
    fn new(owner: Option<UserId>) -> Self {
        let mut board = [None; 64];
        let back_rank = [
            Kind::Rook,
            Kind::Knight,
            Kind::Bishop,
            Kind::Queen,
            Kind::King,
            Kind::Bishop,
            Kind::Knight,
            Kind::Rook,
        ];
        for (file, kind) in back_rank.into_iter().enumerate() {
            board[file] = Some(Piece {
                color: Color::White,
                kind,
            });
            board[8 + file] = Some(Piece {
                color: Color::White,
                kind: Kind::Pawn,
            });
            board[48 + file] = Some(Piece {
                color: Color::Black,
                kind: Kind::Pawn,
            });
            board[56 + file] = Some(Piece {
                color: Color::Black,
                kind,
            });
        }
        Self {
            board,
            turn: Color::White,
            selected: None,
            owner,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct Piece {
    color: Color,
    kind: Kind,
}

impl Piece {
    fn label(self) -> String {
        self.symbol().to_owned()
    }

    fn symbol(self) -> &'static str {
        match (self.color, self.kind) {
            (Color::White, Kind::Pawn) => "♙\u{fe0f}",
            (Color::White, Kind::Knight) => "♘\u{fe0f}",
            (Color::White, Kind::Bishop) => "♗\u{fe0f}",
            (Color::White, Kind::Rook) => "♖\u{fe0f}",
            (Color::White, Kind::Queen) => "♕\u{fe0f}",
            (Color::White, Kind::King) => "♔\u{fe0f}",
            (Color::Black, Kind::Pawn) => "♟\u{fe0f}",
            (Color::Black, Kind::Knight) => "♞\u{fe0f}",
            (Color::Black, Kind::Bishop) => "♝\u{fe0f}",
            (Color::Black, Kind::Rook) => "♜\u{fe0f}",
            (Color::Black, Kind::Queen) => "♛\u{fe0f}",
            (Color::Black, Kind::King) => "♚\u{fe0f}",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum Color {
    White,
    Black,
}

impl Color {
    fn label(self) -> &'static str {
        match self {
            Self::White => "White",
            Self::Black => "Black",
        }
    }

    fn other(self) -> Self {
        match self {
            Self::White => Self::Black,
            Self::Black => Self::White,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum Kind {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Square(u8);

impl Square {
    fn new(file: i8, rank: i8) -> Option<Self> {
        (0..8).contains(&file).then_some(()).and_then(|()| {
            (0..8)
                .contains(&rank)
                .then_some(Self((rank * 8 + file) as u8))
        })
    }

    fn from_raw(raw: u8) -> Option<Self> {
        (raw < 64).then_some(Self(raw))
    }

    fn index(self) -> usize {
        self.0 as usize
    }

    fn file(self) -> i8 {
        (self.0 % 8) as i8
    }

    fn rank(self) -> i8 {
        (self.0 / 8) as i8
    }

    fn coordinate(self) -> String {
        format!("{}{}", (b'a' + self.file() as u8) as char, self.rank() + 1)
    }
}

impl Default for ChessState {
    fn default() -> Self {
        Self::new(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_board_has_expected_turn_and_pieces() {
        let state = ChessState::new(None);
        assert_eq!(state.turn, Color::White);
        assert_eq!(state.board.iter().flatten().count(), 32);
        assert_eq!(
            state.board[Square::new(4, 0).unwrap().index()],
            Some(Piece {
                color: Color::White,
                kind: Kind::King,
            })
        );
    }

    #[test]
    fn pawn_move_commits_and_changes_turn() {
        let state = ChessState::new(None);
        let moved = transition(state, ChessAction::Square(Square::new(4, 1).unwrap().0))
            .and_then(|state| transition(state, ChessAction::Square(Square::new(4, 3).unwrap().0)))
            .unwrap();
        assert_eq!(moved.turn, Color::Black);
        assert!(moved.board[Square::new(4, 1).unwrap().index()].is_none());
        assert_eq!(
            moved.board[Square::new(4, 3).unwrap().index()],
            Some(Piece {
                color: Color::White,
                kind: Kind::Pawn,
            })
        );
    }

    #[test]
    fn black_can_select_and_move_after_white_turn() {
        let state = ChessState::new(None);
        let state = transition(state, ChessAction::Square(Square::new(4, 1).unwrap().0))
            .and_then(|state| transition(state, ChessAction::Square(Square::new(4, 3).unwrap().0)))
            .unwrap();
        let moved = transition(state, ChessAction::Square(Square::new(4, 6).unwrap().0))
            .and_then(|state| transition(state, ChessAction::Square(Square::new(4, 4).unwrap().0)))
            .unwrap();
        assert_eq!(moved.turn, Color::White);
        assert_eq!(
            moved.board[Square::new(4, 4).unwrap().index()],
            Some(Piece {
                color: Color::Black,
                kind: Kind::Pawn,
            })
        );
    }

    #[test]
    fn every_board_cell_has_a_server_action() {
        let registry = ActionRegistry::new();
        let rendered = render_game(
            &registry,
            ViewId::new(1),
            Revision::INITIAL,
            &ChessState::new(None),
            &reference_emoji_palette(),
        )
        .unwrap();
        assert_eq!(rendered.action_tokens.len(), 65);
    }

    #[test]
    fn blocked_piece_cannot_move_through_another_piece() {
        let state = ChessState::new(None);
        assert!(!legal_move(
            &state.board,
            Square::new(0, 0).unwrap(),
            Square::new(0, 3).unwrap()
        ));
    }
}
