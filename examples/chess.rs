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
    types::{CallbackQuery, ChatId, Message, Update, UserId},
    Bot,
};
use teloxide_ui::{
    ActionRegistry, ActorPolicy, ButtonLabel, InMemoryUiStore, RenderContext, Revision,
    RichRenderer, StalePolicy, Surface, SurfaceWorker, TableCell, Ui, UiStore, ViewId, ViewRecord,
};

type HandlerError = Box<dyn Error + Send + Sync>;

const ACTION_TTL: Duration = Duration::from_secs(30 * 60);

#[derive(Clone)]
struct ChessEmojiPalette {
    ids: [&'static str; 104],
    pieces: HashMap<Piece, ButtonLabel>,
    cells: [[ButtonLabel; 2]; 4],
}

impl ChessEmojiPalette {
    fn cell_label(
        &self,
        visual: CellVisual,
        light: bool,
        piece: Option<Piece>,
    ) -> Option<ButtonLabel> {
        if let Some(piece) = piece {
            if matches!(visual, CellVisual::Base | CellVisual::Legal) {
                return Some(self.pieces.get(&piece)?.clone());
            }

            let index = visual.index() * 26 + usize::from(light) * 13 + piece_index(Some(piece));
            let fallback = if piece.color == Color::White {
                "⚪"
            } else {
                "⚫"
            };
            return Some(ButtonLabel::custom_emoji(self.ids[index], fallback));
        }

        if visual == CellVisual::Base {
            return None;
        }
        Some(self.cells[visual.index()][usize::from(light)].clone())
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
    // The v2 entries below are the alternate complete-cell sprite palette.
    // The active board uses the transparent piece/marker entries plus native
    // table-cell backgrounds, because inline full-cell sprites leave gaps.
    // Generated and uploaded as teloxide_ui_chess_v2_by_testteloxideui_bot.
    ChessEmojiPalette {
        ids: [
            "5229161470729693238",
            "5231311277954999194",
            "5228800109361276788",
            "5231006068989007362",
            "5229019968737158946",
            "5228781984599288321",
            "5228969623130511052",
            "5230945458410526285",
            "5231362194792292292",
            "5229189899118224586",
            "5231421688679275409",
            "5228702149747191179",
            "5231233672190924875",
            "5229129546237785357",
            "5231047300675050789",
            "5230982064416791232",
            "5230957922405621415",
            "5231139212975185691",
            "5229106443608694315",
            "5228852366228371642",
            "5231347914026032278",
            "5228880133191932918",
            "5229228759982320480",
            "5228701535566867140",
            "5231367662285656844",
            "5228938622056575565",
            "5231051213390256484",
            "5229225117850049453",
            "5229040533040568540",
            "5231490936436989652",
            "5228765814047416100",
            "5229158812144936775",
            "5231221474483804648",
            "5230945174942683968",
            "5228782998211569059",
            "5231451461392571239",
            "5228894044591008638",
            "5231042413002266815",
            "5228909884430398034",
            "5229122562620958283",
            "5231362461080262304",
            "5231032036361281257",
            "5228949385244615071",
            "5231256658855895225",
            "5231063548536330452",
            "5229217159275649996",
            "5231146200886975272",
            "5228922515929210250",
            "5229014436819280801",
            "5229074622196001036",
            "5231373859923468346",
            "5228684952698135776",
            "5228819818966198063",
            "5228802596147342783",
            "5229217743391205832",
            "5231220971972631305",
            "5231233955658767870",
            "5228960320231350704",
            "5231291740148766232",
            "5229129168280659855",
            "5230994150454763468",
            "5231486727369039517",
            "5229167200216067287",
            "5231436338812725055",
            "5229212417631757621",
            "5231143129985359909",
            "5229040520155673146",
            "5231035854587208641",
            "5231289334967079902",
            "5228980811520320114",
            "5231463135113683876",
            "5229080991632497477",
            "5229046679138771688",
            "5231387097012689582",
            "5231441226485504882",
            "5229135937149114764",
            "5229101229518398261",
            "5228812504636891094",
            "5231032294059318069",
            "5231419528310727491",
            "5228991231110982190",
            "5231430454707525269",
            "5231105115229823089",
            "5229198308664192757",
            "5231175857636154955",
            "5229083109051381946",
            "5229026565806928281",
            "5228870452335653250",
            "5231243898508058182",
            "5228945975040589382",
            "5231427701633487022",
            "5231299857636956631",
            "5228997669266954309",
            "5230934922855750485",
            "5231262637450369294",
            "5229077375270039133",
            "5231029807273251847",
            "5231280989845629711",
            "5231324850051655564",
            "5231014714758174349",
            "5228946688005152975",
            "5229053898978798305",
            "5228939644258789499",
            "5231159188868077127",
        ],
        pieces: HashMap::from([
            (
                Piece {
                    color: Color::White,
                    kind: Kind::Pawn,
                },
                ButtonLabel::custom_emoji("5228706161246642160", "⚪"),
            ),
            (
                Piece {
                    color: Color::White,
                    kind: Kind::Knight,
                },
                ButtonLabel::custom_emoji("5228737651946858079", "⚪"),
            ),
            (
                Piece {
                    color: Color::White,
                    kind: Kind::Bishop,
                },
                ButtonLabel::custom_emoji("5228900139149603713", "⚪"),
            ),
            (
                Piece {
                    color: Color::White,
                    kind: Kind::Rook,
                },
                ButtonLabel::custom_emoji("5228764658701214476", "⚪"),
            ),
            (
                Piece {
                    color: Color::White,
                    kind: Kind::Queen,
                },
                ButtonLabel::custom_emoji("5229122957757949016", "⚪"),
            ),
            (
                Piece {
                    color: Color::White,
                    kind: Kind::King,
                },
                ButtonLabel::custom_emoji("5228868553960106840", "⚪"),
            ),
            (
                Piece {
                    color: Color::Black,
                    kind: Kind::Pawn,
                },
                ButtonLabel::custom_emoji("5231291748738701863", "⚫"),
            ),
            (
                Piece {
                    color: Color::Black,
                    kind: Kind::Knight,
                },
                ButtonLabel::custom_emoji("5228976370524139940", "⚫"),
            ),
            (
                Piece {
                    color: Color::Black,
                    kind: Kind::Bishop,
                },
                ButtonLabel::custom_emoji("5229157983216248474", "⚫"),
            ),
            (
                Piece {
                    color: Color::Black,
                    kind: Kind::Rook,
                },
                ButtonLabel::custom_emoji("5229076176974163218", "⚫"),
            ),
            (
                Piece {
                    color: Color::Black,
                    kind: Kind::Queen,
                },
                ButtonLabel::custom_emoji("5228772548556139102", "⚫"),
            ),
            (
                Piece {
                    color: Color::Black,
                    kind: Kind::King,
                },
                ButtonLabel::custom_emoji("5231337923932101541", "⚫"),
            ),
        ]),
        cells: [
            [
                ButtonLabel::custom_emoji("5228872857517338448", "⬜"),
                ButtonLabel::custom_emoji("5231047184710931936", "⬛"),
            ],
            [
                ButtonLabel::custom_emoji("5228943294980993429", "🔶"),
                ButtonLabel::custom_emoji("5229164915293464312", "🔷"),
            ],
            [
                ButtonLabel::custom_emoji("5231332615352522782", "🟢"),
                ButtonLabel::custom_emoji("5229180544679459827", "🟢"),
            ],
            [
                ButtonLabel::custom_emoji("5229049359198361382", "🔴"),
                ButtonLabel::custom_emoji("5229058687867328917", "🔴"),
            ],
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
    let status = if state.finished {
        "Game finished".to_owned()
    } else if !has_legal_move(&state.board, state.turn) {
        if is_in_check(&state.board, state.turn) {
            format!("Checkmate · {} wins", state.turn.other().label())
        } else {
            "Stalemate · draw".to_owned()
        }
    } else {
        // Keep this block to one stable line. The selected square is already
        // visible through the board marker, so putting its coordinate here
        // only makes Telegram reflow the whole message on every click.
        let check = if is_in_check(&state.board, state.turn) {
            " · check"
        } else {
            ""
        };
        format!("Your move as {}{check}", state.turn.label())
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
    // The reference is a compact table whose native header cells provide the
    // light checkerboard squares. Buttons are layered only where an action
    // exists; this keeps adjacent cells touching instead of showing gaps
    // around an inline full-cell emoji.
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
            let capture_target = legal_destination && state.board[square.index()].is_some();
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
            let label = palette
                .cell_label(cell_visual, light, state.board[square.index()])
                .map(|label| {
                    TableCell::button(label, ChessAction::Square(square.0))
                        // Link style removes the native rounded button chrome;
                        // the table cell itself supplies the checkerboard.
                        .style(teloxide_ui::ButtonStyle::Link)
                        .disabled(state.finished)
                });
            row.push(label.map_or_else(
                || TableCell::empty().header(light),
                |cell| cell.header(light),
            ));
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
    if !state.moves.is_empty() {
        let moves = state.moves.join("  ");
        ui = ui.push(Ui::details(
            format!("Moves · {moves}"),
            [Ui::paragraph(moves)],
        ));
    }
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
            state.turn = previous.turn;
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
            if !has_legal_move(&state.board, state.turn) {
                return Err("game is over");
            }
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
                    let from_coordinate = from.coordinate();
                    let to_coordinate = square.coordinate();
                    state.history.push(ChessPosition {
                        board: state.board,
                        turn: state.turn,
                    });
                    let mut piece = state.board[from.index()].take().ok_or("missing piece")?;
                    if piece.kind == Kind::Pawn && (square.rank() == 0 || square.rank() == 7) {
                        piece.kind = Kind::Queen;
                    }
                    state.board[square.index()] = Some(piece);
                    state.selected = None;
                    state.turn = state.turn.other();
                    state
                        .moves
                        .push(format!("{from_coordinate}-{to_coordinate}"));
                }
                Some(_) => return Err("illegal move"),
            }
        }
    }
    Ok(state)
}

fn legal_move(board: &[Option<Piece>; 64], from: Square, to: Square) -> bool {
    let Some(piece) = board[from.index()] else {
        return false;
    };
    if !pseudo_legal_move(board, from, to) {
        return false;
    }

    let mut next = *board;
    next[from.index()] = None;
    next[to.index()] = Some(piece);
    !is_in_check(&next, piece.color)
}

fn pseudo_legal_move(board: &[Option<Piece>; 64], from: Square, to: Square) -> bool {
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

fn is_in_check(board: &[Option<Piece>; 64], color: Color) -> bool {
    let Some(king) = board.iter().enumerate().find_map(|(index, piece)| {
        (*piece
            == Some(Piece {
                color,
                kind: Kind::King,
            }))
        .then(|| Square::from_raw(index as u8))
        .flatten()
    }) else {
        return true;
    };
    square_attacked(board, king, color.other())
}

fn square_attacked(board: &[Option<Piece>; 64], target: Square, by: Color) -> bool {
    board.iter().enumerate().any(|(index, piece)| {
        piece.is_some_and(|piece| {
            piece.color == by
                && Square::from_raw(index as u8)
                    .is_some_and(|from| piece_attacks_square(board, piece, from, target))
        })
    })
}

fn piece_attacks_square(
    board: &[Option<Piece>; 64],
    piece: Piece,
    from: Square,
    to: Square,
) -> bool {
    if from == to {
        return false;
    }
    let df = to.file() - from.file();
    let dr = to.rank() - from.rank();
    let abs_file = df.unsigned_abs();
    let abs_rank = dr.unsigned_abs();
    match piece.kind {
        Kind::Pawn => {
            let direction = if piece.color == Color::White { 1 } else { -1 };
            abs_file == 1 && dr == direction
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

fn has_legal_move(board: &[Option<Piece>; 64], color: Color) -> bool {
    board.iter().enumerate().any(|(from, piece)| {
        piece.is_some_and(|piece| {
            piece.color == color
                && (0..64).any(|to| {
                    legal_move(
                        board,
                        Square::from_raw(from as u8).expect("board index"),
                        Square::from_raw(to).expect("board index"),
                    )
                })
        })
    })
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
    FlipBoard,
    Undo,
    Finish,
}

#[derive(Clone, Debug, PartialEq)]
struct ChessState {
    board: [Option<Piece>; 64],
    turn: Color,
    selected: Option<Square>,
    owner: Option<UserId>,
    history: Vec<ChessPosition>,
    moves: Vec<String>,
    flipped: bool,
    finished: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ChessPosition {
    board: [Option<Piece>; 64],
    turn: Color,
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
        // Only occupied cells are interactive before a piece is selected; the
        // table's native header cells provide the checkerboard underneath.
        assert_eq!(rendered.action_tokens.len(), 34);
    }

    #[test]
    fn flip_and_undo_are_stateful_controls() {
        let state = ChessState::new(None);
        let state = transition(state, ChessAction::FlipBoard).unwrap();
        assert!(state.flipped);
        let state = transition(state, ChessAction::Square(Square::new(4, 1).unwrap().0))
            .and_then(|state| transition(state, ChessAction::Square(Square::new(4, 3).unwrap().0)))
            .unwrap();
        assert_eq!(state.moves, ["e2-e4"]);
        let state = transition(state, ChessAction::Undo).unwrap();
        assert_eq!(state.turn, Color::White);
        assert!(state.board[Square::new(4, 1).unwrap().index()].is_some());
        assert!(state.moves.is_empty());
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

    #[test]
    fn a_move_that_leaves_the_king_in_check_is_rejected() {
        let mut state = ChessState::new(None);
        state.board = [None; 64];
        state.board[Square::new(4, 0).unwrap().index()] = Some(Piece {
            color: Color::White,
            kind: Kind::King,
        });
        state.board[Square::new(0, 0).unwrap().index()] = Some(Piece {
            color: Color::White,
            kind: Kind::Rook,
        });
        state.board[Square::new(4, 7).unwrap().index()] = Some(Piece {
            color: Color::Black,
            kind: Kind::Rook,
        });
        state.board[Square::new(7, 7).unwrap().index()] = Some(Piece {
            color: Color::Black,
            kind: Kind::King,
        });

        let selected =
            transition(state, ChessAction::Square(Square::new(0, 0).unwrap().0)).unwrap();
        assert!(is_in_check(&selected.board, Color::White));
        assert_eq!(
            transition(selected, ChessAction::Square(Square::new(0, 1).unwrap().0)),
            Err("illegal move")
        );
    }

    #[test]
    fn a_king_cannot_move_into_check() {
        let mut state = ChessState::new(None);
        state.board = [None; 64];
        state.board[Square::new(4, 0).unwrap().index()] = Some(Piece {
            color: Color::White,
            kind: Kind::King,
        });
        state.board[Square::new(4, 7).unwrap().index()] = Some(Piece {
            color: Color::Black,
            kind: Kind::Rook,
        });
        state.board[Square::new(7, 7).unwrap().index()] = Some(Piece {
            color: Color::Black,
            kind: Kind::King,
        });

        let selected =
            transition(state, ChessAction::Square(Square::new(4, 0).unwrap().0)).unwrap();
        assert_eq!(
            transition(selected, ChessAction::Square(Square::new(4, 1).unwrap().0)),
            Err("illegal move")
        );
    }
}
