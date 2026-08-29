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
use lazychess::uci::{AnalysisInfo, Score, SearchConfig, UciEngine, UciError};
use teloxide::{
    dispatching::UpdateFilterExt,
    outbound::{Outbound, OutboundQueue, OutboundSettings},
    payloads::{AnswerCallbackQuerySetters, SendRichMessageSetters},
    prelude::Dispatcher,
    requests::Requester,
    types::{
        CallbackQuery, CallbackQueryId, ChatId, EphemeralMessageParameters, Message, Update, User,
        UserId,
    },
    Bot,
};
use teloxide_ui::{
    ActionRegistry, ActorPolicy, ButtonLabel, InMemoryUiStore, RenderContext, Revision,
    RichRenderer, StalePolicy, Surface, SurfaceWorker, TableCell, Ui, UiStore, ViewId, ViewRecord,
};

type HandlerError = Box<dyn Error + Send + Sync>;

const ACTION_TTL: Duration = Duration::from_secs(30 * 60);
const VISIBLE_MOVE_COUNT: usize = 2;
const DEFAULT_ENGINE_MOVETIME_MS: u64 = 350;
const MIN_ENGINE_MOVETIME_MS: u64 = 100;
const MAX_ENGINE_MOVETIME_MS: u64 = 5_000;
const INVISIBLE_EMPTY_CELL_LABEL: &str = "\u{2063}";

#[derive(Debug)]
struct EngineError(String);

impl std::fmt::Display for EngineError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for EngineError {}

impl From<UciError> for EngineError {
    fn from(error: UciError) -> Self {
        Self(error.to_string())
    }
}

#[derive(Debug)]
struct EngineReply {
    best_move: String,
    score: Option<Score>,
}

#[derive(Clone)]
struct StockfishEngine {
    process: Arc<Mutex<UciEngine>>,
    search: SearchConfig,
}

impl StockfishEngine {
    fn from_env() -> Result<Self, EngineError> {
        let path = std::env::var("STOCKFISH_PATH").unwrap_or_else(|_| "stockfish".to_owned());
        let movetime = std::env::var("STOCKFISH_MOVETIME_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(DEFAULT_ENGINE_MOVETIME_MS)
            .clamp(MIN_ENGINE_MOVETIME_MS, MAX_ENGINE_MOVETIME_MS);
        let search = std::env::var("STOCKFISH_DEPTH")
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
            .map(|depth| SearchConfig::depth(depth.clamp(1, 32)))
            .unwrap_or_else(|| SearchConfig::movetime(movetime));
        let mut process = UciEngine::new(&path)
            .map_err(|error| EngineError(format!("cannot start Stockfish at `{path}`: {error}")))?;
        process.new_game()?;
        Ok(Self {
            process: Arc::new(Mutex::new(process)),
            search,
        })
    }

    async fn best_move(&self, board: &ChessBoard) -> Result<EngineReply, EngineError> {
        let fen = board.to_string();
        let process = Arc::clone(&self.process);
        let search = self.search.clone();
        tokio::task::spawn_blocking(move || {
            let mut process = process
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            process.set_position_fen(&fen)?;
            let (best_move, analysis) = process.best_move_with_analysis(&search)?;
            Ok(EngineReply {
                best_move,
                score: deepest_score(&analysis),
            })
        })
        .await
        .map_err(|error| EngineError(format!("Stockfish worker stopped: {error}")))?
    }
}

fn deepest_score(analysis: &[AnalysisInfo]) -> Option<Score> {
    analysis
        .iter()
        .filter(|info| info.multipv.unwrap_or(1) == 1)
        .max_by_key(|info| info.depth.unwrap_or(0))
        .and_then(|info| info.score.clone())
}

#[derive(Clone)]
struct ChessEmojiPalette {
    ids: Vec<&'static str>,
    piece_ids: Vec<&'static str>,
}

impl ChessEmojiPalette {
    fn cell_label(&self, visual: CellVisual, piece: Option<Piece>) -> Option<ButtonLabel> {
        // Do not put a custom emoji in an empty base cell. The native table
        // already owns its background, and the `▫️` fallback would otherwise
        // become a visible white square on clients that cannot resolve the
        // custom-emoji document. The caller still installs an invisible text
        // button so every square remains an action target.
        if visual == CellVisual::Base && piece.is_none() {
            return None;
        }

        if let Some(piece) = piece {
            // Normal pieces use the published set whose metadata matches the
            // fallback emoji. This preserves the correct degraded rendering
            // for clients without rich custom-emoji support.
            if matches!(visual, CellVisual::Base | CellVisual::Legal) {
                let index = piece_index(Some(piece)) - 1;
                let fallback = if piece.color == Color::White {
                    "⚪"
                } else {
                    "⚫"
                };
                return Some(ButtonLabel::custom_emoji(self.piece_ids[index], fallback));
            }
        }

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
        Some(ButtonLabel::custom_emoji(self.ids[index], fallback))
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
    let piece_ids: Vec<_> = include_str!("../../../assets/chess-emoji/piece-ids.txt")
        .lines()
        .filter(|line| !line.is_empty())
        .collect();
    assert_eq!(
        piece_ids.len(),
        12,
        "piece-only chess emoji palette must be complete"
    );
    ChessEmojiPalette { ids, piece_ids }
}

#[derive(Clone)]
struct ChessApp {
    outbound: Outbound<Bot>,
    registry: ActionRegistry<ChessAction>,
    store: InMemoryUiStore<ChessState>,
    worker: SurfaceWorker<Bot>,
    views: Arc<Mutex<HashMap<Surface, ViewId>>>,
    surface_flips: Arc<Mutex<HashMap<Surface, bool>>>,
    emoji_palette: ChessEmojiPalette,
    stockfish: Option<StockfishEngine>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GameMode {
    Stockfish,
    TwoPlayer,
}

fn requested_mode(command: &str, is_private_chat: bool) -> GameMode {
    match command.split_whitespace().nth(1) {
        Some("pvp" | "2p" | "two" | "двое") => GameMode::TwoPlayer,
        Some("bot" | "stockfish" | "solo") => GameMode::Stockfish,
        _ if is_private_chat => GameMode::Stockfish,
        _ => GameMode::TwoPlayer,
    }
}

fn autostart_mode() -> GameMode {
    match std::env::var("TELOXIDE_CHESS_AUTOSTART_MODE") {
        Ok(mode) if mode.eq_ignore_ascii_case("pvp") => GameMode::TwoPlayer,
        _ => GameMode::Stockfish,
    }
}

impl ChessApp {
    fn new(bot: Bot) -> Result<Self, HandlerError> {
        let queue = OutboundQueue::new_spawn(OutboundSettings::default())?;
        let outbound = bot.clone().outbound(queue.clone());
        let stockfish = match StockfishEngine::from_env() {
            Ok(engine) => Some(engine),
            Err(error) => {
                eprintln!("Stockfish is unavailable: {error}");
                None
            }
        };
        Ok(Self {
            outbound,
            registry: ActionRegistry::with_default_ttl(Some(ACTION_TTL)),
            store: InMemoryUiStore::new(),
            worker: SurfaceWorker::new(bot, queue),
            views: Arc::new(Mutex::new(HashMap::new())),
            surface_flips: Arc::new(Mutex::new(HashMap::new())),
            emoji_palette: reference_emoji_palette(),
            stockfish,
        })
    }

