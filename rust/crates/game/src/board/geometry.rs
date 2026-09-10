//! Pure geometry for the battlefield: how a half is sized inside the space the
//! header and the hand leave, and how an illustration is cropped "cover"-style
//! around its focal point.
//!
//! Nothing here touches the ECS, so every number the board draws can be
//! asserted in a head-less test.

use crate::app::layout as l;
use crate::art::Focus;

// ============================================================
// Battlefield width
// ============================================================

/// Width of one line of slots — the battlefield column is exactly this wide, so
/// the command bar, the row captions and the three slots stay aligned.
pub const BATTLE_W: f32 = l::SLOT_W * 3.0 + l::SLOT_GAP * 2.0;

// ============================================================
// Vertical fit
// ============================================================

/// Never shrink a half below this fraction of its natural height: past that the
/// tiles stop being readable and it is better to clip.
pub const MIN_HALF_SCALE: f32 = 0.55;

/// The height one half wants: command bar + two captioned rows + the gaps
/// between them + the vertical padding.
pub const HALF_NATURAL_H: f32 = l::CMD_H
    + 2.0 * l::ROW_LABEL_H
    + 2.0 * l::SLOT_H
    + 4.0 * l::ROW_GAP
    + 2.0 * l::HALF_PAD_Y;

/// The resolved height of every box inside a half.
///
/// The mock-up is a phone; on a 1280x800 window the header and the hand leave
/// less than [`HALF_NATURAL_H`] per half, so everything is scaled by one
/// factor rather than letting flexbox shrink the boxes unevenly (which would
/// also break the "cover" crop of the tiles).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HalfMetrics {
    /// `available / natural`, clamped to `MIN_HALF_SCALE..=1`.
    pub scale: f32,
    pub cmd_h: f32,
    pub label_h: f32,
    pub slot_h: f32,
    pub gap: f32,
    pub pad_y: f32,
}

impl HalfMetrics {
    /// Total height the metrics actually occupy.
    pub fn total_h(&self) -> f32 {
        self.cmd_h + 2.0 * self.label_h + 2.0 * self.slot_h + 4.0 * self.gap + 2.0 * self.pad_y
    }
}

/// Fit a half into `available_h` logical pixels.
pub fn half_metrics(available_h: f32) -> HalfMetrics {
    let scale = if available_h.is_finite() && available_h > 0.0 {
        (available_h / HALF_NATURAL_H).clamp(MIN_HALF_SCALE, 1.0)
    } else {
        1.0
    };
    HalfMetrics {
        scale,
        cmd_h: l::CMD_H * scale,
        label_h: l::ROW_LABEL_H * scale,
        slot_h: l::SLOT_H * scale,
        gap: l::ROW_GAP * scale,
        pad_y: l::HALF_PAD_Y * scale,
    }
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
        let m = half_metrics(HALF_NATURAL_H);
        assert_eq!(m.scale, 1.0);
        assert_eq!(m.slot_h, l::SLOT_H);

        let tight = half_metrics(HALF_NATURAL_H * 0.8);
        assert!(tight.scale < 1.0);
        assert!((tight.total_h() - HALF_NATURAL_H * 0.8).abs() < 0.01);
        assert!(tight.slot_h < l::SLOT_H);
    }

    #[test]
    fn a_half_never_shrinks_past_the_floor() {
        let m = half_metrics(1.0);
        assert_eq!(m.scale, MIN_HALF_SCALE);
        let none = half_metrics(0.0);
        assert_eq!(none.scale, 1.0, "a degenerate size keeps the natural layout");
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
