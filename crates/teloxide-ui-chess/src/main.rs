use std::{
    collections::HashMap,
    error::Error,
    sync::{Arc, Mutex},
    time::Duration,
};

use cozy_chess::{
    util::{display_uci_move, parse_uci_move},
    Board as ChessBoard, Color as EngineColor, GameStatus, Piece as EnginePiece,
    Square as EngineSquare,
};
use teloxide::{
    dispatching::UpdateFilterExt,
    outbound::{Outbound, OutboundQueue, OutboundSettings},
    prelude::Dispatcher,
    requests::Requester,
    types::{CallbackQuery, ChatId, Message, Update, UserId},
    Bot,
};
use teloxide_ui::{
    ActionRegistry, ActorPolicy, ButtonLabel, InMemoryUiStore, RenderContext, Revision,
    RichRenderer, StalePolicy, Surface, SurfaceWorker, TableCell, Ui, UiStore, ViewId, ViewRecord,
};

type HandlerError = Box<dyn Error + Send + Sync>;

const ACTION_TTL: Duration = Duration::from_secs(30 * 60);
const VISIBLE_MOVE_COUNT: usize = 2;

#[derive(Clone)]
struct ChessEmojiPalette {
    ids: Vec<&'static str>,
}

impl ChessEmojiPalette {
    fn cell_label(&self, visual: CellVisual, piece: Option<Piece>) -> ButtonLabel {
        let index = visual.index() * 13 + piece_index(piece);
        let fallback = match (visual, piece) {
            (CellVisual::Legal, None) => "🟢",
            (CellVisual::Capture, None) => "🔴",
            (CellVisual::Selected, None) => "🟨",
            (_, None) => "▫️",
            (
                _,
                Some(Piece {
                    color: Color::White,
                    ..
                }),
            ) => "⚪",
            (
                _,
                Some(Piece {
                    color: Color::Black,
                    ..
                }),
            ) => "⚫",
        };
        ButtonLabel::custom_emoji(self.ids[index], fallback)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum CellVisual {
    Base,
    Selected,
    Legal,
    Capture,
}

impl CellVisual {
    const fn index(self) -> usize {
        match self {
            Self::Base => 0,
            Self::Selected => 1,
            Self::Legal => 2,
            Self::Capture => 3,
        }
    }
}

const fn piece_index(piece: Option<Piece>) -> usize {
    match piece {
        None => 0,
        Some(Piece {
            color: Color::Black,
            kind: Kind::Pawn,
        }) => 1,
        Some(Piece {
            color: Color::Black,
            kind: Kind::Knight,
        }) => 2,
        Some(Piece {
            color: Color::Black,
            kind: Kind::Bishop,
        }) => 3,
        Some(Piece {
            color: Color::Black,
            kind: Kind::Rook,
        }) => 4,
        Some(Piece {
            color: Color::Black,
            kind: Kind::Queen,
        }) => 5,
        Some(Piece {
            color: Color::Black,
            kind: Kind::King,
        }) => 6,
        Some(Piece {
            color: Color::White,
            kind: Kind::Pawn,
        }) => 7,
        Some(Piece {
            color: Color::White,
            kind: Kind::Knight,
        }) => 8,
        Some(Piece {
            color: Color::White,
            kind: Kind::Bishop,
        }) => 9,
        Some(Piece {
            color: Color::White,
            kind: Kind::Rook,
        }) => 10,
        Some(Piece {
            color: Color::White,
            kind: Kind::Queen,
        }) => 11,
        Some(Piece {
            color: Color::White,
            kind: Kind::King,
        }) => 12,
    }
}

fn reference_emoji_palette() -> ChessEmojiPalette {
    // Every board cell is a transparent, code-composited overlay. Telegram's
    // native table supplies the alternating square background; this keeps the
    // eight columns flush like the official Rich Text Chess reference.
    //
    // The overlays and markers are generated locally. ImageGen was used only
    // for the transparent 2D piece source sheet.
    let ids: Vec<_> = include_str!("../../../assets/chess-emoji-native/ids.txt")
        .lines()
        .filter(|line| !line.is_empty())
        .collect();
    assert_eq!(
        ids.len(),
        52,
        "generated chess emoji palette must be complete"
    );
    ChessEmojiPalette { ids }
}

#[derive(Clone)]
struct ChessApp {
    outbound: Outbound<Bot>,
    registry: ActionRegistry<ChessAction>,
    store: InMemoryUiStore<ChessState>,
    worker: SurfaceWorker<Bot>,
    views: Arc<Mutex<HashMap<Surface, ViewId>>>,
    emoji_palette: ChessEmojiPalette,
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
            emoji_palette: reference_emoji_palette(),
        })
    }

    fn emoji_palette(&self) -> ChessEmojiPalette {
        self.emoji_palette.clone()
    }

    async fn start_game(&self, message: &Message) -> Result<(), HandlerError> {
        self.start_game_in_chat(message.chat.id, message.from.as_ref().map(|user| user.id))
            .await
    }

    async fn start_game_in_chat(
        &self,
        chat_id: ChatId,
        owner: Option<UserId>,
    ) -> Result<(), HandlerError> {
        let view_id = ViewId::fresh();
        let state = ChessState::new(owner);
        let palette = self.emoji_palette();
        let rendered = render_game(&self.registry, view_id, Revision::INITIAL, &state, &palette)?;

        // The initial send must use the same shared outbound queue as later
        // edits. The view/store record is inserted only after Telegram gives
        // us the concrete message surface.
        let sent = self
            .outbound
            .send_rich_message(chat_id, rendered.rich_message)
            .await?;
        let surface = Surface::Message {
            chat_id,
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
    if let Some(raw_chat_id) = std::env::var_os("TELOXIDE_CHESS_AUTOSTART_CHAT_ID") {
        let chat_id = ChatId(raw_chat_id.to_string_lossy().parse()?);
        app.start_game_in_chat(chat_id, None).await?;
        println!("sent chess demo to chat {chat_id:?}");
    }
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
        if let Err(error) = app.start_game(&message).await {
            eprintln!("chess start failed: {error}");
        }
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
    let turn = ui_color(state.board.side_to_move());
    let status = if state.finished {
        "Game finished".to_owned()
    } else {
        match state.board.status() {
            GameStatus::Won => format!("Checkmate · {} wins", turn.other().label()),
            GameStatus::Drawn => "Game drawn".to_owned(),
            GameStatus::Ongoing => {
                // Keep this block to one stable line. The selected square is already
                // visible through the board marker, so putting its coordinate here
                // only makes Telegram reflow the whole message on every click.
                let check = if !state.board.checkers().is_empty() {
                    " · check"
                } else {
                    ""
                };
                format!("Your move as {}{check}", turn.label())
            }
        }
    };
    let files: Vec<i8> = if state.flipped {
        (0..8).rev().collect()
    } else {
        (0..8).collect()
    };
    let ranks: Vec<i8> = if state.flipped {
        (0..8).collect()
    } else {
        (0..8).rev().collect()
    };
    // Each board cell is always a button carrying a transparent overlay. The
    // native table cell supplies the checkerboard background; the header flag
    // is deliberately applied to every light square, including empty cells.
    let mut board = Ui::table().bordered(false).striped(false).compact(true);
    let mut coordinate_row = vec![TableCell::empty()];
    coordinate_row.extend(
        files
            .iter()
            .copied()
            .map(|file| TableCell::text(file_label(file))),
    );
    board = board.row(coordinate_row);

    for rank in ranks {
        let mut row = vec![TableCell::text((rank + 1).to_string())];
        for file in files.iter().copied() {
            let square = Square::new(file, rank).expect("board coordinate");
            let legal_destination = state
                .selected
                .is_some_and(|from| legal_move(&state.board, from, square));
            let capture_target = legal_destination && board_piece(&state.board, square).is_some();
            let cell_visual = if state.selected == Some(square) {
                CellVisual::Selected
            } else if capture_target {
                CellVisual::Capture
            } else if legal_destination {
                CellVisual::Legal
            } else {
                CellVisual::Base
            };
            let light = (file + rank) % 2 != 0;
            let label = palette.cell_label(cell_visual, board_piece(&state.board, square));
            row.push(
                TableCell::button(label, ChessAction::Square(square.0))
                    // Link style removes native rounded button chrome; the
                    // table cell supplies the square background.
                    .style(teloxide_ui::ButtonStyle::Link)
                    .header(light)
                    .disabled(state.finished),
            );
        }
        board = board.row(row);
    }
    let mut coordinate_row = vec![TableCell::empty()];
    coordinate_row.extend(
        files
            .iter()
            .copied()
            .map(|file| TableCell::text(file_label(file))),
    );
    board = board.row(coordinate_row);

    // Match the reference projection order: the board is the primary visual,
    // followed by turn status, move history, and controls.
    let mut ui = Ui::column().push(board).push(Ui::blockquote(status));
    // Keep the details block present from the first render. Its summary and
    // body show only the last two plies, so a new move does not add a block or
    // another wrapped line to the message. The full move list remains in
    // server state for undo and future richer history projections.
    let moves = visible_moves(&state.moves);
    ui = ui.push(Ui::details(
        format!("Moves · {moves}"),
        [Ui::paragraph(moves)],
    ));
    ui = ui.push(
        Ui::button_row()
            .push(Ui::button("⟳ Flip board", ChessAction::FlipBoard))
            .push(Ui::button("↶ Undo", ChessAction::Undo).disabled(state.history.is_empty())),
    );
    if state.finished {
        ui.push(Ui::button_row().push(Ui::button("New Game", ChessAction::Reset)))
    } else {
        ui.push(Ui::button_row().push(Ui::button("⊗ Finish Game", ChessAction::Finish)))
    }
}

fn visible_moves(moves: &[String]) -> String {
    if moves.is_empty() {
        return "—".to_owned();
    }
    let first_visible = moves.len().saturating_sub(VISIBLE_MOVE_COUNT);
    let recent = moves[first_visible..].join("  ");
    if first_visible == 0 {
        recent
    } else {
        format!("…  {recent}")
    }
}

fn transition(mut state: ChessState, action: ChessAction) -> Result<ChessState, &'static str> {
    match action {
        ChessAction::Reset => {
            let owner = state.owner;
            state = ChessState::new(owner);
        }
        ChessAction::FlipBoard => state.flipped = !state.flipped,
        ChessAction::Undo => {
            let previous = state.history.pop().ok_or("nothing to undo")?;
            state.board = previous.board;
            state.selected = None;
            state.finished = false;
            state.moves.pop();
        }
        ChessAction::Finish => {
            state.finished = true;
            state.selected = None;
        }
        ChessAction::Square(raw) => {
            if state.finished {
                return Err("game is finished");
            }
            if state.board.status() != GameStatus::Ongoing {
                return Err("game is over");
            }
            let square = Square::from_raw(raw).ok_or("invalid square")?;
            let engine_square = engine_square(square);
            let turn = state.board.side_to_move();
            match state.selected {
                None => {
                    if state.board.color_on(engine_square) == Some(turn) {
                        state.selected = Some(square);
                    } else {
                        return Err("not your piece");
                    }
                }
                Some(from) if from == square => state.selected = None,
                Some(_from) if state.board.color_on(engine_square) == Some(turn) => {
                    state.selected = Some(square);
                }
                Some(from) if legal_move(&state.board, from, square) => {
                    let from_coordinate = from.coordinate();
                    let to_coordinate = square.coordinate();
                    let mv = legal_move_for(&state.board, from, square).ok_or("illegal move")?;
                    let notation = display_uci_move(&state.board, mv).to_string();
                    state.history.push(ChessPosition {
                        board: state.board.clone(),
                    });
                    state.board.try_play(mv).map_err(|_| "illegal move")?;
                    state.selected = None;
                    state
                        .moves
                        .push(if notation == format!("{from_coordinate}{to_coordinate}") {
                            format!("{from_coordinate}-{to_coordinate}")
                        } else {
                            notation
                        });
                }
                Some(_) => return Err("illegal move"),
            }
        }
    }
    Ok(state)
}

