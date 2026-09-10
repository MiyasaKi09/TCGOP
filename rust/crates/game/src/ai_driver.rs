//! The opponent's turn loop and its pacing — a port of the `needsAutoAction`
//! effect of `src/hooks/useGameEngine.ts`.
//!
//! **What runs the game.** The *decision* of what to do next is already in the
//! bridge: [`Session::needs_auto_action`](crate::bridge::Session::needs_auto_action)
//! returns the same four cases as the TS memo. This module owns the *when*:
//!
//! 1. every frame in [`AppScreen::Board`](crate::app::AppScreen), read
//!    [`AutoAction`](crate::bridge::AutoAction);
//! 2. `AutoAction::None` → disarm and wait for the human;
//! 3. otherwise arm a `Timer` of [`wait_for`]`(base_delay(..), busy_until, now)`
//!    — [`AI_TURN_DELAY`] for a turn, and for a counter window
//!    [`SPECIAL_ATTACK_PAUSE`] when the pending attack is special or comes from
//!    a captain, else [`BASE_ATTACK_PAUSE`] ([`COUNTER_DELAY`] when there is no
//!    pending attack at all);
//! 4. when it fires, pick the action —
//!    `AutoPass` → [`GameAction::PassCounter`];
//!    `AiDefend` / `AiTurn` → [`ai_choose_action`], falling back to
//!    `PassCounter` / `EndTurn` on `Err` — and write
//!    [`DispatchAction`](crate::bridge::DispatchAction).
//!
//! **`busy_until` is the TS `busyUntilRef`.** It stalls the loop until the
//! reveal currently on screen has finished, so two announcements never overlap.
//! The TS extends it inside `announce()`, which runs for *both* sides; here the
//! reveals are built by [`vfx`](crate::vfx) (it keeps the pre-action state the
//! announcement needs), so the driver simply absorbs every
//! [`AnnouncePlay`](crate::vfx::AnnouncePlay) and pushes `busy_until` by its
//! duration. Same arithmetic, one source of truth.
//!
//! **Re-arming.** The TS effect re-runs — and therefore cancels its
//! `setTimeout` — whenever `needsAutoAction` or `stateVersion` changes.
//! [`AiPacing::state_version`] is that `stateVersion`: it is bumped on every
//! [`StateChanged`](crate::bridge::StateChanged), and an armed timer whose
//! kind or version is stale is thrown away and replaced.
//!
//! **Dead ends** (decision §8.21). A refused action does *not* change the
//! state, so nothing invalidates the version and the TS loop would re-arm on
//! the very same choice for ever. [`AiPacing::refused`] remembers every action
//! the engine turned down for the current `state_version` and the loop never
//! offers one of them again for that version: the first refusal falls back to
//! `passCounter` / `endTurn`, and if *that* is refused too the loop
//! [`stall`](AiPacing::stall)s — it disarms, stops dispatching, and raises one
//! [`EngineErrorEvent`] instead of livelocking. A [`StateChanged`] clears the
//! memory and gives the loop another go.
//!
//! Everything here ticks off `Res<Time>` and talks to the world only through
//! messages, so a whole AI-vs-AI game runs inside an `App` built with
//! `MinimalPlugins` and `TimeUpdateStrategy::ManualDuration` (see the tests).

use core::time::Duration;

use bevy::prelude::*;
use tcgop_engine::ai::ai_choose_action;
use tcgop_engine::error::EngineError;
use tcgop_engine::state::PendingAttack;
use tcgop_engine::types::GameAction;

use crate::app::{AppScreen, AppSet, configure_pipeline};
use crate::bridge::{
    AutoAction, BridgeSet, DispatchAction, EngineErrorEvent, Session, StateChanged,
};
use crate::selection::is_captain_key;
use crate::vfx::{AnnouncePlay, VfxSet};

// ============================================================
// Pacing constants (useGameEngine.ts)
// ============================================================

/// Delay before the AI takes its next turn action.
pub const AI_TURN_DELAY: Duration = Duration::from_millis(650);
/// Delay before an automatic counter-window answer when no attack is pending.
pub const COUNTER_DELAY: Duration = Duration::from_millis(700);
/// Dramatic pause on a pending **base** attack before it resolves.
pub const BASE_ATTACK_PAUSE: Duration = Duration::from_millis(750);
/// Dramatic pause on a pending **special / captain** attack (it plays a cut-in).
pub const SPECIAL_ATTACK_PAUSE: Duration = Duration::from_millis(1000);

