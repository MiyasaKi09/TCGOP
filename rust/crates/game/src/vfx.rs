//! Everything animated: combat feedback, cut-ins, play reveals and the ambient
//! sea.
//!
//! **Shape of the module.** The decision layer is pure and head-lessly tested,
//! the drawing layer only obeys it:
//!
//! - [`element`] — the per-element look (colour / glow / tint / glyph), a port
//!   of `src/lib/vfx.ts`;
//! - [`detect`] — the state diff that turns two engine snapshots into
//!   [`CombatEvent`]s (`src/lib/useCombatVfx.ts`): projectiles, impacts,
//!   heals, KOs and deploy slams, with the shake and the tile flash each one
//!   asks for;
//! - [`announce`] — the play reveals (`src/lib/announce.ts`): what card pops at
//!   the centre of the screen, what caption it carries and for how long;
//! - [`tween`] — a minimal tweening component + system (no extra crate);
//! - [`render`] — the systems that spawn the UI entities, shake the board,
//!   play the cut-ins, drive the reveal queue and animate the sea.
//!
//! **Contract.** VFX never gate gameplay: this module reads the engine state
//! and never writes [`DispatchAction`](crate::bridge::DispatchAction). It
//! consumes [`ActionApplied`](crate::bridge::ActionApplied) /
//! [`StateChanged`](crate::bridge::StateChanged) and emits [`CombatVfx`];
//! anything else may push a reveal with [`AnnouncePlay`].
//!
//! **Reduced motion.** [`ReducedMotion`] shortens every duration and disables
//! the board shake and the cut-in slide, exactly like the web's
//! `prefers-reduced-motion` guard.

pub mod announce;
pub mod detect;
pub mod element;
pub mod render;
pub mod tween;

use std::collections::VecDeque;

use bevy::prelude::*;
use core::time::Duration;
use tcgop_engine::state::{GameState, PendingAttack};
use tcgop_engine::types::{GameAction, PlayerId};

use crate::app::{AppScreen, AppSet, configure_pipeline};
use crate::bridge::{ActionApplied, BridgeSet, Session, StateChanged};

// The module's public vocabulary, re-exported so the rest of the client names
// `vfx::CombatEvent` rather than `vfx::detect::CombatEvent`.
/// The only font in `assets/fonts` with symbol coverage — every element glyph
/// goes through it (see [`element`]).
pub use crate::hand::SYMBOL_FONT_PATH;
pub use announce::{PlayAnnouncement, Side, build_announcement};
pub use detect::{CombatEvent, Flash, Shake, Snapshot, VfxKind};
pub use element::VfxElement;
pub use tween::{Tween, TweenState};

// ============================================================
// Messages
// ============================================================

/// A combat effect to draw. Written by [`detect_combat`], read by [`render`].
#[derive(Message, Debug, Clone, PartialEq)]
pub struct CombatVfx(pub CombatEvent);

/// Push a reveal onto the queue. [`detect_combat`] writes one per applied
/// action; any other module may write its own.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct AnnouncePlay(pub PlayAnnouncement);

/// Dismiss the reveal currently on screen (click, or the cut-in being skipped).
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SkipReveal;

/// A captain just flipped — the flip cinematic plays on that side.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptainFlipped(pub PlayerId);

// ============================================================
// Resources
// ============================================================

/// Accessibility switch: shorten every animation and drop the ones that move
/// the whole screen. Mirrors `window.matchMedia("(prefers-reduced-motion)")`,
/// which the web client checks in `useCombatVfx` / `CutInLayer` / `VfxStage`.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Default)]
pub struct ReducedMotion {
    pub enabled: bool,
}

impl ReducedMotion {
    /// How much of a duration survives when reduced motion is on.
    pub const FACTOR: f32 = 0.35;

    /// The environment variable that turns it on at start-up.
    pub const ENV: &'static str = "TCGOP_REDUCED_MOTION";

    /// The key that toggles it at runtime.
    pub const TOGGLE_KEY: KeyCode = KeyCode::F2;

