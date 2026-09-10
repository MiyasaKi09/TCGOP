//! End-to-end head-less tests: the whole client, assembled the way `main.rs`
//! assembles it, but on `MinimalPlugins` — no window, no renderer, no asset
//! server.
//!
//! Two things are proven here that no unit test can prove on its own:
//!
//! 1. **The plugins compose.** Every feature plugin is added, in the order
//!    [`TcgopPlugin`](crate::app::TcgopPlugin) uses, so a duplicated message
//!    registration, a missing resource or a cycle in the
//!    [`AppSet`] chain shows up as a panic while the schedule is built.
//! 2. **A whole game runs to its end through the message pipeline.** The AI
//!    plays for itself at `Expert`; the human seat is a scripted bot that
//!    always dispatches `Session::valid[0]`. Nothing here reaches into the
//!    engine directly — every move travels
//!    `DispatchAction → bridge → StateChanged → AppScreen::GameOver`.
//!
//! Rendering systems are all guarded on `resource_exists::<AssetServer>` (or
//! take `Option<Res<AssetServer>>`), so they simply never run here — which is
//! exactly the head-less contract the module docs claim.

use core::time::Duration;
use std::collections::HashSet;

use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::time::TimeUpdateStrategy;

use tcgop_engine::ai::Difficulty;
use tcgop_engine::decks::{marines_deck, mugiwara_deck};
use tcgop_engine::types::GameAction;

use crate::ai_driver::AiDriverPlugin;
use crate::app::{AppScreen, AppSet, Fonts, Palette};
use crate::art::ArtPlugin;
use crate::board::BoardPlugin;
use crate::bridge::{BridgePlugin, DispatchAction, EngineErrorEvent, Session};
use crate::hand::HandPlugin;
use crate::panels::PanelsPlugin;
use crate::screens::ScreensPlugin;
use crate::selection::SelectionPlugin;
use crate::vfx::VfxPlugin;

/// One simulated frame. Long enough that the AI's dramatic pauses (and the
/// reveal durations they absorb) elapse in a handful of updates.
const FRAME: Duration = Duration::from_millis(100);

/// Hard ceiling on the simulation. A One Piece game is a few hundred actions;
/// this is two orders of magnitude of head-room, and its only job is to turn a
/// hang into a readable failure.
const MAX_FRAMES: usize = 20_000;

/// The client, head-less: `MinimalPlugins` + states + **every** gameplay
/// plugin, plus the two resources `TcgopPlugin` would have inserted before
/// them.
///
/// `TcgopPlugin` itself is deliberately not used: it also spawns the 2D camera,
/// which belongs to the rendering half.
fn headless_app() -> App {
    let mut app = App::new();
    app.add_plugins(bevy::MinimalPlugins)
        .add_plugins(StatesPlugin)
        .insert_resource(TimeUpdateStrategy::ManualDuration(FRAME))
        .insert_resource(Palette::default())
        .insert_resource(Fonts::default())
        .init_state::<AppScreen>()
        // Same order as `TcgopPlugin::build`: core, then presentation, then
        // the opponent.
        .add_plugins((BridgePlugin, ArtPlugin, SelectionPlugin))
        .add_plugins((ScreensPlugin, BoardPlugin, HandPlugin, PanelsPlugin, VfxPlugin))
        .add_plugins(AiDriverPlugin);
    app
}

/// Start a game and walk to `AppScreen::Board`, exactly like the setup screen's
/// *Commencer* button does.
fn start_session(app: &mut App, level: Difficulty, seed: u64) {
    let session = Session::new(&mugiwara_deck(), &marines_deck(), level, seed)
        .expect("the shipped decks are valid");
    app.insert_resource(session);
    app.world_mut()
        .resource_mut::<NextState<AppScreen>>()
        .set(AppScreen::Board);
    // One update to run the transition schedule and `OnEnter(Board)`.
    app.update();
}

/// What the scripted human seat did.
#[derive(Debug, Default)]
struct Played {
    frames: usize,
    /// Actions the human bot dispatched.
    human: usize,
    /// How many of those the engine refused (see [`play_out`]).
    refused: usize,
    /// Whether the loop ended because the engine declared a winner.
    finished: bool,
}