// --- announcement durations (announce.ts `announceDuration`) ---

/// Special / fruit-special attack cut-in.
pub const REVEAL_SPECIAL: Duration = Duration::from_millis(1700);
/// Captain attack cut-in.
pub const REVEAL_CAPTAIN: Duration = Duration::from_millis(1600);
/// Full card reveal.
pub const REVEAL_BIG: Duration = Duration::from_millis(1150);
/// Compact card reveal.
pub const REVEAL_COMPACT: Duration = Duration::from_millis(700);
/// Text-only toast.
pub const REVEAL_TOAST: Duration = Duration::from_millis(750);
/// End-of-turn toast.
pub const REVEAL_END_TURN: Duration = Duration::from_millis(550);

// ============================================================
// Pure pacing arithmetic
// ============================================================

/// TS `attackPause` / `baseDelay`: how long the loop waits *at least* before
/// answering, ignoring any reveal still on screen.
///
/// - a turn is paced at [`AI_TURN_DELAY`];
/// - a counter window holds on the pending attack — [`SPECIAL_ATTACK_PAUSE`]
///   for a special or a captain attack (they play a cut-in),
///   [`BASE_ATTACK_PAUSE`] otherwise;
/// - with no pending attack the TS falls back to [`COUNTER_DELAY`].
pub fn base_delay(auto: AutoAction, pending: Option<&PendingAttack>) -> Duration {
    match auto {
        AutoAction::None => Duration::ZERO,
        AutoAction::AiTurn => AI_TURN_DELAY,
        AutoAction::AiDefend | AutoAction::AutoPass => match pending {
            Some(attack) if attack.is_special || is_captain_key(&attack.attacker_id) => {
                SPECIAL_ATTACK_PAUSE
            }
            Some(_) => BASE_ATTACK_PAUSE,
            None => COUNTER_DELAY,
        },
    }
}

/// TS `Math.max(baseDelay, busyUntilRef.current - Date.now())` — never cut a
/// reveal short, never answer faster than the dramatic pause.
pub fn wait_for(base: Duration, busy_until: Duration, now: Duration) -> Duration {
    base.max(busy_until.saturating_sub(now))
}

// ============================================================
// Resources
// ============================================================

/// One armed auto-action: what it was armed for, at which state version, and
/// how much of its delay is left.
#[derive(Debug, Clone)]
pub struct ArmedAuto {
    /// The case this timer answers. A different case cancels it.
    pub kind: AutoAction,
    /// [`AiPacing::state_version`] when it was armed. A newer state cancels it.
    pub version: u64,
    /// Counts down the [`wait_for`] delay.
    pub timer: Timer,
}

/// The AI loop's clock — the Bevy shape of the TS `busyUntilRef` +
/// `setTimeout` + `stateVersion` triple.
#[derive(Resource, Debug, Default)]
pub struct AiPacing {
    /// Elapsed time (`Time::elapsed`) until which a reveal is still on screen.
    pub busy_until: Duration,
    /// Bumped on every [`StateChanged`]; a stale armed timer is re-armed.
    pub state_version: u64,
    /// The pending auto-action, if the loop is currently counting down.
    pub armed: Option<ArmedAuto>,
    /// Actions the engine refused at the current [`AiPacing::state_version`].
    /// The loop never offers one of them again until the state moves on.
    pub refused: Vec<GameAction>,
    /// Set when even the fallback (`passCounter` / `endTurn`) was refused: the
    /// loop stays disarmed instead of re-arming on every frame.
    pub stalled: bool,
    /// The refused fallback, waiting to be reported *once* as an
    /// [`EngineErrorEvent`].
    pub stall_report: Option<GameAction>,
}

impl AiPacing {
    /// TS `busyUntilRef.current = Math.max(busyUntilRef.current, Date.now()) + d`.
    pub fn extend(&mut self, now: Duration, hold: Duration) {
        if hold.is_zero() {
            return;
        }
        self.busy_until = self.busy_until.max(now) + hold;
    }