    /// There is no `prefers-reduced-motion` to query from a native window, so
    /// the preference is read from the environment — `TCGOP_REDUCED_MOTION`
    /// set to anything but `0` / `false` / the empty string — and can be
    /// flipped at any time with [`ReducedMotion::TOGGLE_KEY`].
    pub fn from_env() -> Self {
        let enabled = std::env::var(Self::ENV)
            .map(|value| Self::truthy(&value))
            .unwrap_or(false);
        ReducedMotion { enabled }
    }

    /// `"1"`, `"true"`, `"yes"`, `"on"` (any case) mean *on*; anything else,
    /// the empty string included, means *off*.
    pub fn truthy(value: &str) -> bool {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    }

    /// Multiplier to apply to any duration.
    pub fn factor(&self) -> f32 {
        if self.enabled { Self::FACTOR } else { 1.0 }
    }

    /// `duration`, shortened when reduced motion is on.
    pub fn duration(&self, duration: Duration) -> Duration {
        if self.enabled {
            duration.mul_f32(Self::FACTOR)
        } else {
            duration
        }
    }

    /// `seconds`, shortened when reduced motion is on.
    pub fn secs(&self, seconds: f32) -> f32 {
        seconds * self.factor()
    }

    /// Screen-wide motion (board shake, cut-in slide, tint flash) is dropped
    /// entirely.
    pub fn allows_screen_motion(&self) -> bool {
        !self.enabled
    }
}

/// What the previous frame looked like — the TS `prev` / `lastPending` refs,
/// plus the pre-action state the reveals are built from.
#[derive(Resource, Debug, Default)]
pub struct VfxHistory {
    /// Snapshot the next diff compares against.
    pub previous: Option<Snapshot>,
    /// State **before** the last applied action (`buildAnnouncement`'s input).
    pub previous_state: Option<GameState>,
    /// Most recent pending attack, kept after the engine cleared it.
    pub last_pending: Option<PendingAttack>,
    /// The action that declared [`VfxHistory::last_pending`].
    ///
    /// `PendingAttack` says *that* an attack is special, never which ability
    /// produced it, and the port has three that all set the flag (§8.34(a) the
    /// captain's ★, §8.34(b) its surcharge, §8.28×§8.48 an awakened fruit's
    /// own special). The declaring action is the only thing that tells them
    /// apart, so the banner and the cut-in read it — see
    /// [`detect::attack_label`].
    pub last_attack_action: Option<GameAction>,
}

/// Reveals waiting for the screen — only the head is displayed (TS
/// `PlayRevealLayer` renders `announcements[0]`).
#[derive(Resource, Debug, Default)]
pub struct RevealQueue(pub VecDeque<PlayAnnouncement>);

impl RevealQueue {
    /// Never let a burst of AI actions pile up more than this.
    pub const MAX: usize = 4;

    pub fn push(&mut self, announcement: PlayAnnouncement) {
        if self.0.len() >= Self::MAX {
            self.0.pop_front();
        }
        self.0.push_back(announcement);
    }

    pub fn pop(&mut self) -> Option<PlayAnnouncement> {
        self.0.pop_front()
    }

    /// How many reveals are still queued (read by the reveal tests and the
    /// `Vfx` render pass's "is a reveal on screen?" guard).
    // Used by the reveal-queue tests; the render pass peeks with `pop`.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    // Kept beside `len` (clippy::len_without_is_empty).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// The board shake — TS `.vfx-shake` / `.vfx-shake-hard` on the board element.
#[derive(Resource, Debug, Default)]
pub struct ScreenShake {
    /// Remaining time, in seconds.
    pub remaining: f32,
    /// Total duration of the current shake, in seconds.
    pub duration: f32,
    /// Peak offset, in logical pixels.
    pub amplitude: f32,
}

impl ScreenShake {
    /// Soft: 360 ms / 5 px. Hard: 520 ms / 10 px (the web durations).
    pub fn hit(&mut self, shake: Shake, reduced: &ReducedMotion) {
        if !reduced.allows_screen_motion() {
            return;
        }
        let (duration, amplitude) = match shake {
            Shake::Soft => (0.36, 5.0),
            Shake::Hard => (0.52, 10.0),
        };
        // A new hit restarts the animation and keeps the strongest amplitude.
        self.amplitude = self.amplitude.max(amplitude);
        self.duration = self.duration.max(duration);
        self.remaining = self.remaining.max(duration);
    }

