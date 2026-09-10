//! Application shell: screens, plugin wiring, palette, fonts and the layout
//! constants every feature module shares.
//!
//! `TcgopPlugin` is the only thing `main.rs` knows about. It adds — in this
//! order — the engine bridge, the art cache, the UI state machine, then the
//! rendering / interaction features (screens, board, hand, panels, vfx) and
//! finally the AI driver.
//!
//! Two resources are inserted at plugin-build time so they are available to
//! every `Startup` / `OnEnter` system:
//! - [`Palette`] — the anime colour tokens of `public/tests/board-hs-anime-v2.html`
//!   (gold `#ffd36b`, atk `#ff7a8a`, def `#7fb6ff`, hp `#5fe39a`, foe `#ff7a8a`,
//!   dark navy backgrounds).
//! - [`Fonts`] — the four families in `assets/fonts` (Cinzel for names/titles,
//!   Oswald for UI labels, Spectral for flavour, Bangers for damage numbers).
//!
//! When the app is built head-less (`MinimalPlugins`, no `AssetServer`) the
//! font handles fall back to `Handle::default()`, so logic tests still run.

use bevy::prelude::*;

use crate::ai_driver::AiDriverPlugin;
use crate::art::ArtPlugin;
use crate::board::BoardPlugin;
use crate::bridge::BridgePlugin;
use crate::hand::HandPlugin;
use crate::panels::PanelsPlugin;
use crate::screens::ScreensPlugin;
use crate::selection::SelectionPlugin;
use crate::vfx::VfxPlugin;

// ============================================================
// Screens
// ============================================================

/// Top-level screen. `Setup` picks the decks and the difficulty and creates the
/// [`Session`](crate::bridge::Session); `Board` is the game itself; `GameOver`
/// shows the victory / defeat splash (`state.winner`).
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AppScreen {
    #[default]
    Setup,
    Board,
    GameOver,
}

// ============================================================
// The frame pipeline
// ============================================================

/// The six ordered stages of one `Update`. Every gameplay system belongs to
/// exactly one of them, so a click that lands in stage 1 is on screen — with
/// its vfx — at the end of the very same frame.
///
/// ```text
/// Input      raw pointer / keyboard events and the AI's decision become
///            intent messages (`HandCardClicked`, `DispatchAction`, …)
/// Selection  the `UiMode` state machine turns that intent into a `UiCommand`
///            and, when the selection is complete, a `DispatchAction`
/// Dispatch   the bridge applies every queued action to the engine
/// Refresh    `Session::valid` (and the view caches derived from it) are
///            recomputed from the new `GameState`
/// Render     the UI tree is rebuilt / re-tinted and the board anchors move
/// Vfx        combat feedback, cut-ins, reveals and tweens — they read the
///            anchors `Render` just wrote
/// ```
///
/// Sets are *additional* to the `before`/`after(BridgeSet)` relations the
/// feature plugins already declare, so every plugin keeps working on its own
/// in a head-less test that never calls [`configure_pipeline`].
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppSet {
    Input,
    Selection,
    Dispatch,
    Refresh,
    Render,
    Vfx,
}

/// Marks the pipeline as already chained, so [`configure_pipeline`] stays
/// idempotent when several plugins call it.
#[derive(Resource)]
struct PipelineConfigured;

/// Chain [`AppSet`] in `Update`. Called by every feature plugin's `build`, so
/// the ordering holds whichever subset of plugins a test assembles.
pub(crate) fn configure_pipeline(app: &mut App) {
    if app.world().contains_resource::<PipelineConfigured>() {
        return;
    }
    app.insert_resource(PipelineConfigured).configure_sets(
        Update,
        (
            AppSet::Input,
            AppSet::Selection,
            AppSet::Dispatch,
            AppSet::Refresh,
            AppSet::Render,
            AppSet::Vfx,
        )
            .chain(),
    );
}

// ============================================================
// Plugin
// ============================================================

/// The whole game, as one plugin.
pub struct TcgopPlugin;