    fn emoji_palette(&self) -> ChessEmojiPalette {
        self.emoji_palette.clone()
    }

    async fn start_game(&self, message: &Message) -> Result<(), HandlerError> {
        let mode = requested_mode(
            message.text().unwrap_or_default(),
            message.chat.is_private(),
        );
        if mode == GameMode::Stockfish && self.stockfish.is_none() {
            self.outbound
                .send_message(
                    message.chat.id,
                    "Stockfish is not configured on this bot. Use /chess pvp for a two-player game, or set STOCKFISH_PATH.",
                )
                .await?;
            return Ok(());
        }
        self.start_game_in_chat(
            message.chat.id,
            message.from.as_ref().map(|user| user.id),
            message.from.as_ref().map(player_label),
            mode,
        )
        .await
    }

    async fn start_game_in_chat(
        &self,
        chat_id: ChatId,
        owner: Option<UserId>,
        owner_label: Option<String>,
        mode: GameMode,
    ) -> Result<(), HandlerError> {
        let view_id = ViewId::fresh();
        let state = ChessState::with_mode_and_owner_label(owner, owner_label, mode);
        let palette = self.emoji_palette();
        let rendered = render_game(
            &self.registry,
            view_id,
            Revision::INITIAL,
            &state,
            &palette,
            false,
        )?;

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
        self.register_surface(surface, view_id, false);
        Ok(())
    }

    fn register_surface(&self, surface: Surface, view_id: ViewId, flipped: bool) {
        self.views
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(surface.clone(), view_id);
        self.surface_flips
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(surface, flipped);
    }

    fn binding_for(&self, surface: &Surface) -> Option<(ViewId, bool)> {
        let view_id = self
            .views
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(surface)
            .copied()?;
        let flipped = self
            .surface_flips
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(surface)
            .copied()
            .unwrap_or(false);
        Some((view_id, flipped))
    }

