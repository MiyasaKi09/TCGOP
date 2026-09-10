//! Pure geometry for the hand: the fan transform, the floating preview anchor
//! and "object-fit: cover" cropping.
//!
//! Nothing here touches the ECS, so every number the hand draws is unit-tested
//! head-lessly.

use crate::app::layout as L;
use crate::art::Focus;

// ============================================================
// Fan
// ============================================================

/// What the hand knows about one card while it is laying the fan out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HandCardState {
    /// The pointer is over this card (TS `hoveredHand?.id === id`).
    pub hovered: bool,
    /// This card started the current selection (TS `selectedHandCard === id`).
    pub selected: bool,
}

/// The resolved transform of one hand card.
///
/// `offset_*` are logical pixels applied **after** the rotation (Bevy's
/// `UiTransform` is `p' = R·S·p + t`), and already contain the compensation
/// that turns Bevy's centre rotation into the web's
/// `transform-origin: bottom center`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FanTransform {
    /// Clockwise degrees, like the CSS `rotate()`.
    pub rotation_deg: f32,
    pub offset_x: f32,
    /// Positive = downwards (screen space).
    pub offset_y: f32,
    pub scale: f32,
}

impl FanTransform {
    pub const IDENTITY: FanTransform = FanTransform {
        rotation_deg: 0.0,
        offset_x: 0.0,
        offset_y: 0.0,
        scale: 1.0,
    };
}

/// Extra lift of the card that owns the current selection (TS `-translate-y-3`).
pub const SELECTED_LIFT: f32 = 12.0;
/// Extra scale of the selected card (TS `scale-105`).
pub const SELECTED_SCALE: f32 = 1.05;

/// The mock-up's fan, which rotates **only the outermost pair**:
///
/// ```css
/// .h:nth-child(1){transform:rotate(-7deg) translateY(6px)}
/// .h:nth-child(4){transform:rotate( 7deg) translateY(6px)}
/// ```
///
/// Every inner card is flat. Spreading the rotation across the whole hand (the
/// `off * STEP` the TSX uses) flattens a small hand to ±3° / ±1° and never
/// reaches the ±7° the mock-up shows, so the shape is taken from the mock-up:
/// the two ends lean out by [`HAND_FAN_ROT_MAX`](crate::app::layout::HAND_FAN_ROT_MAX)
/// and drop by [`HAND_FAN_LIFT_MAX`](crate::app::layout::HAND_FAN_LIFT_MAX),
/// the rest stand upright.
///
/// Hovering still straightens the card and lifts it, and the selected card
/// still rises and grows (TS `-translate-y-3 scale-105`).
///
/// `card_h` is the card height the pivot compensation is computed for
/// ([`HAND_CARD_H`](crate::app::layout::HAND_CARD_H) in practice).
pub fn fan_transform(
    index: usize,
    count: usize,
    state: HandCardState,
    card_h: f32,
) -> FanTransform {
    if count == 0 {
        return FanTransform::IDENTITY;
    }
    let last = count - 1;

    let (rotation_deg, lift) = if state.hovered {
        (0.0, -L::HAND_HOVER_LIFT)
    } else if count > 1 && index == 0 {
        (-L::HAND_FAN_ROT_MAX, L::HAND_FAN_LIFT_MAX)
    } else if count > 1 && index == last {
        (L::HAND_FAN_ROT_MAX, L::HAND_FAN_LIFT_MAX)
    } else {
        (0.0, 0.0)
    };

    // Rotating about the bottom edge instead of the centre: with
    // `p' = R·(p − P) + P` and `P = (0, h/2)` (y grows downwards) the extra
    // translation is `P − R·P`.
    let theta = rotation_deg.to_radians();
    let half_h = card_h / 2.0;
    let pivot_x = theta.sin() * half_h;
    let pivot_y = (1.0 - theta.cos()) * half_h;

    let selected_lift = if state.selected { SELECTED_LIFT } else { 0.0 };

    FanTransform {
        rotation_deg,
        offset_x: pivot_x,
        offset_y: pivot_y + lift - selected_lift,
        scale: if state.selected { SELECTED_SCALE } else { 1.0 },
    }
}

