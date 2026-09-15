//! Pure geometry for the battlefield: how a half is sized inside the space the
//! header and the hand leave, and how an illustration is cropped "cover"-style
//! around its focal point.
//!
//! Nothing here touches the ECS, so every number the board draws can be
//! asserted in a head-less test.

use bevy::math::Vec2;

use crate::app::layout as l;
use crate::art::Focus;

// ============================================================
// Battlefield width
// ============================================================

/// Horizontal padding a half keeps from the window edges (mock-up
/// `.half{padding:6px 10px}`, scaled).
pub const HALF_PAD_X: f32 = 16.0;

/// Width of one line of slots at scale 1 — the natural (unscaled) width of the
/// battlefield column. The **live** width is [`HalfMetrics::battle_w`], which
/// follows the window; this constant only pins the relation the tests assert.
#[cfg_attr(not(test), allow(dead_code))]
pub const BATTLE_W: f32 = l::SLOT_W * 3.0 + l::SLOT_GAP * 2.0;

// ============================================================
// Vertical fit
// ============================================================

/// Never shrink a half below this fraction of its natural height: past that the
/// tiles stop being readable and it is better to clip.
pub const MIN_HALF_SCALE: f32 = 0.55;

/// …and never grow it past this one.
///
/// A tall window has spare height, and pinning the halves to their design size
/// there would leave the same phone-sized board sitting in ever bigger margins
/// — the very thing the mock-up's full-frame rows are not. The cap keeps the
/// text and the chrome from ballooning on a 4K display.
pub const MAX_HALF_SCALE: f32 = 1.6;

/// How finely [`HalfMetrics::scale`] is allowed to vary.
///
/// The scale is a continuous function of the window height, so without this a
/// vertical resize drag would produce a different value on *every* pixel — and
/// every distinct value tears down and respawns the whole board content
/// (`sync_cells` / `sync_command` / `sync_header` treat a changed
/// `BoardMetrics` as a rebuild trigger). Snapping it to 1/64 of natural size
/// turns a drag into at most one rebuild per ~6 px of window height, and the
/// step (~1.5 px of tile height) is invisible.
///
/// The snap always rounds **down**, so a quantised half never overflows the
/// space it was fitted into.
pub const SCALE_STEPS: f32 = 64.0;

/// The same idea on the horizontal axis: the window's width is read on a grid
/// of this many logical pixels, so a *horizontal* drag is as cheap as a
/// vertical one.
pub const WIDTH_STEP: f32 = 8.0;

/// Round `value` down to a multiple of `step` (`value` itself when either is
/// not a usable number).
fn snap(value: f32, step: f32) -> f32 {
    if value.is_finite() && value > 0.0 && step > 0.0 {
        (value / step).floor() * step
    } else {
        value
    }
}

/// The height one half wants: command bar + two captioned rows + the gaps
/// between them + the vertical padding.
pub const HALF_NATURAL_H: f32 =
    l::CMD_H + 2.0 * l::ROW_LABEL_H + 2.0 * l::SLOT_H + 4.0 * l::ROW_GAP + 2.0 * l::HALF_PAD_Y;

/// The resolved size of every box inside a half.
///
/// The mock-up is a phone; on a 1280x800 window the header and the hand leave
/// less than [`HALF_NATURAL_H`] per half, so everything is scaled by one
/// factor rather than letting flexbox shrink the boxes unevenly (which would
/// also break the "cover" crop of the tiles).
///
/// **Width scales with height.** A tile keeps the mock-up's
/// [`TILE_ASPECT`](crate::app::layout::TILE_ASPECT) whatever the window is, so
/// the illustration crop never drifts; the battlefield column is then exactly
/// as wide as three such tiles, capped by what the window actually offers. The
/// command bar's captain card and ship slot are the same fractions of that
/// column as in the mock-up, so they never overflow it either.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HalfMetrics {
    /// `available / natural`, snapped to [`SCALE_STEPS`] and clamped to
    /// `MIN_HALF_SCALE..=1`.
    pub scale: f32,
    pub cmd_h: f32,
    pub label_h: f32,
    pub slot_h: f32,
    pub slot_w: f32,
    /// `3 * slot_w + 2 * gap` — the width of a row, a caption and a command bar.
    pub battle_w: f32,
    pub captain_w: f32,
    pub ship_w: f32,
    pub gap: f32,
    pub pad_y: f32,
    /// The box the half was fitted into: window width x one half's height.
    ///
    /// Carried here so a painter can crop a full-bleed illustration (the ship
    /// deck floor) around its focal point *at spawn time* instead of showing
    /// one stretched frame while it waits for `ComputedNode`.
    pub half_box: Vec2,
}