    fn surfaces_for(&self, view_id: ViewId) -> Vec<(Surface, bool)> {
        let views = self
            .views
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let flips = self
            .surface_flips
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        views
            .iter()
            .filter_map(|(surface, current_view)| {
                (*current_view == view_id).then_some((
                    surface.clone(),
                    flips.get(surface).copied().unwrap_or(false),
                ))
            })
            .collect()
    }

    fn flip_surface(&self, surface: &Surface) -> Option<bool> {
        let mut flips = self
            .surface_flips
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let flipped = flips.get_mut(surface)?;
        *flipped = !*flipped;
        Some(*flipped)
    }

    async fn project_record(&self, record: &ViewRecord<ChessState>) -> Result<(), HandlerError> {
        let palette = self.emoji_palette();
        for (surface, flipped) in self.surfaces_for(record.id) {
            let rendered = render_game(
                &self.registry,
                record.id,
                record.revision,
                &record.state,
                &palette,
                flipped,
            )?;
            self.worker
                .project(surface, record.revision, rendered.rich_message)
                .await?;
        }
        Ok(())
    }

    async fn project_surface(
        &self,
        surface: Surface,
        record: &ViewRecord<ChessState>,
        flipped: bool,
    ) -> Result<(), HandlerError> {
        let palette = self.emoji_palette();
        let rendered = render_game(
            &self.registry,
            record.id,
            record.revision,
            &record.state,
            &palette,
            flipped,
        )?;
        self.worker
            .project(surface, record.revision, rendered.rich_message)
            .await?;
        Ok(())
    }

    async fn answer_callback(
        &self,
        callback_query_id: CallbackQueryId,
        text: Option<&str>,
    ) -> Result<(), HandlerError> {
        let request = self.outbound.answer_callback_query(callback_query_id);
        if let Some(text) = text {
            request.text(text).await?;
        } else {
            request.await?;
        }
        Ok(())
    }

    async fn handle_callback(&self, query: CallbackQuery) -> Result<(), HandlerError> {
        let Some(data) = query.data.as_deref() else {
            self.answer_callback(query.id, Some("Кнопка устарела"))
                .await?;
            return Ok(());
        };
        let Ok(token) = teloxide_ui::ActionToken::try_from(data) else {
            self.answer_callback(query.id, Some("Кнопка устарела"))
                .await?;
            return Ok(());
        };
        let Some(surface) = callback_surface(&query) else {
            self.answer_callback(query.id, Some("Игра больше недоступна"))
                .await?;
            return Ok(());
        };
        let Some((view_id, _)) = self.binding_for(&surface) else {
            self.answer_callback(query.id, Some("Игра больше недоступна"))
                .await?;
            return Ok(());
        };
        let Some(record) = self.store.load(view_id)? else {
            self.answer_callback(query.id, Some("Игра больше недоступна"))
                .await?;
            return Ok(());
        };
        let action = match self
            .registry
            .resolve(&token, query.from.id, view_id, record.revision)
        {
            Ok(record) => record.action,
            Err(_) => {
                self.answer_callback(query.id, Some("Кнопка устарела"))
                    .await?;
                return Ok(());
            }
        };

        // Board orientation is a projection preference, not domain state. A
        // flip therefore updates only the surface that produced this callback
        // and does not consume a state revision or affect the other player.
        if matches!(&action, ChessAction::FlipBoard) {
            let Some(flipped) = self.flip_surface(&surface) else {
                self.answer_callback(query.id, Some("Поверхность игры больше недоступна"))
                    .await?;
                return Ok(());
            };
            // ACK is deliberately independent of the following render.
            self.answer_callback(query.id, None).await?;
            self.project_surface(surface, &record, flipped).await?;
            return Ok(());
        }

        let is_join_black = matches!(&action, ChessAction::JoinBlack);
        let white_was_unclaimed = record.state.white_player.is_none();
        let black_was_unclaimed = record.state.black_player.is_none();
        let mut next_state = match transition_for_actor(record.state, Some(query.from.id), action) {
            Ok(state) => state,
            Err(error) => {
                self.answer_callback(query.id, Some(callback_error_text(error)))
                    .await?;
                return Ok(());
            }
        };
        if is_join_black {
            next_state.black_player_label = Some(player_label(&query.from));
        }
        if white_was_unclaimed && next_state.white_player == Some(query.from.id) {
            next_state.white_player_label = Some(player_label(&query.from));
        }
        if black_was_unclaimed && next_state.black_player == Some(query.from.id) {
            next_state.black_player_label = Some(player_label(&query.from));
        }

        // CAS makes two concurrent clicks on one revision deterministic. The
        // loser observes a conflict and leaves the winner's projection alone.
        let updated = match self
            .store
            .compare_and_set(view_id, record.revision, next_state)
        {
            Ok(record) => record,
            Err(_) => {
                self.answer_callback(query.id, Some("Игра уже изменилась, нажмите ещё раз"))
                    .await?;
                return Ok(());
            }
        };

        // The state transition is committed before any slow ephemeral,
        // Stockfish, or Telegram projection work. ACK remains independent of
        // all of those effects.
        self.answer_callback(query.id.clone(), None).await?;

        if is_join_black {
            if let Err(error) = self
                .send_black_ephemeral(&updated, query.from.id, query.id.to_string())
                .await
            {
                eprintln!("cannot create Black's private chess surface: {error}");
            }
        }

        let updated = if updated.state.should_engine_move() {
            match self.stockfish.as_ref() {
                Some(engine) => {
                    let reply = match engine.best_move(&updated.state.board).await {
                        Ok(reply) => reply,
                        Err(error) => {
                            eprintln!("Stockfish move failed: {error}");
                            return self.project_record(&updated).await;
                        }
                    };
                    let score = reply
                        .score
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| "score unavailable".to_owned());
                    let engine_state =
                        match apply_engine_move(updated.state.clone(), &reply.best_move) {
                            Ok(state) => state,
                            Err(error) => {
                                eprintln!("Stockfish returned an illegal move: {error}");
                                return self.project_record(&updated).await;
                            }
                        };
                    eprintln!("Stockfish played {} ({score})", reply.best_move);
                    match self
                        .store
                        .compare_and_set(view_id, updated.revision, engine_state)
                    {
                        Ok(record) => record,
                        Err(_) => updated,
                    }
                }
                None => {
                    eprintln!("Stockfish move skipped: engine is unavailable");
                    updated
                }
            }
        } else {
            updated
        };
        self.project_record(&updated).await?;
        Ok(())
    }