    /// How long a reveal will still hold the loop.
    // Read by the pacing tests; the loop itself only compares `busy_until`.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn remaining_busy(&self, now: Duration) -> Duration {
        self.busy_until.saturating_sub(now)
    }

    /// `true` while an auto-action is counting down.
    // Read by the pacing tests; the loop matches on `armed` directly.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn is_armed(&self) -> bool {
        self.armed.is_some()
    }

    /// Cancel the armed auto-action (TS `clearTimeout`).
    pub fn disarm(&mut self) {
        self.armed = None;
    }

    /// Remember that the engine refused `action` for the current state.
    pub fn note_refused(&mut self, action: &GameAction) {
        if !self.is_refused(action) {
            self.refused.push(action.clone());
        }
    }

    /// Has the engine already refused `action` for the current state?
    pub fn is_refused(&self, action: &GameAction) -> bool {
        self.refused.iter().any(|refused| refused == action)
    }

    /// Give up on the current state: disarm, stop re-arming, and queue the one
    /// error the loop is allowed to raise about it.
    pub fn stall(&mut self, fallback: GameAction) {
        self.disarm();
        if !self.stalled {
            self.stalled = true;
            self.stall_report = Some(fallback);
        }
    }

    /// A new state un-refuses everything and un-stalls the loop.
    pub fn forget_refusals(&mut self) {
        self.refused.clear();
        self.stalled = false;
        self.stall_report = None;
    }
}

// ============================================================
// Plugin
// ============================================================

/// The AI loop's systems, so other features can order against them.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AiDriverSet;

/// AI turn loop, counter-window answers and the reveal-aware pacing.
pub struct AiDriverPlugin;

impl Plugin for AiDriverPlugin {
    fn build(&self, app: &mut App) {
        configure_pipeline(app);
        app.init_resource::<AiPacing>()
            // Safety net: the driver reads vfx's reveal stream, and must not
            // panic in a head-less app that skipped `VfxPlugin`. `add_message`
            // is idempotent, so this is a no-op in the real app.
            .add_message::<AnnouncePlay>()
            .add_systems(OnEnter(AppScreen::Board), reset_pacing)
            .add_systems(OnExit(AppScreen::Board), reset_pacing)
            // Decide *before* the bridge so the action lands in the same frame.
            .add_systems(
                Update,
                drive_auto_actions
                    .in_set(AiDriverSet)
                    .in_set(AppSet::Input)
                    .before(BridgeSet)
                    .run_if(resource_exists::<Session>)
                    .run_if(in_state(AppScreen::Board)),
            )
            // Bookkeeping *after* the bridge and after vfx queued its reveals.
            .add_systems(
                Update,
                (
                    bump_state_version,
                    absorb_reveal_pacing,
                    recover_from_refused_actions,
                    watch_for_winner,
                )
                    .chain()
                    .in_set(AiDriverSet)
                    .in_set(AppSet::Vfx)
                    .after(BridgeSet)
                    .after(VfxSet)
                    .run_if(resource_exists::<Session>)
                    .run_if(in_state(AppScreen::Board)),
            );
    }
}

/// A new game (or leaving the board) forgets every pending delay.
fn reset_pacing(mut pacing: ResMut<AiPacing>) {
    *pacing = AiPacing::default();
}

/// TS `stateVersion` — every applied action invalidates an armed timer.
fn bump_state_version(mut changed: MessageReader<StateChanged>, mut pacing: ResMut<AiPacing>) {
    if changed.read().count() > 0 {
        pacing.state_version = pacing.state_version.wrapping_add(1);
        // Decision §8.21: the refusals belonged to the *old* state.
        pacing.forget_refusals();
    }
}

/// TS `announce()`'s side effect: a reveal pushes the busy window forward, so
/// the next auto-action waits for it to finish.
fn absorb_reveal_pacing(
    time: Res<Time>,
    mut reveals: MessageReader<AnnouncePlay>,
    mut pacing: ResMut<AiPacing>,
) {
    let now = time.elapsed();
    for reveal in reveals.read() {
        pacing.extend(now, reveal.0.duration());
    }
}