/// Run the game to its end: every frame the human seat dispatches the **first**
/// action of `Session::valid`, then the app is advanced by one [`FRAME`].
///
/// One wrinkle: `valid_actions_for` is occasionally more permissive than
/// `apply` — it will offer a `playEvent` whose card then refuses its own
/// pre-condition (`MG-024` *Flashback* is listed with an empty graveyard and
/// then rejected with *"aucun de vos personnages n'a été KO ce match"*). The
/// client is right to trust `valid`; a human simply clicks something else. So
/// the bot remembers what the engine has just refused and takes the next
/// candidate instead — otherwise a single such card wedges the game forever.
/// `Played::refused` reports how often that happened.
fn play_out(app: &mut App, level_name: &str) -> Played {
    let mut out = Played::default();
    // Actions `apply` rejected; cleared as soon as the state moves on, because
    // the same action may become legal later (a KO fills the graveyard).
    let mut refused: HashSet<GameAction> = HashSet::new();
    let mut last_version = state_version(app);

    for frame in 0..MAX_FRAMES {
        out.frames = frame + 1;
        if app.world().resource::<Session>().winner().is_some() {
            out.finished = true;
            break;
        }

        let version = state_version(app);
        if version != last_version {
            last_version = version;
            refused.clear();
        }

        let session = app.world().resource::<Session>();
        let pick = session
            .valid
            .iter()
            .find(|a| !refused.contains(*a))
            .cloned()
            // Everything on offer was refused: the two answers that are always
            // legal for whoever is on the clock.
            .or_else(|| {
                (!session.valid.is_empty()).then(|| {
                    if session.in_counter_window() {
                        GameAction::PassCounter
                    } else {
                        GameAction::EndTurn
                    }
                })
            });

        if let Some(action) = pick {
            app.world_mut().write_message(DispatchAction(action.clone()));
            out.human += 1;
            app.update();
            if app
                .world()
                .resource::<Messages<EngineErrorEvent>>()
                .iter_current_update_messages()
                .any(|e| e.action == action)
            {
                out.refused += 1;
                refused.insert(action);
            }
        } else {
            app.update();
        }
    }

    assert!(
        out.finished,
        "[{level_name}] the game never ended: {out:?}, stopped on turn {}",
        app.world().resource::<Session>().state.turn_number
    );
    out
}

/// A cheap "did the engine state move?" stamp: the turn number plus how much
/// has been written to the log.
fn state_version(app: &App) -> (u32, usize) {
    let session = app.world().resource::<Session>();
    (session.state.turn_number, session.state.log.len())
}

#[test]
fn a_whole_game_runs_head_lessly_from_setup_to_game_over() {
    let mut app = headless_app();
    start_session(&mut app, Difficulty::Expert, 20_260_910);

    let out = play_out(&mut app, "expert");
    assert!(
        out.human > 5,
        "the scripted human really played its own moves: {out:?}"
    );

    // `StateTransition` runs before `Update`, so the screen swap `watch_for_winner`
    // requested lands on the frame *after* the winning blow.
    app.update();
    assert_eq!(
        *app.world().resource::<State<AppScreen>>().get(),
        AppScreen::GameOver,
        "a winner ends the game"
    );

    let session = app.world().resource::<Session>();
    assert!(session.winner().is_some());
    assert!(
        session.valid.is_empty(),
        "a finished game offers no more moves"
    );
}

/// The same run at every difficulty — the AI level must not be able to wedge
/// the loop, and each seed must still terminate.
#[test]
fn every_difficulty_terminates() {
    for (level, name, seed) in [
        (Difficulty::Beginner, "beginner", 7),
        (Difficulty::Intermediate, "intermediate", 4_242),
        (Difficulty::Expert, "expert", 99_001),
    ] {
        let mut app = headless_app();
        start_session(&mut app, level, seed);
        let out = play_out(&mut app, name);
        assert!(out.human > 0, "[{name}] {out:?}");
        app.update();
        assert_eq!(
            *app.world().resource::<State<AppScreen>>().get(),
            AppScreen::GameOver,
            "[{name}] {out:?}"
        );
    }
}