    async fn send_black_ephemeral(
        &self,
        record: &ViewRecord<ChessState>,
        receiver_user_id: UserId,
        callback_query_id: String,
    ) -> Result<(), HandlerError> {
        let Surface::Message { chat_id, .. } = &record.surface else {
            return Ok(());
        };
        let chat_id = *chat_id;
        let palette = self.emoji_palette();
        let rendered = render_game(
            &self.registry,
            record.id,
            record.revision,
            &record.state,
            &palette,
            true,
        )?;
        let sent = self
            .outbound
            .send_rich_message(chat_id, rendered.rich_message)
            .ephemeral_message_parameters(
                EphemeralMessageParameters::new(receiver_user_id)
                    .callback_query_id(callback_query_id)
                    .replace_callback_query_message(true),
            )
            .await?;
        let Some(ephemeral_message_id) = sent.ephemeral_message_id else {
            return Err("Telegram did not return an ephemeral_message_id".into());
        };
        self.register_surface(
            Surface::Ephemeral {
                chat_id,
                receiver_user_id,
                ephemeral_message_id,
            },
            record.id,
            true,
        );
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
        let mode = autostart_mode();
        if mode == GameMode::TwoPlayer || app.stockfish.is_some() {
            app.start_game_in_chat(chat_id, None, None, mode).await?;
            println!("sent chess demo to chat {chat_id:?} ({mode:?})");
        } else {
            eprintln!(
                "skipping chess autostart for {chat_id:?}: Stockfish is unavailable; use /chess pvp or configure STOCKFISH_PATH"
            );
        }
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
        eprintln!(
            "chess command received: chat_id={:?}, title={:?}, private={}",
            message.chat.id,
            message.chat.title(),
            message.chat.is_private()
        );
        if let Err(error) = app.start_game(&message).await {
            eprintln!("chess start failed: {error}");
        }
    }
    Ok(())
}

async fn callback_handler(app: Arc<ChessApp>, query: CallbackQuery) -> Result<(), HandlerError> {
    app.handle_callback(query).await
}

fn callback_surface(query: &CallbackQuery) -> Option<Surface> {
    let message = query.regular_message()?;
    if let Some(ephemeral_message_id) = message.ephemeral_message_id {
        let receiver_user_id = message.receiver_user.as_ref()?.id;
        (receiver_user_id == query.from.id).then_some(Surface::Ephemeral {
            chat_id: message.chat.id,
            receiver_user_id,
            ephemeral_message_id,
        })
    } else {
        Some(Surface::Message {
            chat_id: message.chat.id,
            message_id: message.id,
        })
    }
}

const MAX_PLAYER_LABEL_CHARS: usize = 24;

fn player_label(user: &User) -> String {
    let label = user.full_name();
    let mut chars = label.chars();
    let mut bounded: String = chars.by_ref().take(MAX_PLAYER_LABEL_CHARS).collect();
    if chars.next().is_some() {
        bounded.push('…');
    }
    bounded
}

fn callback_error_text(error: &'static str) -> &'static str {
    match error {
        "not your turn" => "Сейчас ходит другой игрок",
        "not your piece" => "Это не ваша фигура",
        "illegal move" => "Так ходить нельзя",
        "game is finished" | "game is over" => "Игра уже завершена",
        "black is already occupied" => "Чёрный уже занят другим игроком",
        "white player cannot join as black" => "Белые не могут занять чёрных",
        "nothing to undo" => "Нечего отменять",
        _ => "Действие сейчас недоступно",
    }
}

fn render_game(
    registry: &ActionRegistry<ChessAction>,
    view_id: ViewId,
    revision: Revision,
    state: &ChessState,
    palette: &ChessEmojiPalette,
    flipped: bool,
) -> Result<teloxide_ui::RenderedRichMessage, teloxide_ui::RenderError> {
    let renderer = RichRenderer::new(registry.clone()).action_ttl(Some(ACTION_TTL));
    renderer.render(
        &view(state, palette, flipped),
        RenderContext::new(view_id, revision)
            .actor(actor_policy(state))
            .stale_policy(StalePolicy::Reject),
    )
}

fn actor_policy(state: &ChessState) -> ActorPolicy {
    if state.mode == GameMode::TwoPlayer {
        ActorPolicy::Any
    } else {
        state.owner.map_or(ActorPolicy::Any, ActorPolicy::User)
    }
}

fn view(state: &ChessState, palette: &ChessEmojiPalette, flipped: bool) -> Ui<ChessAction> {
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
                match state.mode {
                    GameMode::Stockfish => format!("Your move as {}{check}", turn.label()),
                    GameMode::TwoPlayer => state.player_label(turn).map_or_else(
                        || format!("{} to move{check}", turn.label()),
                        |label| format!("{label} to move{check}"),
                    ),
                }
            }
        }
    };
    let files: Vec<i8> = if flipped {
        (0..8).rev().collect()
    } else {
        (0..8).collect()
    };
    let ranks: Vec<i8> = if flipped {
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
            let cell = palette
                .cell_label(cell_visual, board_piece(&state.board, square))
                .map_or_else(
                    || {
                        // Keep empty squares interactive without rendering a
                        // fallback glyph. U+2063 is a real, non-empty Rich
                        // Text value and occupies no visible space.
                        TableCell::button(
                            ButtonLabel::Plain(INVISIBLE_EMPTY_CELL_LABEL.to_owned()),
                            ChessAction::Square(square.0),
                        )
                    },
                    |label| TableCell::button(label, ChessAction::Square(square.0)),
                )
                // Link style removes native rounded button chrome; the
                // table cell supplies the square background.
                .style(teloxide_ui::ButtonStyle::Link)
                .header(light);
            row.push(cell);
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
    if state.mode == GameMode::TwoPlayer {
        let join_label = if state.black_player.is_some() {
            "✓ Black joined"
        } else {
            "Join as Black"
        };
        ui = ui.push(
            Ui::button_row().push(
                Ui::button(join_label, ChessAction::JoinBlack)
                    .disabled(state.finished || state.black_player.is_some()),
            ),
        );
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

#[cfg(test)]
fn transition(state: ChessState, action: ChessAction) -> Result<ChessState, &'static str> {
    transition_for_actor(state, None, action)
}

fn transition_for_actor(
    mut state: ChessState,
    actor: Option<UserId>,
    action: ChessAction,
) -> Result<ChessState, &'static str> {
    match action {
        ChessAction::Reset => {
            ensure_manager(&state, actor)?;
            let owner = state.owner;
            let owner_label = state.white_player_label.clone();
            let mode = state.mode;
            state = ChessState::with_mode_and_owner_label(owner, owner_label, mode);
        }
        ChessAction::FlipBoard => {
            // Board orientation belongs to the rendered surface, not the
            // authoritative game state. ChessApp handles this action locally.
        }
        ChessAction::Undo => {
            ensure_participant(&state, actor)?;
            let undo_count = if state.mode == GameMode::Stockfish {
                state.history.len().min(2)
            } else {
                1
            };
            if undo_count == 0 {
                return Err("nothing to undo");
            }
            for _ in 0..undo_count {
                let previous = state.history.pop().ok_or("nothing to undo")?;
                state.board = previous.board;
                state.moves.pop();
            }
            state.selected = None;
            state.finished = false;
        }
        ChessAction::Finish => {
            ensure_manager(&state, actor)?;
            state.finished = true;
            state.selected = None;
        }
        ChessAction::JoinBlack => {
            let actor = actor.ok_or("actor required")?;
            if state.mode != GameMode::TwoPlayer {
                return Err("joining is only available in two-player mode");
            }
            if state.finished {
                return Err("game is finished");
            }
            if state.white_player == Some(actor) {
                return Err("white player cannot join as black");
            }
            if state.black_player.is_some() {
                return Err("black is already occupied");
            }
            state.black_player = Some(actor);
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
            if !state.can_play(actor, turn) {
                return Err("not your turn");
            }
            match state.selected {
                None => {
                    if state.board.color_on(engine_square) == Some(turn) {
                        if let Some(actor) = actor {
                            match turn {
                                EngineColor::White if state.white_player.is_none() => {
                                    state.white_player = Some(actor);
                                }
                                EngineColor::Black if state.black_player.is_none() => {
                                    state.black_player = Some(actor);
                                }
                                _ => {}
                            }
                        }
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
                    let mv = legal_move_for(&state.board, from, square).ok_or("illegal move")?;
                    apply_move(&mut state, mv)?;
                }
                Some(_) => return Err("illegal move"),
            }
        }
    }
    Ok(state)
}

fn ensure_manager(state: &ChessState, actor: Option<UserId>) -> Result<(), &'static str> {
    match (state.owner, actor) {
        (Some(owner), Some(actor)) if owner != actor => Err("only the game creator can do that"),
        _ => Ok(()),
    }
}

fn ensure_participant(state: &ChessState, actor: Option<UserId>) -> Result<(), &'static str> {
    match actor {
        Some(actor)
            if !(state.is_participant(actor)
                || (state.owner.is_none()
                    && state.white_player.is_none()
                    && state.black_player.is_none())) =>
        {
            Err("only a player can do that")
        }
        _ => Ok(()),
    }
}