/// The loop itself.
fn drive_auto_actions(
    time: Res<Time>,
    mut session: ResMut<Session>,
    mut pacing: ResMut<AiPacing>,
    mut dispatch: MessageWriter<DispatchAction>,
    mut failed: MessageWriter<EngineErrorEvent>,
) {
    // Decision §8.21: a stalled loop says so once, then stays quiet until a
    // [`StateChanged`] clears it — no timer, no dispatch, no second error.
    if let Some(action) = pacing.stall_report.take() {
        failed.write(EngineErrorEvent {
            error: EngineError::illegal(format!(
                "AI loop stalled: {} is the only answer left and the engine refuses it",
                action.type_name()
            )),
            action,
        });
    }
    if pacing.stalled {
        pacing.disarm();
        return;
    }

    let auto = session.needs_auto_action();
    if auto == AutoAction::None {
        pacing.disarm();
        return;
    }

    let now = time.elapsed();
    let version = pacing.state_version;
    let stale = match pacing.armed.as_ref() {
        Some(armed) => armed.kind != auto || armed.version != version,
        None => true,
    };
    if stale {
        let base = base_delay(auto, session.state.pending_attack.as_ref());
        let wait = wait_for(base, pacing.busy_until, now);
        pacing.armed = Some(ArmedAuto {
            kind: auto,
            version,
            timer: Timer::new(wait, TimerMode::Once),
        });
    }

    let delta = time.delta();
    let Some(armed) = pacing.armed.as_mut() else {
        return;
    };
    armed.timer.tick(delta);
    if !armed.timer.is_finished() {
        return;
    }
    pacing.disarm();

    let Some(action) = choose_auto_action(&mut session, auto) else {
        return;
    };
    // Decision §8.21: never re-offer what this state already refused.
    let action = if pacing.is_refused(&action) {
        let fallback = fallback_action(&session);
        if pacing.is_refused(&fallback) {
            pacing.stall(fallback);
            return;
        }
        fallback
    } else {
        action
    };
    dispatch.write(DispatchAction(action));
}

/// The answer that is always legal for whoever is on the clock — TS
/// `updateState`'s `catch` fallback.
fn fallback_action(session: &Session) -> GameAction {
    if session.in_counter_window() {
        GameAction::PassCounter
    } else {
        GameAction::EndTurn
    }
}

/// TS timer body: re-check the *current* state, then choose.
///
/// The guards are the TS `return`s — between arming the timer and firing it the
/// state may have moved on (the human countered, the game ended), in which case
/// the loop does nothing and the next frame re-arms from scratch.
fn choose_auto_action(session: &mut Session, auto: AutoAction) -> Option<GameAction> {
    if session.winner().is_some() {
        return None;
    }
    let ai = session.ai_player();
    let level = session.ai_level;
    match auto {
        AutoAction::None => None,
        AutoAction::AutoPass => session
            .state
            .pending_attack
            .as_ref()
            .map(|_| GameAction::PassCounter),
        AutoAction::AiDefend => {
            session.state.pending_attack.as_ref()?;
            Some(ai_pick(session, level).unwrap_or(GameAction::PassCounter))
        }
        AutoAction::AiTurn => {
            if session.state.current_player != ai || session.state.pending_attack.is_some() {
                return None;
            }
            Some(ai_pick(session, level).unwrap_or(GameAction::EndTurn))
        }
    }
}

/// `aiChooseAction(cur, aiPlayer, difficulty)`, on the session's own random
/// stream so a seed replays the whole game.
fn ai_pick(session: &mut Session, level: tcgop_engine::ai::Difficulty) -> Option<GameAction> {
    let ai = session.ai_player();
    let session = &mut *session;
    ai_choose_action(
        &session.state,
        &session.registry,
        &mut session.ctx,
        ai,
        level,
    )
    .ok()
}

/// TS `updateState`'s inner `catch`: if the engine refuses the action the loop
/// chose, fall back to `passCounter` / `endTurn` so the game cannot deadlock.
///
/// Only the auto-driven cases are recovered — a refused *human* action is a UI
/// bug, reported by the bridge and left alone here.
///
/// Decision §8.21: the refusal is also written down for the current
/// `state_version` (a refused action leaves the state untouched, so nothing
/// else would stop the loop from choosing it again), and if the fallback is
/// itself among the refusals the loop stalls rather than re-arming for ever.
fn recover_from_refused_actions(
    mut failures: MessageReader<EngineErrorEvent>,
    session: Res<Session>,
    mut pacing: ResMut<AiPacing>,
    mut dispatch: MessageWriter<DispatchAction>,
) {
    // Always drain the reader, even when there is nothing to recover — a
    // message left behind would be re-read on a later, unrelated frame.
    let refused: Vec<GameAction> = failures
        .read()
        .map(|failure| failure.action.clone())
        .collect();
    if refused.is_empty() || pacing.stalled || session.winner().is_some() {
        return;
    }
    if session.needs_auto_action() == AutoAction::None {
        return;
    }
    for action in &refused {
        pacing.note_refused(action);
    }
    let fallback = fallback_action(&session);
    // The fallback itself failing must not re-queue the fallback for ever.
    if pacing.is_refused(&fallback) {
        pacing.stall(fallback);
        return;
    }
    dispatch.write(DispatchAction(fallback));
}