fn legal_move(board: &ChessBoard, from: Square, to: Square) -> bool {
    legal_move_for(board, from, to).is_some()
}

fn legal_move_for(board: &ChessBoard, from: Square, to: Square) -> Option<cozy_chess::Move> {
    if from == to {
        return None;
    }
    let promotion = if board.piece_on(engine_square(from)) == Some(EnginePiece::Pawn)
        && (to.rank() == 0 || to.rank() == 7)
    {
        "q"
    } else {
        ""
    };
    let uci = format!("{}{}{promotion}", from.coordinate(), to.coordinate());
    let mv = parse_uci_move(board, &uci).ok()?;
    board.is_legal(mv).then_some(mv)
}

fn board_piece(board: &ChessBoard, square: Square) -> Option<Piece> {
    let engine_square = engine_square(square);
    Some(Piece {
        color: ui_color(board.color_on(engine_square)?),
        kind: ui_kind(board.piece_on(engine_square)?),
    })
}

fn engine_square(square: Square) -> EngineSquare {
    EngineSquare::index(square.index())
}

fn ui_color(color: EngineColor) -> Color {
    match color {
        EngineColor::White => Color::White,
        EngineColor::Black => Color::Black,
    }
}

fn ui_kind(piece: EnginePiece) -> Kind {
    match piece {
        EnginePiece::Pawn => Kind::Pawn,
        EnginePiece::Knight => Kind::Knight,
        EnginePiece::Bishop => Kind::Bishop,
        EnginePiece::Rook => Kind::Rook,
        EnginePiece::Queen => Kind::Queen,
        EnginePiece::King => Kind::King,
    }
}

