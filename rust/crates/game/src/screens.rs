//! The two screens around the board: [`AppScreen::Setup`] and
//! [`AppScreen::GameOver`], plus the transition **into** game over.
//!
//! - [`setup`] is the port of `src/app/page.tsx`: pick your crew, the AI's crew
//!   and its difficulty, then *Commencer le combat* builds the
//!   [`Session`](crate::bridge::Session) with a fresh seed and enters
//!   [`AppScreen::Board`].
//! - [`game_over`] is the port of the `if (state.winner)` early return of
//!   `src/components/Game.tsx`: VICTOIRE / DÉFAITE, the turn count and
//!   *Rejouer*.
//! - [`model`] holds everything both screens *decide* — the deck catalogue, the
//!   level list, the selection and the splash — as plain values, so the whole
//!   screen logic is unit-tested under `MinimalPlugins`.
//!
//! Clicks never act directly: they raise [`StartGameRequested`] /
//! [`ReplayRequested`], which the two systems below turn into a session and a
//! state change. Other features may write those messages too (a keyboard
//! shortcut, a debug menu) and get exactly the same behaviour.
//!
//! **The board log** (`state.log.slice(-12).reverse()`) is *not* built here:
//! it lives in the footer that [`crate::hand`] owns, next to the action column,
//! as [`crate::hand::log_lines`] + its `update_log` system. Spawning a second
//! panel would double it on screen.

pub mod game_over;
pub mod model;
pub mod setup;
pub mod widgets;

use bevy::prelude::*;

use crate::app::{AppScreen, AppSet, configure_pipeline};
use crate::bridge::{BridgeSet, Session};

use model::SetupSelection;

// ============================================================
// Messages
// ============================================================

/// "Commencer le combat" was activated. Handled by
/// [`setup::handle_start_request`], which is a no-op while the selection is
/// incomplete (TS `disabled={!ready}`).
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StartGameRequested;

/// "Rejouer" was activated: drop the session and go back to the deck picker.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ReplayRequested;

// ============================================================
// Plugin
// ============================================================

/// Setup screen, game-over screen and the transitions between them.
pub struct ScreensPlugin;

/// The screen systems, so other features can order against them.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScreensSet;

impl Plugin for ScreensPlugin {
    fn build(&self, app: &mut App) {
        configure_pipeline(app);
        app.init_resource::<SetupSelection>()
            .add_message::<StartGameRequested>()
            .add_message::<ReplayRequested>()
            // --- setup ---
            .add_systems(
                OnEnter(AppScreen::Setup),
                (drop_session, setup::spawn_setup).chain(),
            )
            .add_systems(
                Update,
                (setup::sync_selection, setup::handle_start_request)
                    .chain()
                    .in_set(ScreensSet)
                    .in_set(AppSet::Input)
                    .run_if(in_state(AppScreen::Setup)),
            )
            // --- board → game over ---
            .add_systems(
                Update,
                watch_for_winner
                    .in_set(ScreensSet)
                    .in_set(AppSet::Render)
                    .after(BridgeSet)
                    .run_if(in_state(AppScreen::Board))
                    .run_if(resource_exists::<Session>),
            )
            // --- game over ---
            .add_systems(OnEnter(AppScreen::GameOver), game_over::spawn_game_over)
            .add_systems(
                Update,
                game_over::handle_replay_request
                    .in_set(ScreensSet)
                    .in_set(AppSet::Input)
                    .run_if(in_state(AppScreen::GameOver)),
            );
    }
}

/// Coming back to the deck picker always starts from a clean slate — the
/// *Rejouer* handler already dropped the session, this is the safety net for
/// any other route into [`AppScreen::Setup`].
fn drop_session(mut commands: Commands) {
    commands.remove_resource::<Session>();
}