impl Plugin for TcgopPlugin {
    fn build(&self, app: &mut App) {
        // Palette / fonts first: every other plugin may read them at build time.
        let fonts = match app.world().get_resource::<AssetServer>() {
            Some(assets) => Fonts::load(assets),
            None => Fonts::default(),
        };
        configure_pipeline(app);
        app.insert_resource(Palette::default())
            .insert_resource(fonts)
            .insert_resource(ClearColor(Palette::default().bg_deep))
            .init_state::<AppScreen>()
            .add_systems(Startup, spawn_camera)
            // Core (headless-testable) layers.
            .add_plugins((BridgePlugin, ArtPlugin, SelectionPlugin))
            // Presentation layers.
            .add_plugins((ScreensPlugin, BoardPlugin, HandPlugin, PanelsPlugin, VfxPlugin))
            // Opponent.
            .add_plugins(AiDriverPlugin);
    }
}

/// The single 2D camera used by both the sprite layers and the UI tree.
fn spawn_camera(mut commands: Commands) {
    commands.spawn((Camera2d, Name::new("MainCamera")));
}

// ============================================================
// Palette
// ============================================================

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::srgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
}

const fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color {
    Color::srgba(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a)
}

/// Anime colour tokens — the single source of truth for every colour the
/// client draws. Ported from `public/tests/board-hs-anime-v2.html` `:root`
/// and `src/lib/theme.ts`.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    // --- identity ---
    /// `--gd` `#ffd36b` — gold: titles, captain frames, selection.
    pub gold: Color,
    /// `#d9a434` — deep gold, for the lower half of gold gradients.
    pub gold_deep: Color,
    /// `#ff9d5b` — warm amber, the other end of the CTA gradient.
    pub amber: Color,

    // --- stat semantics ---
    /// `--atk` `#ff7a8a`.
    pub atk: Color,
    /// `--def` `#7fb6ff`.
    pub def: Color,
    /// `--hp` `#5fe39a`.
    pub hp: Color,
    /// `--foe` `#ff7a8a` — everything belonging to the opponent.
    pub foe: Color,
    /// HP ramp, ratio > 0.5.
    pub hp_full: Color,
    /// HP ramp, 0.25 < ratio <= 0.5.
    pub hp_mid: Color,
    /// HP ramp, ratio <= 0.25.
    pub hp_low: Color,
    /// Healing numbers / heal vfx.
    pub heal: Color,

    // --- interaction states ---
    /// Attack target ring (red).
    pub target: Color,
    /// Valid deploy slot (green).
    pub deploy: Color,
    /// Selected (gold).
    pub select: Color,
    /// Zone / AoE splash preview (orange).
    pub impact: Color,

    // --- factions ---
    /// Pirate accent `#e8954a`.
    pub pirate: Color,
    /// Marine accent `#5b97d8`.
    pub marine: Color,

    // --- surfaces ---
    /// `#070d1c` — page background, deep navy.
    pub bg_deep: Color,
    /// Panel / modal fill.
    pub bg_panel: Color,
    /// Empty-slot fill.
    pub bg_slot: Color,
    /// Scrim behind modals.
    pub scrim: Color,
    /// Hairline border ("ink edge").
    pub ink: Color,
    /// The luminous waterline that cuts the terrain in two.
    pub waterline: Color,

    // --- text ---
    /// Primary text `#f2f6ff`.
    pub text: Color,
    /// Secondary text (labels).
    pub text_dim: Color,
    /// Tertiary text (counts, hints).
    pub text_faint: Color,
    /// Text on a gold button.
    pub text_on_gold: Color,
}

impl Default for Palette {
    fn default() -> Self {
        Palette {
            gold: rgb(0xff, 0xd3, 0x6b),
            gold_deep: rgb(0xd9, 0xa4, 0x34),
            amber: rgb(0xff, 0x9d, 0x5b),

            atk: rgb(0xff, 0x7a, 0x8a),
            def: rgb(0x7f, 0xb6, 0xff),
            hp: rgb(0x5f, 0xe3, 0x9a),
            foe: rgb(0xff, 0x7a, 0x8a),
            hp_full: rgb(0x5f, 0xe3, 0x9a),
            hp_mid: rgb(0xe8, 0xc5, 0x3b),
            hp_low: rgb(0xff, 0x70, 0x62),
            heal: rgb(0x5b, 0xc4, 0x6a),

            target: rgb(0xe0, 0x46, 0x3f),
            deploy: rgb(0x5b, 0xc4, 0x6a),
            select: rgb(0xff, 0xd3, 0x6b),
            impact: rgb(0xe8, 0x95, 0x4a),

            pirate: rgb(0xe8, 0x95, 0x4a),
            marine: rgb(0x5b, 0x97, 0xd8),

            bg_deep: rgb(0x07, 0x0d, 0x1c),
            bg_panel: rgba(0x0a, 0x10, 0x1e, 0.97),
            bg_slot: rgba(0x0a, 0x10, 0x1e, 0.34),
            scrim: rgba(0x00, 0x00, 0x00, 0.72),
            ink: rgba(0xff, 0xff, 0xff, 0.20),
            waterline: rgb(0x5a, 0xa0, 0xff),

            text: rgb(0xf2, 0xf6, 0xff),
            text_dim: rgba(0xff, 0xff, 0xff, 0.60),
            text_faint: rgba(0xff, 0xff, 0xff, 0.40),
            text_on_gold: rgb(0x08, 0x13, 0x0c),
        }
    }
}