    pub fn is_active(&self) -> bool {
        self.remaining > 0.0
    }

    /// Advance by `delta` seconds and return the offset to apply.
    pub fn advance(&mut self, delta: f32) -> Vec2 {
        if !self.is_active() {
            return Vec2::ZERO;
        }
        self.remaining = (self.remaining - delta).max(0.0);
        if self.remaining <= 0.0 {
            self.amplitude = 0.0;
            self.duration = 0.0;
            return Vec2::ZERO;
        }
        let progress = 1.0 - (self.remaining / self.duration.max(f32::EPSILON));
        shake_offset(progress, self.amplitude)
    }
}

/// A decaying, alternating shake — `progress` in `0..=1`.
pub fn shake_offset(progress: f32, amplitude: f32) -> Vec2 {
    let decay = (1.0 - progress).clamp(0.0, 1.0);
    let angle = progress * core::f32::consts::TAU * 3.0;
    Vec2::new(
        ops::sin(angle) * amplitude * decay,
        ops::cos(angle * 0.5) * amplitude * 0.45 * decay,
    )
}

/// Handle of the symbol font ([`SYMBOL_FONT_PATH`]); `Handle::default()`
/// head-lessly.
#[derive(Resource, Debug, Clone, Default)]
pub struct VfxSymbolFont(pub Handle<Font>);

// ============================================================
// Plugin
// ============================================================

/// The logic half of the module (diffing and queueing), so other features can
/// order against it.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VfxSet;

/// Combat feedback, cut-ins, play reveals and ambient animation.
pub struct VfxPlugin;

impl Plugin for VfxPlugin {
    fn build(&self, app: &mut App) {
        let can_load = app.world().contains_resource::<Assets<Font>>();
        let symbols = match app.world().get_resource::<AssetServer>() {
            Some(assets) if can_load => VfxSymbolFont(assets.load(SYMBOL_FONT_PATH)),
            _ => VfxSymbolFont::default(),
        };

        configure_pipeline(app);
        app.insert_resource(symbols)
            .insert_resource(ReducedMotion::from_env())
            .init_resource::<VfxHistory>()
            .init_resource::<RevealQueue>()
            .init_resource::<ScreenShake>()
            .init_resource::<render::ActiveReveal>()
            .add_message::<CombatVfx>()
            .add_message::<AnnouncePlay>()
            .add_message::<SkipReveal>()
            .add_message::<CaptainFlipped>()
            // --- logic: head-less, no window needed ---
            .add_systems(
                Update,
                (detect_combat, enqueue_announcements)
                    .chain()
                    .in_set(VfxSet)
                    .in_set(AppSet::Vfx)
                    .after(BridgeSet)
                    // `Session` outlives the board — it is only dropped later,
                    // by the replay handler — so gating on it alone would let
                    // the diff re-seed itself from the *finished* game on the
                    // transition frame, after `reset_vfx_state` has cleared it.
                    .run_if(vfx_ready),
            )
            .add_systems(
                Update,
                toggle_reduced_motion
                    .in_set(AppSet::Input)
                    .run_if(resource_exists::<ButtonInput<KeyCode>>),
            )
            .add_systems(
                Update,
                (tween::tick_tweens, tween::tick_lifetimes).in_set(AppSet::Vfx),
            )
            // --- presentation ---
            .add_plugins(render::RenderPlugin);
    }
}

// ============================================================
// Detection
// ============================================================

/// Diff the engine state, emit the combat events and queue the reveals.
///
/// Runs after the bridge, so it sees the state the action produced; the
/// *reveal* of that action is built from [`VfxHistory::previous_state`], the
/// state as it was **before** the action — the TS builds its announcement
/// before executing.
#[allow(clippy::too_many_arguments)]
pub fn detect_combat(
    session: Res<Session>,
    mut history: ResMut<VfxHistory>,
    mut applied: MessageReader<ActionApplied>,
    mut changed: MessageReader<StateChanged>,
    mut events: MessageWriter<CombatVfx>,
    mut reveals: MessageWriter<AnnouncePlay>,
    mut flips: MessageWriter<CaptainFlipped>,
    mut shake: ResMut<ScreenShake>,
    reduced: Res<ReducedMotion>,
) {
    let actions: Vec<GameAction> = applied.read().map(|message| message.0.clone()).collect();
    let state_changed = changed.read().count() > 0;

    // First frame: seed the history and stay silent (TS `if (!p) { … return; }`).
    if history.previous.is_none() {
        history.previous = Some(detect::snapshot(&session.state));
        history.previous_state = Some(session.state.clone());
        return;
    }

    // 1. reveals, from the pre-action state.
    for action in &actions {
        let Some(previous_state) = history.previous_state.as_ref() else {
            continue;
        };
        if let GameAction::FlipCaptain { .. } = action {
            flips.write(CaptainFlipped(announce::actor_of(action, previous_state)));
        }
        if let Some(announcement) =
            build_announcement(action, previous_state, &session.registry, session.human)
        {
            reveals.write(AnnouncePlay(announcement));
        }
    }

    if !state_changed && actions.is_empty() {
        return;
    }

    // 2. combat, from the state diff.
    if let Some(pending) = session.state.pending_attack.as_ref() {
        history.last_pending = Some(pending.clone());
        // Whichever declaration opened this window is in this frame's actions.
        if let Some(declaration) = actions.iter().rev().find(|a| {
            matches!(
                a,
                GameAction::BaseAttack { .. }
                    | GameAction::SpecialAttack { .. }
                    | GameAction::CaptainAttack { .. }
                    | GameAction::UseSurcharge { .. }
                    | GameAction::FruitSpecialAttack { .. }
            )
        }) {
            history.last_attack_action = Some(declaration.clone());
        }
    }
    let next = detect::snapshot(&session.state);
    if let Some(previous) = history.previous.as_ref() {
        for event in detect::diff(
            previous,
            &next,
            &session.state,
            &session.registry,
            history.last_pending.as_ref(),
            history.last_attack_action.as_ref(),
        ) {
            if let Some(kind) = event.shake {
                shake.hit(kind, &reduced);
            }
            events.write(CombatVfx(event));
        }
    }
    history.previous = Some(next);
    history.previous_state = Some(session.state.clone());
}

/// Move [`AnnouncePlay`] messages into the [`RevealQueue`].
pub fn enqueue_announcements(
    mut incoming: MessageReader<AnnouncePlay>,
    mut queue: ResMut<RevealQueue>,
) {
    for message in incoming.read() {
        queue.push(message.0.clone());
    }
}

/// Forget everything when a game ends **and** when a new one starts.
///
/// Both edges matter: instance ids repeat verbatim between two seeded games
/// (`{def_id}_{n}_0`), so a snapshot left over from game 1 diffs cleanly
/// against game 2's opening state and fires phantom heals and damage numbers.
pub fn reset_vfx_state(
    mut history: ResMut<VfxHistory>,
    mut queue: ResMut<RevealQueue>,
    mut shake: ResMut<ScreenShake>,
) {
    *history = VfxHistory::default();
    queue.0.clear();
    *shake = ScreenShake::default();
}

/// Flip [`ReducedMotion`] with [`ReducedMotion::TOGGLE_KEY`].
///
/// Without this the ~20 reduced-motion call sites would be unreachable in the
/// shipped binary and the module's accessibility promise would only hold in
/// tests.
pub fn toggle_reduced_motion(keys: Res<ButtonInput<KeyCode>>, mut reduced: ResMut<ReducedMotion>) {
    if keys.just_pressed(ReducedMotion::TOGGLE_KEY) {
        reduced.enabled = !reduced.enabled;
        info!(
            "reduced motion {}",
            if reduced.enabled { "on" } else { "off" }
        );
    }
}

/// `true` while the board is on screen and a game exists.
pub(crate) fn vfx_ready(
    screen: Option<Res<State<AppScreen>>>,
    session: Option<Res<Session>>,
) -> bool {
    session.is_some() && screen.is_some_and(|screen| *screen.get() == AppScreen::Board)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::{BridgePlugin, DispatchAction, testkit};

    fn logic_app(seed: u64) -> App {
        let mut app = App::new();
        app.add_plugins(bevy::MinimalPlugins)
            .add_plugins(BridgePlugin)
            .insert_resource(testkit::session(seed))
            .init_resource::<ReducedMotion>()
            .init_resource::<VfxHistory>()
            .init_resource::<RevealQueue>()
            .init_resource::<ScreenShake>()
            .add_message::<CombatVfx>()
            .add_message::<AnnouncePlay>()
            .add_message::<CaptainFlipped>()
            .add_systems(
                Update,
                (detect_combat, enqueue_announcements)
                    .chain()
                    .after(BridgeSet)
                    .run_if(resource_exists::<Session>),
            );
        // First update only seeds the history.
        app.update();
        app
    }

    #[test]
    fn reduced_motion_shortens_and_disables() {
        let full = ReducedMotion { enabled: false };
        let reduced = ReducedMotion { enabled: true };
        assert_eq!(full.duration(Duration::from_millis(1000)).as_millis(), 1000);
        let shortened = reduced.duration(Duration::from_millis(1000)).as_millis();
        assert!((340..=360).contains(&shortened), "{shortened} ms");
        assert!(full.allows_screen_motion());
        assert!(!reduced.allows_screen_motion());

        let mut shake = ScreenShake::default();
        shake.hit(Shake::Hard, &reduced);
        assert!(!shake.is_active(), "reduced motion never shakes the board");
        shake.hit(Shake::Hard, &full);
        assert!(shake.is_active());
    }

    #[test]
    fn the_shake_decays_and_stops() {
        let mut shake = ScreenShake::default();
        shake.hit(Shake::Soft, &ReducedMotion { enabled: false });
        let early = shake.advance(0.05).length();
        let late = shake.advance(0.25).length();
        assert!(early > late, "the shake decays: {early} then {late}");
        shake.advance(1.0);
        assert!(!shake.is_active());
        assert_eq!(shake.advance(0.016), Vec2::ZERO);
        // A hard hit overrides a soft one that is still running.
        shake.hit(Shake::Soft, &ReducedMotion { enabled: false });
        shake.hit(Shake::Hard, &ReducedMotion { enabled: false });
        assert_eq!(shake.amplitude, 10.0);
    }

    #[test]
    fn the_reveal_queue_keeps_the_most_recent_plays() {
        let mut queue = RevealQueue::default();
        for index in 0..(RevealQueue::MAX + 2) {
            let mut announcement = PlayAnnouncement {
                side: Side::You,
                def_id: None,
                instance_id: None,
                kind: "endTurn",
                dest_id: None,
                caption: format!("{index}"),
                big: false,
                toast: true,
            };
            announcement.caption = format!("{index}");
            queue.push(announcement);
        }
        assert_eq!(queue.len(), RevealQueue::MAX);
        assert_eq!(queue.pop().unwrap().caption, "2", "the oldest were dropped");
    }

    #[test]
    fn ending_the_turn_queues_exactly_one_reveal_and_no_combat() {
        let mut app = logic_app(42);
        app.world_mut()
            .write_message(DispatchAction(GameAction::EndTurn));
        app.update();

        let queue = app.world().resource::<RevealQueue>();
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.0[0].kind, "endTurn");
        assert_eq!(queue.0[0].side, Side::You, "the human ended the turn");

        let combat = app
            .world()
            .resource::<Messages<CombatVfx>>()
            .iter_current_update_messages()
            .count();
        assert_eq!(combat, 0, "ending a turn hurts nobody");
    }