/// A second game must start from a clean slate.
///
/// `Session` outlives the board — it is only dropped when *Rejouer* is pressed
/// — and instance ids repeat verbatim from one seeded game to the next
/// (`{def_id}_{n}_0`), so anything a module remembers across the transition
/// diffs cleanly against the new game and fires phantom feedback: a heal on the
/// loser's captain going from 0 PV back to full, a hover preview on a card
/// nobody is pointing at, combat events replayed onto the wrong tiles.
#[test]
fn replaying_starts_from_a_clean_slate() {
    use crate::hand::HoveredHandCard;
    use crate::screens::ReplayRequested;
    use crate::vfx::{RevealQueue, VfxHistory};

    let mut app = headless_app();
    start_session(&mut app, Difficulty::Expert, 20_260_910);
    play_out(&mut app, "expert");
    app.update();
    assert_eq!(
        *app.world().resource::<State<AppScreen>>().get(),
        AppScreen::GameOver
    );

    // Pretend the player was hovering a card when the game ended.
    let stale = app.world().resource::<Session>().you().hand.first().cloned();
    app.world_mut().resource_mut::<HoveredHandCard>().0 = stale.clone();

    // *Rejouer* → setup → a brand new session on the board.
    app.world_mut().write_message(ReplayRequested);
    app.update();
    app.update();
    assert_eq!(
        *app.world().resource::<State<AppScreen>>().get(),
        AppScreen::Setup
    );
    assert!(app.world().get_resource::<Session>().is_none());

    start_session(&mut app, Difficulty::Expert, 20_260_910);
    app.update();

    let history = app.world().resource::<VfxHistory>();
    assert!(
        history.previous_state.is_none() || history.last_pending.is_none(),
        "the finished game must not survive into the new one"
    );
    assert!(
        app.world().resource::<RevealQueue>().is_empty(),
        "game 2 opens with an empty reveal queue"
    );
    assert_eq!(
        app.world().resource::<HoveredHandCard>().0,
        None,
        "a hover from game 1 must not match a card of game 2"
    );

    // And it really is playable again.
    let out = play_out(&mut app, "expert-replay");
    assert!(out.finished, "the second game must run to its end too");
}

/// Head-less means *no* rendering happened: without an `AssetServer` not a
/// single UI node may be spawned, and the vfx layers must stay unbuilt.
#[test]
fn nothing_is_rendered_without_an_asset_server() {
    let mut app = headless_app();
    start_session(&mut app, Difficulty::Expert, 5);
    for _ in 0..50 {
        app.update();
    }
    let nodes = app.world_mut().query::<&Node>().iter(app.world()).count();
    assert_eq!(nodes, 0, "the render half stayed asleep");
}

// ============================================================
// The frame pipeline
// ============================================================

/// Records the order [`AppSet`] members actually ran in.
#[derive(Resource, Default)]
struct Trace(Vec<&'static str>);

/// The six stages must run in the declared order, whatever order the plugins
/// were added in — this is the contract every feature plugin's `.in_set(...)`
/// relies on.
#[test]
fn the_frame_pipeline_runs_input_selection_dispatch_refresh_render_vfx() {
    let mut app = headless_app();
    app.init_resource::<Trace>().add_systems(
        Update,
        (
            (|mut t: ResMut<Trace>| t.0.push("vfx")).in_set(AppSet::Vfx),
            (|mut t: ResMut<Trace>| t.0.push("render")).in_set(AppSet::Render),
            (|mut t: ResMut<Trace>| t.0.push("refresh")).in_set(AppSet::Refresh),
            (|mut t: ResMut<Trace>| t.0.push("dispatch")).in_set(AppSet::Dispatch),
            (|mut t: ResMut<Trace>| t.0.push("selection")).in_set(AppSet::Selection),
            (|mut t: ResMut<Trace>| t.0.push("input")).in_set(AppSet::Input),
        ),
    );
    app.update();

    assert_eq!(
        app.world().resource::<Trace>().0,
        vec![
            "input",
            "selection",
            "dispatch",
            "refresh",
            "render",
            "vfx"
        ]
    );
}