impl HalfMetrics {
    /// Total height the metrics actually occupy.
    // Used by the fit tests to check a half never exceeds its budget.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn total_h(&self) -> f32 {
        self.cmd_h + 2.0 * self.label_h + 2.0 * self.slot_h + 4.0 * self.gap + 2.0 * self.pad_y
    }

    /// A fixed-pixel decoration of a tile (badge, HP-bar inset…) at this scale.
    pub fn chrome(&self, natural: f32) -> f32 {
        (natural * self.scale).max(1.0)
    }

    /// Horizontal gap between two slots of a row.
    pub fn gap_x(&self) -> f32 {
        l::SLOT_GAP * self.scale
    }
}

/// Fit a half into `available_h` x `available_w` logical pixels.
pub fn half_metrics(available_h: f32, available_w: f32) -> HalfMetrics {
    let scale = if available_h.is_finite() && available_h > 0.0 {
        let raw = available_h / HALF_NATURAL_H;
        ((raw * SCALE_STEPS).floor() / SCALE_STEPS).clamp(MIN_HALF_SCALE, MAX_HALF_SCALE)
    } else {
        1.0
    };
    let gap = l::SLOT_GAP * scale;
    let slot_h = l::SLOT_H * scale;
    // Both axes are read on their own grid, so every number below — the tile,
    // the column, the box the floor is cropped into — is a step function of the
    // window rather than a continuous one.
    let snapped_w = snap(available_w, WIDTH_STEP);
    let snapped_h = snap(available_h, HALF_NATURAL_H / SCALE_STEPS);

    // How much width one tile may take: the whole frame minus the half's
    // padding and the two gutters, split three ways.
    let room = if snapped_w.is_finite() && snapped_w > 0.0 {
        (snapped_w - 2.0 * HALF_PAD_X - 2.0 * gap) / l::SLOTS_PER_ROW as f32
    } else {
        f32::INFINITY
    };
    // The column **fills** the window rather than being pinned to the height
    // the tiles happen to have: the mock-up's rows span essentially the whole
    // frame, so a play column floating in a wide empty gutter is wrong at any
    // size. Width therefore grows to `room`; the tile only stops widening at
    // `MAX_TILE_ASPECT`, past which the "cover" crop of a portrait card would
    // be a letterbox slit. `TILE_ASPECT` stays the *floor*, so a narrow window
    // still binds on width the way it always did.
    let natural_w = l::SLOT_W * scale;
    let widest = room.min(slot_h * l::MAX_TILE_ASPECT);
    let slot_w = widest.max(natural_w.min(room)).max(1.0);
    let battle_w = l::SLOTS_PER_ROW as f32 * slot_w + 2.0 * gap;

    HalfMetrics {
        scale,
        cmd_h: l::CMD_H * scale,
        label_h: l::ROW_LABEL_H * scale,
        slot_h,
        slot_w,
        battle_w,
        captain_w: battle_w * l::CAPTAIN_CARD_FRACTION,
        ship_w: battle_w * l::SHIP_SLOT_FRACTION,
        gap: l::ROW_GAP * scale,
        pad_y: l::HALF_PAD_Y * scale,
        half_box: Vec2::new(
            if snapped_w.is_finite() && snapped_w > 0.0 {
                snapped_w
            } else {
                l::WINDOW_W
            },
            if snapped_h.is_finite() && snapped_h > 0.0 {
                snapped_h
            } else {
                HALF_NATURAL_H
            },
        ),
    }
}

/// The smallest window height the terrain still fits in.
///
/// Below it `half_metrics` bottoms out at [`MIN_HALF_SCALE`] and the halves are
/// clipped by the terrain's `overflow: clip`, so the window is told not to go
/// there (`main.rs`'s `resize_constraints`).
pub fn min_window_h() -> f32 {
    l::HEADER_H + l::HAND_H + l::WATERLINE_H + 2.0 * HALF_NATURAL_H * MIN_HALF_SCALE
}