impl Palette {
    /// TS `hpColor(ratio)` (`src/lib/theme.ts`): green above 50 %, amber above
    /// 25 %, red below.
    pub fn hp_color(&self, ratio: f32) -> Color {
        if ratio > 0.5 {
            self.hp_full
        } else if ratio > 0.25 {
            self.hp_mid
        } else {
            self.hp_low
        }
    }

    /// The accent of a side: gold for the human half, `foe` red for the AI half.
    pub fn side(&self, is_you: bool) -> Color {
        if is_you { self.gold } else { self.foe }
    }
}

// ============================================================
// Fonts
// ============================================================

/// The four families shipped in `assets/fonts`.
///
/// Usage rule (product owner's brief):
/// - **Cinzel** — card / captain names, screen titles, the turn ring;
/// - **Oswald** — every UI label, stat chip, button;
/// - **Spectral** — flavour text, the log, rules prose;
/// - **Bangers** — damage numbers and cut-in shouts.
#[derive(Resource, Debug, Clone, Default)]
pub struct Fonts {
    pub cinzel: Handle<Font>,
    pub cinzel_bold: Handle<Font>,
    pub oswald: Handle<Font>,
    pub oswald_bold: Handle<Font>,
    pub spectral: Handle<Font>,
    pub spectral_bold: Handle<Font>,
    pub bangers: Handle<Font>,
}

impl Fonts {
    /// Load every family from the asset server (paths relative to `assets/`).
    pub fn load(assets: &AssetServer) -> Self {
        Fonts {
            cinzel: assets.load("fonts/Cinzel-Regular.ttf"),
            cinzel_bold: assets.load("fonts/Cinzel-Bold.ttf"),
            oswald: assets.load("fonts/Oswald-Regular.ttf"),
            oswald_bold: assets.load("fonts/Oswald-Bold.ttf"),
            spectral: assets.load("fonts/Spectral-Regular.ttf"),
            spectral_bold: assets.load("fonts/Spectral-Bold.ttf"),
            bangers: assets.load("fonts/Bangers-Regular.ttf"),
        }
    }
}

// ============================================================
// Layout
// ============================================================

/// Every hard-coded dimension of the board, in logical pixels, derived from the
/// validated mock-up `public/tests/board-hs-anime-v2.html` scaled to a
/// 1280x800 window.
///
/// Feature modules must read these rather than inventing their own numbers, so
/// the board, the hand and the panels stay aligned.
pub mod layout {
    // --- window ---
    pub const WINDOW_W: f32 = 1280.0;
    pub const WINDOW_H: f32 = 800.0;

    // --- top bar ---
    pub const HEADER_H: f32 = 46.0;
    pub const HEADER_PAD_X: f32 = 16.0;
    /// Diameter of the gold "turn number" ring.
    pub const TURN_RING_D: f32 = 30.0;

    // --- battlefield ---
    /// Height of the luminous waterline strip between the two halves.
    pub const WATERLINE_H: f32 = 16.0;
    /// Horizontal padding inside one half.
    pub const HALF_PAD_X: f32 = 18.0;
    /// Vertical padding inside one half.
    pub const HALF_PAD_Y: f32 = 10.0;
    /// Vertical gap between the elements of a half (label / row / row / command).
    pub const ROW_GAP: f32 = 8.0;
    /// Height of a "Ligne avant" / "Ligne arrière" caption.
    pub const ROW_LABEL_H: f32 = 13.0;