/// Horizontal gap between two hand cards.
///
/// The web puts the fan in an `overflow-x: auto` strip; a native window has no
/// scrollbar to spare, so a hand that would not fit tightens into a real fan —
/// the gap goes negative and the cards overlap, never more than 55 % of a card.
pub fn fan_gap(count: usize, available_w: f32) -> f32 {
    if count < 2 {
        return L::HAND_GAP;
    }
    let cards_w = count as f32 * L::HAND_CARD_W;
    let gaps = (count - 1) as f32;
    if cards_w + gaps * L::HAND_GAP <= available_w {
        return L::HAND_GAP;
    }
    ((available_w - cards_w) / gaps).max(-L::HAND_CARD_W * 0.55)
}

// ============================================================
// Hover preview anchor
// ============================================================

/// Top-left corner of the big floating card, in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreviewPlacement {
    pub left: f32,
    pub top: f32,
}

/// Margin the preview keeps from the window edges (TS literal `8`).
pub const PREVIEW_MARGIN: f32 = 8.0;
/// Gap between the hovered card and the preview (TS literal `12`).
pub const PREVIEW_GAP: f32 = 12.0;

/// TS, in the "Hand hover preview" block of `Game.tsx`:
///
/// ```text
/// left = clamp(rect.left + rect.width / 2 - PW / 2, 8, vw - PW - 8)
/// top  = rect.top - PH - 12;  if (top < 8) top = rect.bottom + 12
/// ```
///
/// `card_*` describe the hovered card's bounding box in logical pixels.
pub fn preview_placement(
    card_left: f32,
    card_top: f32,
    card_w: f32,
    card_bottom: f32,
    viewport_w: f32,
) -> PreviewPlacement {
    let pw = L::FULL_CARD_W;
    let ph = L::FULL_CARD_H.round();

    let raw_left = card_left + card_w / 2.0 - pw / 2.0;
    // `clamp` would panic when the window is narrower than the preview, so
    // apply the two bounds in the TS order (min then max) instead.
    let left = raw_left
        .max(PREVIEW_MARGIN)
        .min(viewport_w - pw - PREVIEW_MARGIN)
        .max(PREVIEW_MARGIN);

    let above = card_top - ph - PREVIEW_GAP;
    let top = if above < PREVIEW_MARGIN {
        card_bottom + PREVIEW_GAP
    } else {
        above
    };

    PreviewPlacement { left, top }
}

// ============================================================
// object-fit: cover
// ============================================================

/// Aspect ratio (`height / width`) of every illustration in `assets/cards`
/// (they are all 700x1050). Used until the real image dimensions are known.
pub const NOMINAL_ART_ASPECT: f32 = 1050.0 / 700.0;

/// Focal point `FullCard.tsx` crops its illustration around
/// (`background-position: center 16%`).
pub const CARD_FACE_FOCUS: Focus = Focus::new(0.5, 0.16);

/// Placement of an image inside a clipping box so that it covers the box
/// entirely while keeping its aspect ratio, anchored on `focus`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CoverFit {
    pub width: f32,
    pub height: f32,
    /// Offset of the image's top-left corner relative to the box (≤ 0).
    pub left: f32,
    pub top: f32,
}