fn apply_move(state: &mut ChessState, mv: cozy_chess::Move) -> Result<(), &'static str> {
    let from = mv.from;
    let to = mv.to;
    let from_coordinate = format!("{from}");
    let to_coordinate = format!("{to}");
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
    Ok(())
}

fn apply_engine_move(mut state: ChessState, uci: &str) -> Result<ChessState, &'static str> {
    if !state.should_engine_move() {
        return Err("engine is not to move");
    }
    let mv = parse_uci_move(&state.board, uci).map_err(|_| "engine returned an invalid move")?;
    if !state.board.is_legal(mv) {
        return Err("engine returned an illegal move");
    }
    apply_move(&mut state, mv)?;
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
    JoinBlack,
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
    mode: GameMode,
    white_player: Option<UserId>,
    white_player_label: Option<String>,
    black_player: Option<UserId>,
    black_player_label: Option<String>,
    history: Vec<ChessPosition>,
    moves: Vec<String>,
    finished: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct ChessPosition {
    board: ChessBoard,
}

impl ChessState {
    fn new(owner: Option<UserId>) -> Self {
        Self::with_mode(owner, GameMode::TwoPlayer)
    }

    fn with_mode(owner: Option<UserId>, mode: GameMode) -> Self {
        Self::with_mode_and_owner_label(owner, None, mode)
    }