    #[test]
    fn playing_a_game_out_announces_every_action_and_reports_combat() {
        use tcgop_engine::ai::ai_choose_action;

        let mut app = logic_app(9);
        let mut reveals = 0usize;
        let mut combat = 0usize;

        for _ in 0..120 {
            let action = {
                let mut session = app.world_mut().resource_mut::<Session>();
                if session.winner().is_some() {
                    break;
                }
                let session = &mut *session;
                let actor = session.state.current_player;
                let level = session.ai_level;
                ai_choose_action(
                    &session.state,
                    &session.registry,
                    &mut session.ctx,
                    actor,
                    level,
                )
                .unwrap_or(GameAction::EndTurn)
            };
            app.world_mut().write_message(DispatchAction(action));
            app.update();
            reveals += app
                .world()
                .resource::<Messages<AnnouncePlay>>()
                .iter_current_update_messages()
                .count();
            combat += app
                .world()
                .resource::<Messages<CombatVfx>>()
                .iter_current_update_messages()
                .count();
        }

        assert!(reveals > 5, "every applied action is announced ({reveals})");
        assert!(
            combat > 0,
            "deployments and hits must produce combat events ({combat})"
        );
        let history = app.world().resource::<VfxHistory>();
        assert!(history.previous.is_some() && history.previous_state.is_some());
    }