/// The engine sets `winner` inside `apply`; as soon as it is `Some` the board
/// gives way to the victory / defeat splash.
///
/// `set_if_neq` keeps this idempotent — [`screens`](crate::screens) watches the
/// same flag and both may fire on the same frame.
fn watch_for_winner(session: Res<Session>, mut next: ResMut<NextState<AppScreen>>) {
    if session.winner().is_some() {
        // Fully qualified: `DetectChangesMut::set_if_neq` (which compares the
        // whole `NextState`) shadows `NextState`'s own inherent method.
        NextState::set_if_neq(&mut next, AppScreen::GameOver);
    }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::{ActionApplied, BridgePlugin, testkit};
    use crate::vfx::{PlayAnnouncement, Side};
    use bevy::state::app::StatesPlugin;
    use bevy::time::TimeUpdateStrategy;
    use tcgop_engine::types::PlayerId;

    /// One simulated frame.
    const FRAME: Duration = Duration::from_millis(100);

    fn pending(attacker: &str, is_special: bool) -> PendingAttack {
        PendingAttack {
            attacker_id: attacker.to_string(),
            target_id: "t".to_string(),
            target_is_captain: false,
            is_special,
            raw_damage: 3,
            attack_power: None,
            element: None,
            attack_traits: Vec::new(),
            has_haki: false,
            ignore_shield: None,
            cannot_be_dodged: None,
            immobilize: None,
            sleep: None,
            pushback: None,
            pushback_slots: None,
            strip_stealth: None,
            survive_played: None,
            survive_target_id: None,
            damage_reduction: None,
            ignore_def: None,
            permanent_pv_loss: None,
            no_heal: None,
        }
    }

    fn toast(kind: &'static str) -> PlayAnnouncement {
        PlayAnnouncement {
            side: Side::Foe,
            def_id: None,
            instance_id: None,
            kind,
            dest_id: None,
            caption: String::new(),
            big: false,
            toast: true,
        }
    }

    /// A head-less board: bridge + driver, no window, no vfx, time stepped by
    /// hand one [`FRAME`] per `app.update()`.
    fn driver_app(seed: u64) -> App {
        let mut app = App::new();
        app.add_plugins(bevy::MinimalPlugins)
            .add_plugins(StatesPlugin)
            .insert_resource(TimeUpdateStrategy::ManualDuration(FRAME))
            .init_state::<AppScreen>()
            .add_plugins(BridgePlugin)
            .add_plugins(AiDriverPlugin)
            .insert_resource(testkit::session(seed));
        app.world_mut()
            .resource_mut::<NextState<AppScreen>>()
            .set(AppScreen::Board);
        app.update();
        app
    }

    fn applied(app: &App) -> Vec<GameAction> {
        app.world()
            .resource::<Messages<ActionApplied>>()
            .iter_current_update_messages()
            .map(|message| message.0.clone())
            .collect()
    }

    /// Pump frames until `stop` holds, returning how many frames it took.
    fn pump(app: &mut App, max_frames: usize, stop: impl Fn(&App) -> bool) -> Option<usize> {
        for frame in 0..max_frames {
            app.update();
            if stop(app) {
                return Some(frame + 1);
            }
        }
        None
    }

    // --- pure arithmetic ---

    #[test]
    fn base_delay_mirrors_the_web_table() {
        assert_eq!(base_delay(AutoAction::None, None), Duration::ZERO);
        assert_eq!(base_delay(AutoAction::AiTurn, None), AI_TURN_DELAY);
        // A turn keeps its own delay even if the caller passes an attack.
        assert_eq!(
            base_delay(AutoAction::AiTurn, Some(&pending("c1", true))),
            AI_TURN_DELAY
        );
        // No pending attack → the TS `|| 700` fallback.
        assert_eq!(base_delay(AutoAction::AiDefend, None), COUNTER_DELAY);
        assert_eq!(
            base_delay(AutoAction::AiDefend, Some(&pending("c1", false))),
            BASE_ATTACK_PAUSE
        );
        assert_eq!(
            base_delay(AutoAction::AutoPass, Some(&pending("c1", true))),
            SPECIAL_ATTACK_PAUSE
        );
        // A captain attack is cinematic even when it is not "special".
        assert_eq!(
            base_delay(
                AutoAction::AutoPass,
                Some(&pending(
                    &crate::selection::captain_key(PlayerId::Player2),
                    false
                ))
            ),
            SPECIAL_ATTACK_PAUSE
        );
    }

    #[test]
    fn a_running_reveal_stretches_the_wait_but_never_shortens_it() {
        let now = Duration::from_secs(10);
        // Nothing on screen → the dramatic pause wins.
        assert_eq!(wait_for(AI_TURN_DELAY, Duration::ZERO, now), AI_TURN_DELAY);
        // A short reveal is shorter than the pause → the pause still wins.
        assert_eq!(
            wait_for(AI_TURN_DELAY, now + Duration::from_millis(200), now),
            AI_TURN_DELAY
        );
        // A cut-in outlasts it → wait for the cut-in.
        assert_eq!(
            wait_for(AI_TURN_DELAY, now + REVEAL_SPECIAL, now),
            REVEAL_SPECIAL
        );
    }

    #[test]
    fn reveals_queue_up_instead_of_overlapping() {
        let mut pacing = AiPacing::default();
        let now = Duration::from_secs(4);
        pacing.extend(now, REVEAL_BIG);
        assert_eq!(pacing.remaining_busy(now), REVEAL_BIG);
        // A second reveal starts where the first ends.
        pacing.extend(now, REVEAL_TOAST);
        assert_eq!(pacing.remaining_busy(now), REVEAL_BIG + REVEAL_TOAST);
        // Once the window has elapsed it restarts from "now", not from the past.
        let later = now + Duration::from_secs(30);
        pacing.extend(later, REVEAL_COMPACT);
        assert_eq!(pacing.remaining_busy(later), REVEAL_COMPACT);
        // A zero-length reveal changes nothing.
        let before = pacing.busy_until;
        pacing.extend(later, Duration::ZERO);
        assert_eq!(pacing.busy_until, before);
    }

    // --- the loop, head-lessly ---

    /// The "human" seat, played by a second AI so a whole game can run with no
    /// window and no clicks.
    ///
    /// `stuck` means the engine refused our previous pick: `valid_actions_for`
    /// can offer a `playCounter` that `apply` then rejects (e.g. a *Survive*
    /// counter aimed at the opponent — "Survive only protects allies"). A real
    /// player would simply click something else; here we fall back to the answer
    /// that is always legal, otherwise the game would never move on.
    fn human_choice(app: &mut App, stuck: bool) -> Option<GameAction> {
        let mut session = app.world_mut().resource_mut::<Session>();
        if session.valid.is_empty() {
            return None;
        }
        if stuck {
            return Some(if session.in_counter_window() {
                GameAction::PassCounter
            } else {
                GameAction::EndTurn
            });
        }
        let session = &mut *session;
        let human = session.human;
        let level = session.ai_level;
        ai_choose_action(
            &session.state,
            &session.registry,
            &mut session.ctx,
            human,
            level,
        )
        .ok()
    }

    /// What a whole game looked like from the outside.
    #[derive(Debug, Default)]
    struct PlayOut {
        /// Actions the test dispatched on the human's behalf.
        human: usize,
        /// Action kinds the *driver* applied — everything the test never asked
        /// for: the AI's turn, its counter answers and the automatic passes.
        driver: Vec<&'static str>,
        frames: usize,
    }

    impl PlayOut {
        fn driver_played(&self, kind: &str) -> bool {
            self.driver.contains(&kind)
        }
    }

    /// Run the game to its end (or `max_frames`), one [`FRAME`] per update.
    fn play_out(app: &mut App, max_frames: usize) -> PlayOut {
        let mut out = PlayOut::default();
        let mut stuck = false;
        for frame in 0..max_frames {
            out.frames = frame + 1;
            if app.world().resource::<Session>().winner().is_some() {
                break;
            }
            let written = human_choice(app, stuck);
            if let Some(action) = written.clone() {
                app.world_mut().write_message(DispatchAction(action));
                out.human += 1;
            }
            app.update();
            let mut ours = written;
            for action in applied(app) {
                if ours.as_ref() == Some(&action) {
                    ours = None;
                } else {
                    out.driver.push(action.type_name());
                }
            }
            stuck = app
                .world()
                .resource::<Messages<EngineErrorEvent>>()
                .iter_current_update_messages()
                .count()
                > 0;
        }
        out
    }

    #[test]
    fn the_driver_is_idle_on_the_humans_turn() {
        let mut app = driver_app(42);
        for _ in 0..20 {
            app.update();
            assert!(applied(&app).is_empty(), "the AI must not play for us");
        }
        assert!(!app.world().resource::<AiPacing>().is_armed());
        assert_eq!(
            app.world().resource::<Session>().state.current_player,
            PlayerId::Player1
        );
    }

    #[test]
    fn the_ai_turn_waits_its_dramatic_pause_before_acting() {
        let mut app = driver_app(42);
        app.world_mut()
            .write_message(DispatchAction(GameAction::EndTurn));
        app.update();
        assert_eq!(applied(&app), vec![GameAction::EndTurn]);
        assert_eq!(
            app.world().resource::<Session>().needs_auto_action(),
            AutoAction::AiTurn
        );

        // 650 ms at 100 ms a frame: nothing before the 6th frame.
        let frames =
            pump(&mut app, 40, |app| !applied(app).is_empty()).expect("the AI eventually plays");
        assert!(
            (6..=8).contains(&frames),
            "the AI waited {frames} frames (~{} ms)",
            frames * 100
        );
        assert!(app.world().resource::<AiPacing>().state_version >= 1);
    }

    #[test]
    fn a_long_reveal_stalls_the_loop_past_the_base_delay() {
        let mut app = driver_app(42);
        app.world_mut()
            .write_message(DispatchAction(GameAction::EndTurn));
        app.update();

        // Pretend a 1.7 s cut-in is on screen (what vfx would report).
        let now = app.world().resource::<Time>().elapsed();
        app.world_mut()
            .resource_mut::<AiPacing>()
            .extend(now, REVEAL_SPECIAL);

        let frames =
            pump(&mut app, 60, |app| !applied(app).is_empty()).expect("the AI eventually plays");
        // 1700 ms of reveal, minus the frame that consumed part of it, is still
        // far beyond the 650 ms base delay (7 frames).
        assert!(
            frames >= 15,
            "the AI cut the 1700 ms reveal short after {frames} frames"
        );
    }

    #[test]
    fn a_reveal_message_extends_the_busy_window() {
        let mut app = driver_app(7);
        app.world_mut()
            .write_message(AnnouncePlay(toast("endTurn")));
        app.update();
        let pacing = app.world().resource::<AiPacing>();
        let now = app.world().resource::<Time>().elapsed();
        assert_eq!(pacing.remaining_busy(now), REVEAL_END_TURN);
    }

    #[test]
    fn leaving_the_board_forgets_every_delay() {
        let mut app = driver_app(42);
        app.world_mut()
            .write_message(DispatchAction(GameAction::EndTurn));
        app.update();
        app.update();
        assert!(app.world().resource::<AiPacing>().is_armed());

        app.world_mut()
            .resource_mut::<NextState<AppScreen>>()
            .set(AppScreen::Setup);
        app.update();
        let pacing = app.world().resource::<AiPacing>();
        assert!(!pacing.is_armed());
        assert_eq!(pacing.busy_until, Duration::ZERO);
        assert_eq!(pacing.state_version, 0);
    }

    #[test]
    fn a_whole_ai_vs_ai_game_terminates_and_reaches_game_over() {
        let mut app = driver_app(11);
        let out = play_out(&mut app, 20_000);

        let session = app.world().resource::<Session>();
        assert!(
            session.winner().is_some(),
            "the game must end — stopped on turn {} after {} frames ({out:?})",
            session.state.turn_number,
            out.frames
        );
        assert!(out.human > 5, "the human seat really played ({out:?})");
        assert!(!out.driver.is_empty(), "so did the driver ({out:?})");

        // `StateTransition` runs before `Update`, so the screen swap lands on
        // the frame after the winning blow.
        app.update();
        assert_eq!(
            *app.world().resource::<State<AppScreen>>().get(),
            AppScreen::GameOver,
            "a winner switches the screen"
        );
    }

    #[test]
    fn the_driver_plays_the_ai_turn_and_closes_the_counter_windows() {
        let mut app = driver_app(11);
        let out = play_out(&mut app, 20_000);

        // The AI's own turn: it ends it, and it attacks.
        assert!(out.driver_played("endTurn"), "{out:?}");
        assert!(
            out.driver_played("baseAttack")
                || out.driver_played("specialAttack")
                || out.driver_played("captainAttack"),
            "{out:?}"
        );
        // And it answered counter windows on both sides: `AutoPass` when the
        // human had nothing but `passCounter`, `AiDefend` when we attacked.
        assert!(
            out.driver_played("passCounter"),
            "no counter window was closed automatically ({out:?})"
        );
    }

    #[test]
    fn the_loop_stops_once_the_game_is_over() {
        let mut app = driver_app(11);
        play_out(&mut app, 20_000);
        assert!(app.world().resource::<Session>().winner().is_some());

        // The board is gone, so the loop is gone too: no further action ever
        // reaches the engine.
        let turn = app.world().resource::<Session>().state.turn_number;
        for _ in 0..30 {
            app.update();
            assert!(applied(&app).is_empty());
        }
        assert_eq!(app.world().resource::<Session>().state.turn_number, turn);
        assert!(!app.world().resource::<AiPacing>().is_armed());
    }

    // --- decision §8.21: dead ends ---

    /// Corrupt the session into a dead end: a counter window whose attack aims
    /// at an instance that does not exist, with an empty human hand so
    /// `passCounter` is the only answer left — and the engine refuses it every
    /// single time (`Instance not found`).
    fn dead_end(app: &mut App) {
        let mut session = app.world_mut().resource_mut::<Session>();
        let human = session.human;
        let ai = session.ai_player();
        session.state.player_mut(human).hand.clear();
        session.state.current_player = ai;
        session.state.pending_attack = Some(pending("ghost", false));
        session.refresh_valid();
    }

    fn errors_this_frame(app: &App) -> usize {
        app.world()
            .resource::<Messages<EngineErrorEvent>>()
            .iter_current_update_messages()
            .count()
    }

    #[test]
    fn a_refused_fallback_stalls_the_loop_after_a_single_error() {
        let mut app = driver_app(42);
        dead_end(&mut app);
        assert_eq!(
            app.world().resource::<Session>().needs_auto_action(),
            AutoAction::AutoPass,
            "the dead end must still ask the driver for an answer"
        );

        let mut errors = Vec::new();
        for _ in 0..80 {
            app.update();
            errors.push(errors_this_frame(&app));
            assert!(
                applied(&app).is_empty(),
                "nothing can be applied in a dead end"
            );
        }

        // Before decision §8.21 the loop re-armed on every frame and replayed
        // the refused `passCounter` roughly every 750 ms, for ever.
        let total: usize = errors.iter().sum();
        assert!(
            total <= 2,
            "the loop kept retrying: {total} refusals in 80 frames ({errors:?})"
        );
        assert!(
            errors[20..].iter().all(|count| *count == 0),
            "the loop was still dispatching after it gave up ({errors:?})"
        );
        let pacing = app.world().resource::<AiPacing>();
        assert!(pacing.stalled, "the loop must record that it gave up");
        assert!(!pacing.is_armed(), "and stay disarmed");
    }

    #[test]
    fn a_new_state_forgets_what_the_old_one_refused() {
        let mut app = driver_app(42);
        {
            let mut pacing = app.world_mut().resource_mut::<AiPacing>();
            pacing.note_refused(&GameAction::PassCounter);
            pacing.note_refused(&GameAction::PassCounter);
            assert_eq!(pacing.refused.len(), 1, "a refusal is remembered once");
            assert!(pacing.is_refused(&GameAction::PassCounter));
            pacing.stall(GameAction::PassCounter);
        }

        app.world_mut().write_message(StateChanged);
        app.update();

        let pacing = app.world().resource::<AiPacing>();
        assert!(
            !pacing.is_refused(&GameAction::PassCounter),
            "a new state re-opens every action"
        );
        assert!(!pacing.stalled, "and gives the loop another go");
    }
}