/// The smallest window width three floor-scaled tiles and their gutters need.
pub fn min_window_w() -> f32 {
    BATTLE_W * MIN_HALF_SCALE + 2.0 * HALF_PAD_X
}

/// Height left for the terrain once the header and the hand took their share.
pub fn board_area_h(window_h: f32) -> f32 {
    (window_h - l::HEADER_H - l::HAND_H).max(0.0)
}

/// Height of one of the two halves.
pub fn half_available_h(window_h: f32) -> f32 {
    ((board_area_h(window_h) - l::WATERLINE_H) * 0.5).max(0.0)
}

// ============================================================
// "cover" crop
// ============================================================

/// An absolutely positioned image inside a clipped box, in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CoverRect {
    pub width: f32,
    pub height: f32,
    pub left: f32,
    pub top: f32,
}

/// CSS `background-size: cover; background-position: <focus>` — scale the image
/// so it covers the box, then slide it so that the point at `focus` of the
/// image sits at the same relative point of the box.
pub fn cover(box_w: f32, box_h: f32, img_w: f32, img_h: f32, focus: Focus) -> CoverRect {
    if !(box_w > 0.0 && box_h > 0.0 && img_w > 0.0 && img_h > 0.0) {
        return CoverRect {
            width: box_w.max(0.0),
            height: box_h.max(0.0),
            left: 0.0,
            top: 0.0,
        };
    }
    let scale = (box_w / img_w).max(box_h / img_h);
    let width = img_w * scale;
    let height = img_h * scale;
    CoverRect {
        width,
        height,
        left: -(width - box_w) * focus.x.clamp(0.0, 1.0),
        top: -(height - box_h) * focus.y.clamp(0.0, 1.0),
    }
}

// ============================================================
// Focal points
// ============================================================

/// Focus of a **board tile**: the def's entry in `CARD_ART_FOCUS`, or the
/// board default `50% 16%` ([`SLOT_ART_FOCUS_Y`](crate::app::layout::SLOT_ART_FOCUS_Y)).
///
/// `card_art_focus` answers `Focus::CENTER` for a def that has no entry, and no
/// entry in the table is centred, so "centre" reads as "unset" — exactly the
/// `CARD_ART_FOCUS[def.id] ?? "50% 16%"` of `Card.tsx`.
pub fn tile_focus(def_id: &str) -> Focus {
    let focus = crate::art::card_art_focus(def_id);
    if focus == Focus::CENTER {
        Focus::new(0.5, l::SLOT_ART_FOCUS_Y)
    } else {
        focus
    }
}