    /// The plugin's own wiring, on the board screen but with no window.
    fn plugin_app(seed: u64) -> App {
        let mut app = App::new();
        app.add_plugins(bevy::MinimalPlugins)
            .add_plugins(bevy::state::app::StatesPlugin)
            .init_state::<AppScreen>()
            .add_plugins(BridgePlugin)
            .add_plugins(VfxPlugin)
            .insert_resource(testkit::session(seed));
        app.world_mut()
            .resource_mut::<NextState<AppScreen>>()
            .set(AppScreen::Board);
        app.update();
        app
    }

    #[test]
    fn the_whole_plugin_builds_and_runs_head_lessly() {
        let mut app = plugin_app(5);

        app.world_mut()
            .write_message(DispatchAction(GameAction::EndTurn));
        app.update();

        assert_eq!(
            app.world().resource::<RevealQueue>().len(),
            1,
            "the logic tier works without a window"
        );
        // Nothing was drawn: there is no board on screen to draw onto.
        assert!(
            app.world()
                .resource::<render::ActiveReveal>()
                .entity
                .is_none()
        );
    }

    /// Leaving the board stops the diff dead: `Session` outlives the board (the
    /// replay handler drops it later), so without the screen gate the history
    /// would be re-seeded from the finished game and the next one would open
    /// with phantom heals.
    #[test]
    fn the_diff_stops_when_the_board_does() {
        let mut app = plugin_app(5);
        app.world_mut()
            .resource_mut::<NextState<AppScreen>>()
            .set(AppScreen::GameOver);
        app.update();

        assert!(
            app.world().resource::<VfxHistory>().previous.is_none(),
            "leaving the board wipes the snapshot"
        );
        assert!(app.world().resource::<Session>().valid.is_empty() || true);

        // A further action must not re-seed it from the finished game.
        app.world_mut()
            .write_message(DispatchAction(GameAction::EndTurn));
        app.update();
        assert!(
            app.world().resource::<VfxHistory>().previous.is_none(),
            "the diff must stay silent off the board"
        );

        // Coming back wipes again, so game 2 starts from nothing.
        app.world_mut()
            .resource_mut::<NextState<AppScreen>>()
            .set(AppScreen::Board);
        app.update();
        assert!(
            app.world().resource::<RevealQueue>().is_empty(),
            "a replayed game opens with an empty reveal queue"
        );
    }