    fn with_mode_and_owner_label(
        owner: Option<UserId>,
        owner_label: Option<String>,
        mode: GameMode,
    ) -> Self {
        Self {
            board: ChessBoard::default(),
            selected: None,
            owner,
            mode,
            white_player: owner,
            white_player_label: owner_label,
            black_player: None,
            black_player_label: None,
            history: Vec::new(),
            moves: Vec::new(),
            finished: false,
        }
    }

    fn is_participant(&self, actor: UserId) -> bool {
        self.white_player == Some(actor) || self.black_player == Some(actor)
    }

    fn can_play(&self, actor: Option<UserId>, color: EngineColor) -> bool {
        if actor.is_none() {
            return true;
        }
        if self.mode == GameMode::Stockfish && color == EngineColor::Black {
            return false;
        }
        let player = match color {
            EngineColor::White => self.white_player,
            EngineColor::Black => self.black_player,
        };
        if player == actor {
            return true;
        }
        if player.is_some() {
            return false;
        }
        if self.mode == GameMode::TwoPlayer {
            return match color {
                EngineColor::White => self.black_player != actor,
                EngineColor::Black => self.white_player != actor,
            };
        }
        true
    }

    fn should_engine_move(&self) -> bool {
        self.mode == GameMode::Stockfish
            && self.board.side_to_move() == EngineColor::Black
            && !self.finished
            && self.board.status() == GameStatus::Ongoing
    }