/// Focus of a **captain command card** — `background-position: center 14%`.
pub fn captain_focus(def_id: &str) -> Focus {
    let focus = crate::art::card_art_focus(def_id);
    if focus == Focus::CENTER {
        Focus::new(0.5, l::CAPTAIN_ART_FOCUS_Y)
    } else {
        focus
    }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_is_three_slots_wide() {
        assert_eq!(l::SLOTS_PER_ROW, 3, "BATTLE_W assumes three slots");
        assert_eq!(BATTLE_W, 3.0 * l::SLOT_W + 2.0 * l::SLOT_GAP);
    }

    #[test]
    fn a_half_shrinks_to_the_space_it_gets() {
        let m = half_metrics(HALF_NATURAL_H, l::WINDOW_W);
        assert_eq!(m.scale, 1.0);
        // …and grows into a taller window, up to the ceiling.
        let roomy = half_metrics(HALF_NATURAL_H * 1.3, l::WINDOW_W);
        assert!(roomy.scale > 1.0 && roomy.scale <= MAX_HALF_SCALE);
        assert_eq!(
            half_metrics(HALF_NATURAL_H * 4.0, l::WINDOW_W).scale,
            MAX_HALF_SCALE
        );
        assert_eq!(m.slot_h, l::SLOT_H);

        let tight = half_metrics(HALF_NATURAL_H * 0.8, l::WINDOW_W);
        assert!(tight.scale < 1.0);
        // The scale is snapped to `SCALE_STEPS`, always downward, so the half
        // fits inside what it was given and is at most one step short of it.
        let asked = HALF_NATURAL_H * 0.8;
        assert!(tight.total_h() <= asked + 1e-3);
        assert!(tight.total_h() >= asked - HALF_NATURAL_H / SCALE_STEPS);
        assert!(tight.slot_h < l::SLOT_H);
    }

    #[test]
    fn a_half_never_shrinks_past_the_floor() {
        let m = half_metrics(1.0, l::WINDOW_W);
        assert_eq!(m.scale, MIN_HALF_SCALE);
        let none = half_metrics(0.0, l::WINDOW_W);
        assert_eq!(
            none.scale, 1.0,
            "a degenerate size keeps the natural layout"
        );
    }

    /// A tile is never narrower than the mock-up aspect (that would letterbox
    /// the crop harder than it was designed for) and never wider than the cap.
    #[test]
    fn a_tile_stays_between_the_mock_up_aspect_and_the_cap() {
        for window_h in [640.0, 720.0, 800.0, 1080.0, 1440.0] {
            let m = half_metrics(half_available_h(window_h), l::WINDOW_W);
            let aspect = m.slot_w / m.slot_h;
            assert!(
                aspect >= l::TILE_ASPECT - 0.01,
                "{window_h}px window narrowed the tile to {aspect}"
            );
            assert!(
                aspect <= l::MAX_TILE_ASPECT + 0.01,
                "{window_h}px window stretched the tile to {aspect}"
            );
        }
    }

    /// The play column fills the window instead of floating in a gutter.
    ///
    /// At the sizes the game is actually played at the rows take more than half
    /// the frame (they used to take 26–31 % at *every* size); at the window
    /// floor the tiles are too short for the aspect cap to let them widen that
    /// far, but the column still takes a third of it and never overflows.
    #[test]
    fn the_column_fills_the_window_rather_than_a_gutter() {
        for (w, h) in [(1280.0, 800.0), (1920.0, 1080.0)] {
            let share = half_metrics(half_available_h(h), w).battle_w / w;
            assert!(
                share >= 0.5,
                "{w}x{h}: the battlefield is only {:.0}% of the window",
                share * 100.0
            );
        }
        for (w, h) in [
            (1280.0, 800.0),
            (1920.0, 1080.0),
            (2560.0, 1440.0),
            (1280.0, min_window_h()),
            (min_window_w(), min_window_h()),
        ] {
            let m = half_metrics(half_available_h(h), w);
            let share = m.battle_w / w;
            assert!(
                share >= 0.33,
                "{w}x{h}: the battlefield is only {:.0}% of the window",
                share * 100.0
            );
            assert!(
                m.battle_w <= w - 2.0 * HALF_PAD_X + 0.01,
                "{w}x{h}: the battlefield overflows the frame"
            );
        }
    }

    /// The battlefield column follows the window instead of being pinned to a
    /// design-time constant, and never overflows it.
    #[test]
    fn the_battlefield_follows_the_window() {
        let tall = half_metrics(half_available_h(1440.0), l::WINDOW_W);
        let short = half_metrics(half_available_h(700.0), l::WINDOW_W);
        assert!(tall.battle_w > short.battle_w);

        // A window too narrow for three natural tiles clamps on width.
        let narrow = half_metrics(HALF_NATURAL_H, 420.0);
        assert!(narrow.battle_w <= 420.0 - 2.0 * HALF_PAD_X + 0.01);
        assert!(narrow.slot_w < narrow.slot_h * l::TILE_ASPECT);
    }

    /// The window floor really is a floor: at exactly that height the halves
    /// still fit, so nothing is clipped.
    #[test]
    fn the_minimum_window_is_the_smallest_one_that_still_fits() {
        let h = min_window_h();
        let m = half_metrics(half_available_h(h), l::WINDOW_W);
        assert!(m.scale >= MIN_HALF_SCALE - 1e-4);
        assert!(
            m.total_h() <= half_available_h(h) + 0.01,
            "{} > {}",
            m.total_h(),
            half_available_h(h)
        );
        assert!(
            h <= l::WINDOW_H,
            "the design window must not be below the floor"
        );

        // One pixel less and the halves would be clipped.
        let tight = half_metrics(half_available_h(h - 8.0), l::WINDOW_W);
        assert!(tight.total_h() > half_available_h(h - 8.0));

        assert!(min_window_w() > 0.0 && min_window_w() <= l::WINDOW_W);
    }

    /// Mock-up `.cmd` is a direct child of `.half` while `.row` is
    /// `3 x 122 + 2 x 8 = 382 px`, so the captain card is 116/382 of the row it
    /// overhangs — not 116/420 of the half's content box, which made it ~9 %
    /// narrower than the tiles beside it.
    #[test]
    fn the_captain_card_is_a_fraction_of_the_row() {
        let m = half_metrics(half_available_h(l::WINDOW_H), l::WINDOW_W);
        assert!((m.captain_w / m.battle_w - 116.0 / 382.0).abs() < 1e-4);
        assert!((m.ship_w / m.battle_w - 70.0 / 382.0).abs() < 1e-4);
    }

    /// The scale is snapped, so a slow vertical resize does not produce a new
    /// set of metrics on literally every pixel — which is what made the board
    /// despawn and respawn its whole content once a frame while dragging.
    #[test]
    fn a_resize_sweep_only_changes_the_metrics_a_few_times() {
        let mut changes = 0;
        let mut previous = half_metrics(half_available_h(800.0), l::WINDOW_W);
        for step in 1..=120 {
            let next = half_metrics(half_available_h(800.0 + step as f32), l::WINDOW_W);
            if next != previous {
                changes += 1;
                previous = next;
            }
        }
        assert!(
            changes <= 30,
            "{changes} rebuilds over a 120 px drag (one per ~{:.0} px)",
            120.0 / changes as f32
        );
        assert!(changes > 0, "the board must still follow the window");
    }

    /// The command bar fits inside the column it shares with the rows.
    #[test]
    fn the_command_bar_fits_its_column() {
        let m = half_metrics(half_available_h(l::WINDOW_H), l::WINDOW_W);
        assert!(m.captain_w + m.ship_w + 2.0 * l::CMD_GAP < m.battle_w);
        assert!(m.captain_w > 0.0 && m.ship_w > 0.0);
    }

    #[test]
    fn the_window_splits_into_header_terrain_and_hand() {
        let area = board_area_h(l::WINDOW_H);
        assert_eq!(area, l::WINDOW_H - l::HEADER_H - l::HAND_H);
        assert_eq!(half_available_h(l::WINDOW_H), (area - l::WATERLINE_H) / 2.0);
        assert!(half_available_h(l::WINDOW_H) > 0.0);
    }

    #[test]
    fn cover_fills_a_landscape_box_with_a_portrait_card() {
        // 300x419 card in a 168x94 tile: width drives the scale.
        let r = cover(168.0, 94.0, 300.0, 419.0, Focus::new(0.5, 0.16));
        assert_eq!(r.width, 168.0);
        assert!(r.height > 94.0);
        assert!(r.left == 0.0, "no horizontal slack when width drives");
        assert!(r.top < 0.0, "the image is pulled up to show the face");
        // the focal point of the image lands on the focal point of the box
        let focus_in_box = r.top + r.height * 0.16;
        assert!((focus_in_box - 94.0 * 0.16).abs() < 0.01);
    }

    #[test]
    fn cover_centres_when_asked_to() {
        let r = cover(100.0, 100.0, 200.0, 100.0, Focus::CENTER);
        assert_eq!((r.width, r.height), (200.0, 100.0));
        assert_eq!((r.left, r.top), (-50.0, 0.0));
    }

    #[test]
    fn cover_survives_a_missing_image_size() {
        let r = cover(50.0, 20.0, 0.0, 0.0, Focus::CENTER);
        assert_eq!((r.width, r.height, r.left, r.top), (50.0, 20.0, 0.0, 0.0));
    }

    #[test]
    fn tile_focus_falls_back_to_the_board_default() {
        assert_eq!(tile_focus("MG-020"), Focus::new(0.5, l::SLOT_ART_FOCUS_Y));
        assert_eq!(tile_focus("MG-001"), Focus::new(0.50, 0.22));
        assert_eq!(
            captain_focus("nope"),
            Focus::new(0.5, l::CAPTAIN_ART_FOCUS_Y)
        );
        assert_eq!(captain_focus("CAP-LUFFY"), Focus::new(0.50, 0.24));
    }
}