    // --- unit slots (full-illustration tiles) ---
    pub const SLOTS_PER_ROW: usize = 3;
    pub const SLOT_W: f32 = 168.0;
    pub const SLOT_H: f32 = 94.0;
    pub const SLOT_GAP: f32 = 12.0;
    pub const SLOT_RADIUS: f32 = 16.0;
    /// Thin HP bar drawn at the bottom of a tile — only when damaged.
    pub const SLOT_HP_BAR_H: f32 = 5.0;
    pub const SLOT_HP_BAR_INSET: f32 = 7.0;
    /// Status badge (top-right corner of a tile).
    pub const BADGE_D: f32 = 16.0;
    pub const BADGE_GAP: f32 = 3.0;
    /// Vertical focus of a cropped unit illustration when the def has no entry
    /// in `CARD_ART_FOCUS` (0 = top, 1 = bottom).
    pub const SLOT_ART_FOCUS_Y: f32 = 0.16;

    // --- command bar (captain + ship + volonté) ---
    pub const CMD_H: f32 = 96.0;
    pub const CMD_GAP: f32 = 10.0;
    pub const CAPTAIN_CARD_W: f32 = 150.0;
    pub const CAPTAIN_ART_FOCUS_Y: f32 = 0.14;
    pub const SHIP_SLOT_W: f32 = 92.0;
    pub const WILL_PIPS: usize = 10;
    pub const WILL_PIP_D: f32 = 12.0;
    pub const WILL_PIP_GAP: f32 = 4.0;

    // --- hand ---
    pub const HAND_H: f32 = 182.0;
    pub const HAND_PAD_X: f32 = 14.0;
    pub const HAND_CARD_W: f32 = 118.0;
    /// Card aspect ratio (the source illustrations are 300x419).
    pub const CARD_ASPECT: f32 = 419.0 / 300.0;
    pub const HAND_CARD_H: f32 = HAND_CARD_W * CARD_ASPECT;
    pub const HAND_GAP: f32 = 9.0;
    /// Fan: `rot = clamp(offset * STEP, -MAX, MAX)` degrees, `lift = min(|offset| * LIFT_STEP, LIFT_MAX)` px.
    pub const HAND_FAN_ROT_STEP: f32 = 2.0;
    pub const HAND_FAN_ROT_MAX: f32 = 7.0;
    pub const HAND_FAN_LIFT_STEP: f32 = 2.4;
    pub const HAND_FAN_LIFT_MAX: f32 = 14.0;
    /// Extra lift of the hovered card (it also straightens to 0°).
    pub const HAND_HOVER_LIFT: f32 = 6.0;
    /// Width of the big floating preview shown above a hovered hand card.
    pub const FULL_CARD_W: f32 = 250.0;
    pub const FULL_CARD_H: f32 = FULL_CARD_W * CARD_ASPECT;

    // --- action column / footer ---
    pub const ACTION_COL_W: f32 = 168.0;
    pub const BUTTON_H: f32 = 44.0;
    pub const BUTTON_RADIUS: f32 = 14.0;
    pub const LOG_H: f32 = 70.0;

    // --- modals ---
    pub const PANEL_MAX_W: f32 = 560.0;
    pub const PANEL_RADIUS: f32 = 16.0;
    pub const PANEL_PAD: f32 = 18.0;
    /// Info popover anchored above a clicked unit tile.
    pub const POPOVER_W: f32 = 210.0;

    // --- z ordering (UI `GlobalZIndex`, sprites use the same scale on `translation.z`) ---
    pub const Z_TERRAIN: i32 = 0;
    pub const Z_SHADE: i32 = 1;
    pub const Z_ROW: i32 = 3;
    pub const Z_COMMAND: i32 = 4;
    pub const Z_HEADER: i32 = 6;
    pub const Z_HAND: i32 = 8;
    pub const Z_POPOVER: i32 = 20;
    pub const Z_VFX: i32 = 30;
    pub const Z_MODAL: i32 = 50;
    pub const Z_REVEAL: i32 = 60;

    // --- font sizes (px) ---
    pub const FS_TITLE: f32 = 20.0;
    pub const FS_NAME: f32 = 13.0;
    pub const FS_LABEL: f32 = 10.0;
    pub const FS_TINY: f32 = 8.0;
    pub const FS_STAT: f32 = 11.0;
    pub const FS_BODY: f32 = 12.0;
    pub const FS_DAMAGE: f32 = 44.0;
}