#[derive(Clone, Debug, PartialEq)]
enum ChessAction {
    Square(u8),
    Reset,
    FlipBoard,
    Undo,
    Finish,
}

#[derive(Clone, Debug, PartialEq)]
struct ChessState {
    board: ChessBoard,
    selected: Option<Square>,
    owner: Option<UserId>,
    history: Vec<ChessPosition>,
    moves: Vec<String>,
    flipped: bool,
    finished: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct ChessPosition {
    board: ChessBoard,
}

impl ChessState {
    fn new(owner: Option<UserId>) -> Self {
        Self {
            board: ChessBoard::default(),
            selected: None,
            owner,
            history: Vec::new(),
            moves: Vec::new(),
            flipped: false,
            finished: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct Piece {
    color: Color,
    kind: Kind,
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

fn file_label(file: i8) -> String {
    ((b'a' + file as u8) as char).to_string()
}

impl Default for ChessState {
    fn default() -> Self {
        Self::new(None)
    }
}

#[cfg(test)]
mod tests {
    use teloxide_ui::UiNode;

    use super::*;

    fn square(coordinate: &str) -> Square {
        let bytes = coordinate.as_bytes();
        Square::new((bytes[0] - b'a') as i8, (bytes[1] - b'1') as i8).expect("valid square")
    }

    fn play(state: ChessState, from: &str, to: &str) -> ChessState {
        transition(state, ChessAction::Square(square(from).0))
            .and_then(|state| transition(state, ChessAction::Square(square(to).0)))
            .expect("legal move")
    }

    #[test]
    fn initial_board_has_expected_turn_and_pieces() {
        let state = ChessState::new(None);
        assert_eq!(state.board.side_to_move(), EngineColor::White);
        assert_eq!(state.board.occupied().len(), 32);
        assert_eq!(
            state.board.piece_on(EngineSquare::E1),
            Some(EnginePiece::King)
        );
    }

    #[test]
    fn pawn_move_commits_and_changes_turn() {
        let moved = play(ChessState::new(None), "e2", "e4");
        assert_eq!(moved.board.side_to_move(), EngineColor::Black);
        assert_eq!(moved.board.piece_on(EngineSquare::E2), None);
        assert_eq!(
            moved.board.piece_on(EngineSquare::E4),
            Some(EnginePiece::Pawn)
        );
    }

    #[test]
    fn black_can_select_and_move_after_white_turn() {
        let moved = play(play(ChessState::new(None), "e2", "e4"), "e7", "e5");
        assert_eq!(moved.board.side_to_move(), EngineColor::White);
        assert_eq!(
            moved.board.piece_on(EngineSquare::E5),
            Some(EnginePiece::Pawn)
        );
    }

    #[test]
    fn interactive_board_cells_have_server_actions() {
        let registry = ActionRegistry::new();
        let rendered = render_game(
            &registry,
            ViewId::new(1),
            Revision::INITIAL,
            &ChessState::new(None),
            &reference_emoji_palette(),
        )
        .unwrap();
        // Every cell carries a complete fixed-size sprite and therefore keeps
        // a server-side action token even when it is empty. The enabled Flip
        // and Finish controls add the remaining tokens; Undo is disabled at
        // the initial position.
        assert_eq!(rendered.action_tokens.len(), 66);
    }

    #[test]
    fn selecting_a_piece_keeps_the_same_action_grid() {
        let selected =
            transition(ChessState::new(None), ChessAction::Square(square("d1").0)).unwrap();
        let registry = ActionRegistry::new();
        let rendered = render_game(
            &registry,
            ViewId::new(1),
            Revision::INITIAL,
            &selected,
            &reference_emoji_palette(),
        )
        .unwrap();
        assert_eq!(rendered.action_tokens.len(), 66);
    }

    #[test]
    fn board_uses_native_headers_for_light_squares() {
        let ui = view(&ChessState::new(None), &reference_emoji_palette());
        let UiNode::Table(table) = &ui.nodes[0] else {
            panic!("expected the board to be the first node");
        };

        for (rank_index, row) in table.rows[1..9].iter().enumerate() {
            for (file_index, cell) in row[1..9].iter().enumerate() {
                let light = (file_index + (7 - rank_index)) % 2 != 0;
                if light {
                    assert!(matches!(cell, TableCell::Header(_)));
                } else {
                    assert!(matches!(cell, TableCell::Button(_)));
                }
            }
        }
    }

    #[test]
    fn move_history_projection_is_always_present_and_compact() {
        assert_eq!(visible_moves(&[]), "—");
        assert_eq!(
            visible_moves(&["e2-e4".to_owned(), "e7-e5".to_owned()]),
            "e2-e4  e7-e5"
        );
        assert_eq!(
            visible_moves(&["e2-e4".to_owned(), "e7-e5".to_owned(), "g1-f3".to_owned(),]),
            "…  e7-e5  g1-f3"
        );
    }

    #[test]
    fn flip_and_undo_are_stateful_controls() {
        let state = ChessState::new(None);
        let state = transition(state, ChessAction::FlipBoard).unwrap();
        assert!(state.flipped);
        let state = play(state, "e2", "e4");
        assert_eq!(state.moves, ["e2-e4"]);
        let state = transition(state, ChessAction::Undo).unwrap();
        assert_eq!(state.board.side_to_move(), EngineColor::White);
        assert_eq!(
            state.board.piece_on(EngineSquare::E2),
            Some(EnginePiece::Pawn)
        );
        assert!(state.moves.is_empty());
    }

    #[test]
    fn blocked_piece_cannot_move_through_another_piece() {
        let state = ChessState::new(None);
        assert!(!legal_move(&state.board, square("a1"), square("a4")));
    }

    #[test]
    fn a_move_that_leaves_the_king_in_check_is_rejected() {
        let mut state = ChessState::new(None);
        state.board = "4r1k1/8/8/8/4R3/8/8/4K3 w - - 0 1".parse().unwrap();
        let selected = transition(state, ChessAction::Square(square("e4").0)).unwrap();
        assert_eq!(
            transition(selected, ChessAction::Square(square("d4").0)),
            Err("illegal move")
        );
    }

    #[test]
    fn a_king_cannot_move_into_check() {
        let mut state = ChessState::new(None);
        state.board = "3r2k1/8/8/8/8/8/8/4K3 w - - 0 1".parse().unwrap();
        let selected = transition(state, ChessAction::Square(square("e1").0)).unwrap();
        assert_eq!(
            transition(selected, ChessAction::Square(square("d2").0)),
            Err("illegal move")
        );
    }

    #[test]
    fn castling_and_en_passant_are_delegated_to_the_rules_engine() {
        let mut state = ChessState::new(None);
        state.board = "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1".parse().unwrap();
        let state = play(state, "e1", "g1");
        assert_eq!(
            state.board.piece_on(EngineSquare::G1),
            Some(EnginePiece::King)
        );
        assert_eq!(
            state.board.piece_on(EngineSquare::F1),
            Some(EnginePiece::Rook)
        );

        let state = play(ChessState::new(None), "e2", "e4");
        let state = play(state, "a7", "a6");
        let state = play(state, "e4", "e5");
        let state = play(state, "d7", "d5");
        let state = play(state, "e5", "d6");
        assert_eq!(
            state.board.piece_on(EngineSquare::D6),
            Some(EnginePiece::Pawn)
        );
        assert_eq!(state.board.piece_on(EngineSquare::D5), None);
    }
}