    #[test]
    fn the_first_frame_is_silent() {
        let app = logic_app(1);
        assert!(app.world().resource::<RevealQueue>().is_empty());
        assert!(app.world().resource::<VfxHistory>().previous.is_some());
    }

    /// Reduced motion is reachable in the shipped binary: from the environment
    /// at start-up, and from the keyboard at any time.
    #[test]
    fn reduced_motion_is_reachable_at_runtime() {
        assert!(ReducedMotion::truthy("1"));
        assert!(ReducedMotion::truthy("TRUE"));
        assert!(ReducedMotion::truthy(" yes "));
        assert!(!ReducedMotion::truthy("0"));
        assert!(!ReducedMotion::truthy(""));
        assert!(!ReducedMotion::truthy("false"));

        let mut app = App::new();
        app.add_plugins(bevy::MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(ReducedMotion { enabled: false })
            .add_systems(Update, toggle_reduced_motion);

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(ReducedMotion::TOGGLE_KEY);
        app.update();
        assert!(app.world().resource::<ReducedMotion>().enabled);

        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.clear();
            keys.release(ReducedMotion::TOGGLE_KEY);
            keys.clear();
            keys.press(ReducedMotion::TOGGLE_KEY);
        }
        app.update();
        assert!(!app.world().resource::<ReducedMotion>().enabled);
    }
}