    fn player_label(&self, color: Color) -> Option<&str> {
        match color {
            Color::White => self.white_player_label.as_deref(),
            Color::Black => self.black_player_label.as_deref(),
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
    fn first_pvp_player_is_bound_to_white_and_cannot_join_black() {
        let white = UserId(10);
        let black = UserId(20);
        let selected = transition_for_actor(
            ChessState::new(None),
            Some(white),
            ChessAction::Square(square("e2").0),
        )
        .unwrap();

        assert_eq!(selected.white_player, Some(white));
        assert_eq!(
            transition_for_actor(selected.clone(), Some(white), ChessAction::JoinBlack),
            Err("white player cannot join as black")
        );
        assert_eq!(
            transition_for_actor(selected, Some(black), ChessAction::Square(square("d2").0)),
            Err("not your turn")
        );
    }

    #[test]
    fn interactive_board_cells_have_server_actions() {
        let registry = ActionRegistry::new();
        let rendered = render_game(
            &registry,
            ViewId::new(1),
            Revision::INITIAL,
            &ChessState::with_mode(None, GameMode::Stockfish),
            &reference_emoji_palette(),
            false,
        )
        .unwrap();
        // Every cell carries a complete fixed-size sprite and therefore keeps
        // a server-side action token even when it is empty. The enabled Flip
        // and Finish controls add the remaining tokens; Undo is disabled at
        // the initial position.
        assert_eq!(rendered.action_tokens.len(), 66);
    }

    #[test]
    fn empty_cells_do_not_use_visible_custom_emoji_fallbacks() {
        let palette = reference_emoji_palette();
        let ui = view(&ChessState::new(None), &palette, false);
        let UiNode::Table(table) = &ui.nodes[0] else {
            panic!("expected the board to be the first node");
        };

        let cell = match &table.rows[4][1] {
            TableCell::Header(inner) => inner.as_ref(),
            cell => cell,
        };
        assert!(matches!(
            cell,
            TableCell::Button(button)
                if button.text == ButtonLabel::Plain(INVISIBLE_EMPTY_CELL_LABEL.to_owned())
        ));

        // e2 is a white pawn. Both PVP and engine projections use the same
        // piece-only custom emoji, so the group and private board match.
        let cell = match &table.rows[7][5] {
            TableCell::Header(inner) => inner.as_ref(),
            cell => cell,
        };
        assert!(matches!(
            cell,
            TableCell::Button(button)
                if button.text == ButtonLabel::custom_emoji(palette.piece_ids[6], "⚪")
        ));

        // Engine/private projections use the same published palette.
        let private_ui = view(
            &ChessState::with_mode(None, GameMode::Stockfish),
            &palette,
            false,
        );
        let UiNode::Table(table) = &private_ui.nodes[0] else {
            panic!("expected the board to be the first node");
        };
        let cell = match &table.rows[7][5] {
            TableCell::Header(inner) => inner.as_ref(),
            cell => cell,
        };
        assert!(matches!(
            cell,
            TableCell::Button(button)
                if button.text == ButtonLabel::custom_emoji(palette.piece_ids[6], "⚪")
        ));
    }

    #[test]
    fn selecting_a_piece_keeps_the_same_action_grid() {
        let selected = transition(
            ChessState::with_mode(None, GameMode::Stockfish),
            ChessAction::Square(square("d1").0),
        )
        .unwrap();
        let registry = ActionRegistry::new();
        let rendered = render_game(
            &registry,
            ViewId::new(1),
            Revision::INITIAL,
            &selected,
            &reference_emoji_palette(),
            false,
        )
        .unwrap();
        assert_eq!(rendered.action_tokens.len(), 66);
    }

    #[test]
    fn board_uses_native_headers_for_light_squares() {
        let ui = view(&ChessState::new(None), &reference_emoji_palette(), false);
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
    fn finished_board_keeps_cell_geometry_and_server_actions() {
        let mut state = ChessState::new(None);
        state.finished = true;
        let ui = view(&state, &reference_emoji_palette(), false);
        let UiNode::Table(table) = &ui.nodes[0] else {
            panic!("expected the board to be the first node");
        };

        for row in &table.rows[1..9] {
            for cell in &row[1..9] {
                let cell = match cell {
                    TableCell::Header(inner) => inner.as_ref(),
                    cell => cell,
                };
                assert!(matches!(cell, TableCell::Button(button) if !button.disabled));
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
    fn requested_mode_defaults_to_engine_in_private_chats_and_pvp_in_groups() {
        assert_eq!(requested_mode("/chess", true), GameMode::Stockfish);
        assert_eq!(requested_mode("/chess", false), GameMode::TwoPlayer);
        assert_eq!(requested_mode("/chess pvp", true), GameMode::TwoPlayer);
        assert_eq!(requested_mode("/chess bot", false), GameMode::Stockfish);
    }

    #[test]
    fn callback_errors_are_short_and_actionable() {
        assert_eq!(
            callback_error_text("not your turn"),
            "Сейчас ходит другой игрок"
        );
        assert_eq!(callback_error_text("not your piece"), "Это не ваша фигура");
        assert_eq!(callback_error_text("illegal move"), "Так ходить нельзя");
    }

    #[test]
    fn two_player_status_uses_the_player_label_for_the_current_turn() {
        let mut state = ChessState::new(Some(UserId(10)));
        state.white_player_label = Some("White Player".to_owned());
        let ui = view(&state, &reference_emoji_palette(), false);
        assert_eq!(
            ui.nodes[1],
            UiNode::Blockquote("White Player to move".to_owned())
        );

        let mut state = play(state, "e2", "e4");
        state.black_player_label = Some("Black Player".to_owned());
        let ui = view(&state, &reference_emoji_palette(), false);
        assert_eq!(
            ui.nodes[1],
            UiNode::Blockquote("Black Player to move".to_owned())
        );
    }

    #[test]
    fn two_player_mode_adds_one_stable_join_action() {
        let registry = ActionRegistry::new();
        let rendered = render_game(
            &registry,
            ViewId::new(1),
            Revision::INITIAL,
            &ChessState::new(None),
            &reference_emoji_palette(),
            false,
        )
        .unwrap();
        assert_eq!(rendered.action_tokens.len(), 67);
    }

    #[test]
    fn black_join_is_bound_to_one_player_and_cannot_replace_it() {
        let first = UserId(11);
        let second = UserId(12);
        let state = ChessState::new(Some(UserId(10)));
        let state = transition_for_actor(state, Some(first), ChessAction::JoinBlack).unwrap();
        assert_eq!(state.black_player, Some(first));
        assert_eq!(
            transition_for_actor(state.clone(), Some(second), ChessAction::JoinBlack),
            Err("black is already occupied")
        );
        let state = play(state, "e2", "e4");
        assert_eq!(
            transition_for_actor(state, Some(first), ChessAction::Square(square("e7").0))
                .map(|state| state.selected),
            Ok(Some(square("e7")))
        );
    }

    #[test]
    fn stockfish_mode_rejects_external_black_moves() {
        let state = ChessState::with_mode(Some(UserId(10)), GameMode::Stockfish);
        let state = play(state, "e2", "e4");
        assert_eq!(
            transition_for_actor(state, Some(UserId(10)), ChessAction::Square(square("e7").0)),
            Err("not your turn")
        );
    }

    #[test]
    fn engine_undo_returns_to_the_human_turn() {
        let state = ChessState::with_mode(Some(UserId(10)), GameMode::Stockfish);
        let state = play(state, "e2", "e4");
        let state = apply_engine_move(state, "e7e5").unwrap();
        let state = transition_for_actor(state, Some(UserId(10)), ChessAction::Undo).unwrap();
        assert_eq!(state.board.side_to_move(), EngineColor::White);
        assert!(state.moves.is_empty());
    }

    #[test]
    fn engine_analysis_prefers_the_deepest_primary_line() {
        let shallow = AnalysisInfo {
            depth: Some(8),
            seldepth: None,
            score: Some(Score::Centipawns(12)),
            pv: vec!["e7e5".to_owned()],
            nodes: None,
            nps: None,
            time_ms: None,
            hashfull: None,
            multipv: Some(1),
            message: None,
        };
        let deepest = AnalysisInfo {
            depth: Some(12),
            score: Some(Score::Centipawns(34)),
            ..shallow.clone()
        };
        let alternative = AnalysisInfo {
            multipv: Some(2),
            depth: Some(99),
            score: Some(Score::Centipawns(900)),
            ..deepest.clone()
        };
        assert_eq!(
            deepest_score(&[shallow, alternative, deepest]),
            Some(Score::Centipawns(34))
        );
    }

    #[test]
    fn flip_and_undo_are_stateful_controls() {
        let state = ChessState::new(None);
        let state = transition(state, ChessAction::FlipBoard).unwrap();
        assert_eq!(state, ChessState::new(None));
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
    fn board_orientation_is_a_surface_projection_preference() {
        let state = ChessState::new(None);
        let normal = view(&state, &reference_emoji_palette(), false);
        let flipped = view(&state, &reference_emoji_palette(), true);
        let UiNode::Table(normal) = &normal.nodes[0] else {
            panic!("expected the normal board to be the first node");
        };
        let UiNode::Table(flipped) = &flipped.nodes[0] else {
            panic!("expected the flipped board to be the first node");
        };
        assert!(matches!(normal.rows[0][1], TableCell::Text(_)));
        assert!(matches!(flipped.rows[0][1], TableCell::Text(_)));
        assert_eq!(normal.rows[1][0], TableCell::text("8"));
        assert_eq!(flipped.rows[1][0], TableCell::text("1"));
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