/// The engine sets `state.winner` inside `apply`, so the client only has to
/// notice it — TS `if (state.winner) return <GameOver/>`, which happens on the
/// very next render with no animation in between.
///
/// Checking the session directly (rather than gating on
/// [`StateChanged`](crate::bridge::StateChanged)) also covers a session that is
/// inserted already finished, e.g. by a test or a save.
fn watch_for_winner(session: Res<Session>, mut next: ResMut<NextState<AppScreen>>) {
    if session.winner().is_some() {
        next.set(AppScreen::GameOver);
    }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Fonts, Palette};
    use crate::art::ArtCache;
    use crate::bridge::BridgePlugin;
    use crate::selection::{SelectedHandCard, SelectionPlugin, UiMode};
    use game_over::ReplayButton;
    use model::{DeckKey, LEVELS, PickSide};
    use setup::{DeckCardButton, DeckCardCheck, LevelButton, StartButton};
    use tcgop_engine::ai::Difficulty;
    use tcgop_engine::types::PlayerId;

    /// A head-less app with the screens wired up but no window, no assets and
    /// therefore no font / image loading.
    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(bevy::MinimalPlugins)
            .add_plugins(bevy::state::app::StatesPlugin)
            .init_state::<AppScreen>()
            .insert_resource(Palette::default())
            .insert_resource(Fonts::default())
            .init_resource::<ArtCache>()
            .add_plugins((BridgePlugin, SelectionPlugin, ScreensPlugin));
        app
    }

    fn count<C: Component>(app: &mut App) -> usize {
        app.world_mut().query::<&C>().iter(app.world()).count()
    }

    fn screen(app: &App) -> AppScreen {
        *app.world().resource::<State<AppScreen>>().get()
    }

    #[test]
    fn the_setup_screen_lists_both_crew_rows_the_levels_and_the_cta() {
        let mut app = app();
        app.update();

        assert_eq!(screen(&app), AppScreen::Setup);
        assert_eq!(
            count::<DeckCardButton>(&mut app),
            DeckKey::ALL.len() * 2,
            "four crews on each of the two rows"
        );
        assert_eq!(count::<LevelButton>(&mut app), LEVELS.len());
        assert_eq!(count::<StartButton>(&mut app), 1);
        assert_eq!(count::<DeckCardCheck>(&mut app), DeckKey::ALL.len() * 2);
    }

    #[test]
    fn only_the_picked_crew_shows_its_check_badge() {
        let mut app = app();
        app.update();

        app.world_mut()
            .resource_mut::<SetupSelection>()
            .pick(PickSide::You, DeckKey::Baroque);
        app.update();

        let mut shown = Vec::new();
        let mut query = app.world_mut().query::<(&DeckCardCheck, &Node)>();
        for (check, node) in query.iter(app.world()) {
            if node.display == Display::Flex {
                shown.push((check.side, check.key));
            }
        }
        assert_eq!(shown, vec![(PickSide::You, DeckKey::Baroque)]);
    }

    #[test]
    fn starting_is_refused_until_both_crews_are_picked() {
        let mut app = app();
        app.update();

        app.world_mut().write_message(StartGameRequested);
        app.update();
        assert!(
            app.world().get_resource::<Session>().is_none(),
            "an incomplete selection must not build a game"
        );
        assert_eq!(screen(&app), AppScreen::Setup);
    }

    #[test]
    fn starting_builds_the_session_with_the_picked_decks_and_level() {
        let mut app = app();
        app.update();

        {
            let mut selection = app.world_mut().resource_mut::<SetupSelection>();
            selection.pick(PickSide::You, DeckKey::RedHair);
            selection.pick(PickSide::Foe, DeckKey::Marines);
            selection.level = Difficulty::Expert;
        }
        app.world_mut().write_message(StartGameRequested);
        app.update();

        let session = app
            .world()
            .get_resource::<Session>()
            .expect("the session is created");
        assert_eq!(session.ai_level, Difficulty::Expert);
        assert_eq!(session.human, PlayerId::Player1);
        assert_eq!(
            session.you().captain.def_id,
            DeckKey::RedHair.captain_id(),
            "your deck must be dealt to the human"
        );
        assert_eq!(session.foe().captain.def_id, DeckKey::Marines.captain_id());

        // The state change only lands on the next frame's transition.
        app.update();
        assert_eq!(screen(&app), AppScreen::Board);
        assert!(
            count::<DeckCardButton>(&mut app) == 0,
            "the setup screen is despawned on exit"
        );
    }

    #[test]
    fn a_winner_moves_the_board_to_the_game_over_screen() {
        let mut app = app();
        app.update();
        {
            let mut selection = app.world_mut().resource_mut::<SetupSelection>();
            selection.pick(PickSide::You, DeckKey::Mugiwara);
            selection.pick(PickSide::Foe, DeckKey::Marines);
        }
        app.world_mut().write_message(StartGameRequested);
        app.update();
        app.update();
        assert_eq!(screen(&app), AppScreen::Board);

        // The engine declares the human the winner inside `apply`; the client
        // only notices it.
        app.world_mut().resource_mut::<Session>().state.winner = Some(PlayerId::Player1);
        app.update();
        app.update();

        assert_eq!(screen(&app), AppScreen::GameOver);
        assert_eq!(count::<ReplayButton>(&mut app), 1);
        assert!(matches!(*app.world().resource::<UiMode>(), UiMode::Idle));
    }

    #[test]
    fn replaying_drops_the_session_and_rebuilds_the_deck_picker() {
        let mut app = app();
        app.update();
        {
            let mut selection = app.world_mut().resource_mut::<SetupSelection>();
            selection.pick(PickSide::You, DeckKey::Mugiwara);
            selection.pick(PickSide::Foe, DeckKey::Baroque);
        }
        app.world_mut().write_message(StartGameRequested);
        app.update();
        app.update();
        app.world_mut().resource_mut::<Session>().state.winner = Some(PlayerId::Player2);
        app.update();
        app.update();
        assert_eq!(screen(&app), AppScreen::GameOver);

        app.world_mut().write_message(ReplayRequested);
        app.update();
        app.update();

        assert_eq!(screen(&app), AppScreen::Setup);
        assert!(app.world().get_resource::<Session>().is_none());
        assert_eq!(count::<ReplayButton>(&mut app), 0);
        assert_eq!(count::<DeckCardButton>(&mut app), DeckKey::ALL.len() * 2);
        let selection = app.world().resource::<SetupSelection>();
        assert!(
            !selection.is_ready(),
            "the picker starts over with nothing picked"
        );
        assert_eq!(selection.level, Difficulty::Intermediate);
        assert!(app.world().resource::<SelectedHandCard>().0.is_none());
    }
}