/// CSS `background-size: cover; background-position: <focus>`.
///
/// `image_aspect` is `height / width` of the source image.
pub fn cover_fit(box_w: f32, box_h: f32, image_aspect: f32, focus: Focus) -> CoverFit {
    if box_w <= 0.0 || box_h <= 0.0 || image_aspect <= 0.0 {
        return CoverFit {
            width: box_w.max(0.0),
            height: box_h.max(0.0),
            left: 0.0,
            top: 0.0,
        };
    }
    let box_aspect = box_h / box_w;
    let (width, height) = if image_aspect >= box_aspect {
        // The image is relatively taller: match the width, overflow vertically.
        (box_w, box_w * image_aspect)
    } else {
        (box_h / image_aspect, box_h)
    };
    CoverFit {
        width,
        height,
        left: -(width - box_w) * focus.x,
        top: -(height - box_h) * focus.y,
    }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(index: usize, count: usize) -> FanTransform {
        fan_transform(index, count, HandCardState::default(), L::HAND_CARD_H)
    }

    #[test]
    fn the_middle_card_of_an_odd_hand_is_upright() {
        let t = plain(2, 5);
        assert_eq!(t.rotation_deg, 0.0);
        assert_eq!(t.offset_x, 0.0);
        assert_eq!(t.offset_y, 0.0);
        assert_eq!(t.scale, 1.0);
    }

    #[test]
    fn the_fan_is_symmetric_and_leans_outwards() {
        let left = plain(0, 5);
        let right = plain(4, 5);
        assert!(left.rotation_deg < 0.0, "the left card leans left");
        assert!(right.rotation_deg > 0.0, "the right card leans right");
        assert!((left.rotation_deg + right.rotation_deg).abs() < 1e-4);
        // Same downward lift on both ends, and it is a *downward* offset.
        assert!((left.offset_y - right.offset_y).abs() < 1e-4);
        assert!(left.offset_y > 0.0);
    }

    /// The mock-up's four-card hand: `±7°` on the ends, flat in between —
    /// never the `±3°/±1°` a per-card step would give.
    #[test]
    fn only_the_outermost_pair_leans() {
        let lift = |t: FanTransform| {
            t.offset_y - (1.0 - t.rotation_deg.to_radians().cos()) * L::HAND_CARD_H / 2.0
        };
        assert_eq!(plain(0, 4).rotation_deg, -L::HAND_FAN_ROT_MAX);
        assert_eq!(plain(3, 4).rotation_deg, L::HAND_FAN_ROT_MAX);
        assert!((lift(plain(0, 4)) - L::HAND_FAN_LIFT_MAX).abs() < 1e-4);
        for inner in 1..3 {
            let t = plain(inner, 4);
            assert_eq!(t.rotation_deg, 0.0, "card {inner} must stay flat");
            assert_eq!(t.offset_y, 0.0);
        }
        // A long hand behaves the same: the ends lean, the body is flat.
        assert_eq!(plain(11, 12).rotation_deg, L::HAND_FAN_ROT_MAX);
        assert_eq!(plain(6, 12).rotation_deg, 0.0);
    }

    /// A single card has no "outer pair": it stands upright.
    #[test]
    fn a_one_card_hand_is_upright() {
        let t = plain(0, 1);
        assert_eq!(t.rotation_deg, 0.0);
        assert_eq!(t.offset_y, 0.0);
    }

    #[test]
    fn hovering_straightens_the_card_and_lifts_it() {
        let t = fan_transform(
            0,
            5,
            HandCardState {
                hovered: true,
                selected: false,
            },
            L::HAND_CARD_H,
        );
        assert_eq!(t.rotation_deg, 0.0);
        assert_eq!(t.offset_x, 0.0);
        assert_eq!(t.offset_y, -L::HAND_HOVER_LIFT);
    }

    #[test]
    fn the_selected_card_rises_further_and_grows() {
        let plain_card = plain(2, 5);
        let selected = fan_transform(
            2,
            5,
            HandCardState {
                hovered: false,
                selected: true,
            },
            L::HAND_CARD_H,
        );
        assert_eq!(selected.scale, SELECTED_SCALE);
        assert!((selected.offset_y - (plain_card.offset_y - SELECTED_LIFT)).abs() < 1e-4);
        assert_eq!(selected.rotation_deg, plain_card.rotation_deg);
    }

    #[test]
    fn the_pivot_compensation_keeps_the_bottom_edge_still() {
        // Rotating about the bottom centre must leave that point where it was:
        // R·P + t == P for P = (0, h/2).
        for (i, n) in [(0usize, 5usize), (4, 5), (1, 7)] {
            let t = plain(i, n);
            let theta = t.rotation_deg.to_radians();
            let half = L::HAND_CARD_H / 2.0;
            // R·(0, half) with the standard CCW matrix.
            let rx = -theta.sin() * half;
            let ry = theta.cos() * half;
            // The lift is a deliberate extra translation: remove it first.
            let lift = t.offset_y - (1.0 - theta.cos()) * half;
            assert!((rx + t.offset_x).abs() < 1e-4, "x drifted for {i}/{n}");
            assert!(
                (ry + t.offset_y - lift - half).abs() < 1e-4,
                "y drifted for {i}/{n}"
            );
        }
    }

    #[test]
    fn an_empty_hand_is_harmless() {
        assert_eq!(
            fan_transform(0, 0, HandCardState::default(), L::HAND_CARD_H),
            FanTransform::IDENTITY
        );
    }

    // --- gap --------------------------------------------------

    #[test]
    fn a_small_hand_keeps_the_nominal_gap() {
        assert_eq!(fan_gap(0, 1000.0), L::HAND_GAP);
        assert_eq!(fan_gap(1, 1000.0), L::HAND_GAP);
        assert_eq!(fan_gap(5, 1000.0), L::HAND_GAP);
    }

    #[test]
    fn a_full_hand_overlaps_to_stay_on_screen() {
        // 10 cards in a strip much narrower than they need.
        let available = 6.0 * L::HAND_CARD_W;
        let gap = fan_gap(10, available);
        assert!(gap < 0.0, "the fan must tighten, got {gap}");
        let total = 10.0 * L::HAND_CARD_W + 9.0 * gap;
        assert!(
            (total - available).abs() < 1e-3,
            "the fan must fill exactly"
        );
    }

    #[test]
    fn the_overlap_is_bounded() {
        let gap = fan_gap(10, 1.0);
        assert!(gap >= -L::HAND_CARD_W * 0.55);
    }

    // --- preview ---------------------------------------------

    #[test]
    fn the_preview_is_centred_above_the_card() {
        let p = preview_placement(600.0, 600.0, L::HAND_CARD_W, 765.0, L::WINDOW_W);
        assert!((p.left - (600.0 + L::HAND_CARD_W / 2.0 - L::FULL_CARD_W / 2.0)).abs() < 1e-4);
        assert!((p.top - (600.0 - L::FULL_CARD_H.round() - PREVIEW_GAP)).abs() < 1e-4);
    }

    #[test]
    fn the_preview_stays_inside_the_window() {
        let left_edge = preview_placement(0.0, 600.0, L::HAND_CARD_W, 765.0, L::WINDOW_W);
        assert_eq!(left_edge.left, PREVIEW_MARGIN);

        let right_edge = preview_placement(
            L::WINDOW_W - L::HAND_CARD_W,
            600.0,
            L::HAND_CARD_W,
            765.0,
            L::WINDOW_W,
        );
        assert!(right_edge.left <= L::WINDOW_W - L::FULL_CARD_W - PREVIEW_MARGIN + 1e-4);
    }

    #[test]
    fn the_preview_flips_below_when_there_is_no_room_above() {
        let p = preview_placement(600.0, 40.0, L::HAND_CARD_W, 205.0, L::WINDOW_W);
        assert!((p.top - (205.0 + PREVIEW_GAP)).abs() < 1e-4);
    }

    // --- cover ------------------------------------------------

    #[test]
    fn cover_matches_the_width_when_the_image_is_taller() {
        // 700x1050 art (1.5) inside a 300x419 card (1.3967).
        let fit = cover_fit(300.0, 419.0, NOMINAL_ART_ASPECT, CARD_FACE_FOCUS);
        assert!((fit.width - 300.0).abs() < 1e-4);
        assert!((fit.height - 450.0).abs() < 1e-4);
        assert_eq!(fit.left, 0.0);
        assert!((fit.top - (-(450.0 - 419.0) * 0.16)).abs() < 1e-4);
        // The box is fully covered.
        assert!(fit.top <= 0.0 && fit.top + fit.height >= 419.0);
    }

    #[test]
    fn cover_matches_the_height_when_the_image_is_wider() {
        let fit = cover_fit(168.0, 94.0, 0.5, Focus::CENTER);
        assert!((fit.height - 94.0).abs() < 1e-4);
        assert!((fit.width - 188.0).abs() < 1e-4);
        assert!((fit.left - (-10.0)).abs() < 1e-4);
        assert_eq!(fit.top, 0.0);
    }

    #[test]
    fn cover_is_a_no_op_on_a_degenerate_box() {
        let fit = cover_fit(0.0, 10.0, 1.5, Focus::CENTER);
        assert_eq!(fit.left, 0.0);
        assert_eq!(fit.top, 0.0);
    }
}
