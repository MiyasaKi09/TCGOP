//! The UI state machine — a 1:1 port of `src/components/Game.tsx`.
//!
//! Everything here is **pure**: [`UiMode`] is a plain resource, and the
//! eligibility helpers are free functions over `(&UiMode, &[GameAction])`.
//! Rendering systems read them; nothing in this module touches the ECS world,
//! so the whole interaction logic is unit-testable without a window.
//!
//! Map of the TS source:
//!
//! | `Game.tsx` | here |
//! |---|---|
//! | `type UIMode` | [`UiMode`] |
//! | `selectedHandCard` | [`SelectedHandCard`] |
//! | `deploySlots` useMemo | [`deploy_slots`] |
//! | `equipTargets` useMemo | [`equip_targets`] |
//! | `supportTargets` useMemo | [`support_targets`] |
//! | `attackTargets` useMemo | [`attack_targets`] |
//! | `attackIsZone` useMemo | [`attack_is_zone`] |
//! | `selecting` | [`UiMode::is_selecting`] |
//! | `canPlay` (hand) | [`hand_card_playable`] |
//! | `renderLine` highlighting | [`cell_highlight`] |
//! | `handleHandCardClick` | [`on_hand_card_click`] |
//! | `handleSlotClick` | [`on_slot_click`] |
//! | `handleBoardCharClick` | [`on_board_char_click`] |
//! | `onCaptainClick` | [`on_captain_click`] |
//! | `act()` in `renderLine` | [`on_cell_click`] |
//! | `statusText` | [`status_hint`] |
//!
//! The Rust engine offers four declarations the TS one never had, so four
//! entries have no `Game.tsx` counterpart — they exist so that **every**
//! `GameAction` `valid_actions()` can return is reachable from the UI:
//!
//! | engine decision | here |
//! |---|---|
//! | §8.34(a)/(b) — the captain's ★ special and its `surcharge` | [`AttackKind`] |
//! | §8.28 follow-up × §8.48 — an awakened fruit's own special | [`AttackKind::Fruit`], [`fruit_special_ids`] |
//! | §8.47 — `awakenFruit` | [`awakenable_fruits`] |
//! | §8.29 — the free `moveCharacter` | [`UiMode::SelectingMoveSlot`], [`move_slots`] |

use std::collections::BTreeSet;

use bevy::prelude::*;

use crate::app::configure_pipeline;
use tcgop_engine::combat::CAPTAIN_ATTACKER_PREFIX;
use tcgop_engine::registry::CardRegistry;
use tcgop_engine::state::GameState;
use tcgop_engine::types::{AttackTrait, CardType, GameAction, PlayerId, Slot};

// ============================================================
// Plugin
// ============================================================

/// Inserts [`UiMode`] and [`SelectedHandCard`].
pub struct SelectionPlugin;

impl Plugin for SelectionPlugin {
    fn build(&self, app: &mut App) {
        configure_pipeline(app);
        app.init_resource::<UiMode>()
            .init_resource::<SelectedHandCard>();
    }
}

// ============================================================
// UiMode
// ============================================================

/// Which ability the aimed attack belongs to — the `isSpecial` boolean of
/// `Game.tsx`, widened to the four declaration paths the Rust engine offers.
///
/// The TS engine only ever had two (`baseAttack` / `specialAttack`); the port
/// added the captain's `surcharge` (§8.2 item 34(b)) and made the awakened
/// Devil-Fruit special a declaration of its own (`fruitSpecialAttack`, §8.28
/// follow-up × §8.48), each with its own target list, its own cost and its own
/// zone traits. Carrying the ability in the mode is what lets
/// [`attack_targets`] narrow on it and [`target_action`] rebuild exactly the
/// `GameAction` the engine offered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttackKind {
    /// `baseAttack`, or `captainAttack` with no `isSpecial`.
    Base,
    /// `specialAttack`, or `captainAttack { isSpecial: true }` (§8.34(a)).
    Special,
    /// `useSurcharge` — the captain's active-face `surcharge` (§8.34(b)).
    Surcharge,
    /// `fruitSpecialAttack` — the awakened fruit's own special. The fruit is
    /// part of the action, so the mode has to remember which one is aimed
    /// (a bearer may wear more than one awakened fruit).
    Fruit { fruit_instance_id: String },
}

impl AttackKind {
    /// TS `isSpecial` — everything but the base action reads the *special*
    /// side of a definition (cost, `atkBonus`, `attackTraits`).
    pub fn is_special(&self) -> bool {
        !matches!(self, AttackKind::Base)
    }
}

/// Exactly the arms of `Game.tsx`'s `UIMode` union, plus the one the Rust
/// engine's free move needs (`selectingMoveSlot`, §8.29).
///
/// Only one mode is active at a time. `Idle` is the resting state; every
/// dispatch resets back to it (see [`UiCommand`]).
#[derive(Resource, Debug, Clone, PartialEq, Eq, Default)]
pub enum UiMode {
    /// `{ type: "idle" }`
    #[default]
    Idle,
    /// `{ type: "selectingSlot", cardId }` — a character is in hand, pick a slot.
    SelectingSlot { card_id: String },
    /// `{ type: "selectingTarget", attackerId, kind }`.
    ///
    /// `attacker_id` is either a card instance id or the synthetic
    /// `"captain_<player>"` id (see [`captain_key`]).
    SelectingTarget {
        attacker_id: String,
        kind: AttackKind,
    },
    /// The free repositioning of §8.29 (`moveCharacter`): a board unit is
    /// armed, pick one of the adjacent empty slots it may step into.
    SelectingMoveSlot { instance_id: String },
    /// `{ type: "selectingSupportTarget", instanceId }`.
    SelectingSupportTarget { instance_id: String },
    /// `{ type: "selectingEquipTarget", objectId }`.
    SelectingEquipTarget { object_id: String },
    /// `{ type: "actionMenu", instanceId }` — the per-unit action popover.
    ActionMenu { instance_id: String },
    /// `{ type: "captainMenu", playerId }`.
    CaptainMenu { player_id: PlayerId },
    /// `{ type: "shipMenu", instanceId, isYou }`.
    ShipMenu { instance_id: String, is_you: bool },
    /// `{ type: "cardDetail", defId, instanceId? }`.
    CardDetail {
        def_id: String,
        instance_id: Option<String>,
    },
    /// `{ type: "confirmEvent", instanceId }`.
    ConfirmEvent { instance_id: String },
    /// `{ type: "confirmShip", instanceId }`.
    ConfirmShip { instance_id: String },
    /// `{ type: "selectingCaptainSlot" }` — pick the slot the verso lands on.
    SelectingCaptainSlot,
}

impl UiMode {
    /// TS `selecting` — one of the five board-targeting modes is active, so
    /// every non-eligible cell must be dimmed.
    pub fn is_selecting(&self) -> bool {
        matches!(
            self,
            UiMode::SelectingTarget { .. }
                | UiMode::SelectingSupportTarget { .. }
                | UiMode::SelectingSlot { .. }
                | UiMode::SelectingEquipTarget { .. }
                | UiMode::SelectingCaptainSlot
                | UiMode::SelectingMoveSlot { .. }
        )
    }

    /// `true` for the modal modes (a panel is open above the board).
    // Used by the mode-partition test, the mirror of `is_selecting`.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn is_modal(&self) -> bool {
        matches!(
            self,
            UiMode::ActionMenu { .. }
                | UiMode::CaptainMenu { .. }
                | UiMode::ShipMenu { .. }
                | UiMode::CardDetail { .. }
                | UiMode::ConfirmEvent { .. }
                | UiMode::ConfirmShip { .. }
        )
    }

    pub fn is_idle(&self) -> bool {
        matches!(self, UiMode::Idle)
    }
}

/// TS `selectedHandCard` — the hand card that is currently lit up.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct SelectedHandCard(pub Option<String>);

/// The synthetic id the UI uses for a captain, TS `` `captain_${playerId}` ``
/// (same string the engine's `combat::captain_attacker_id` builds).
pub fn captain_key(player: PlayerId) -> String {
    format!("{CAPTAIN_ATTACKER_PREFIX}{}", player.as_str())
}

/// `true` when an id is a captain key rather than a card instance id.
pub fn is_captain_key(id: &str) -> bool {
    id.starts_with(CAPTAIN_ATTACKER_PREFIX)
}

/// The player a captain key belongs to.
pub fn captain_key_owner(id: &str) -> Option<PlayerId> {
    match id.strip_prefix(CAPTAIN_ATTACKER_PREFIX)? {
        "player1" => Some(PlayerId::Player1),
        "player2" => Some(PlayerId::Player2),
        _ => None,
    }
}

// ============================================================
// Eligibility (the `useMemo` blocks)
// ============================================================

/// TS `deploySlots` — the slots lit up in `selectingSlot` (character deploy)
/// and in `selectingCaptainSlot` (captain flip). Empty in every other mode.
pub fn deploy_slots(mode: &UiMode, valid: &[GameAction]) -> BTreeSet<Slot> {
    let mut slots = BTreeSet::new();
    match mode {
        UiMode::SelectingSlot { card_id } => {
            for a in valid {
                if let GameAction::DeployCharacter { instance_id, slot } = a
                    && instance_id == card_id
                {
                    slots.insert(*slot);
                }
            }
        }
        UiMode::SelectingCaptainSlot => {
            for a in valid {
                if let GameAction::FlipCaptain { slot } = a {
                    slots.insert(*slot);
                }
            }
        }
        // §8.29 — the free move lights the adjacent empty slots of the armed
        // unit, and it lights them with the same green ring a deploy uses:
        // both answer "put a body in this empty cell".
        UiMode::SelectingMoveSlot { instance_id } => {
            for a in valid {
                if let GameAction::MoveCharacter {
                    instance_id: id,
                    target_slot,
                } = a
                    && id == instance_id
                {
                    slots.insert(*target_slot);
                }
            }
        }
        _ => {}
    }
    slots
}

/// The slots `instance_id` may step into — [`deploy_slots`] without needing the
/// mode, for the action menu's *Déplacer* row (§8.29).
pub fn move_slots(valid: &[GameAction], instance_id: &str) -> BTreeSet<Slot> {
    let mut slots = BTreeSet::new();
    for a in valid {
        if let GameAction::MoveCharacter {
            instance_id: id,
            target_slot,
        } = a
            && id == instance_id
        {
            slots.insert(*target_slot);
        }
    }
    slots
}

/// Every fruit `attacker_id` may awaken right now — `awakenFruit` carries only
/// the fruit id, so the bearer is matched through its `attached_objects`
/// (a character's, or the captain's since §8.28's follow-up).
pub fn awakenable_fruits(valid: &[GameAction], attached: &[String]) -> BTreeSet<String> {
    let mut fruits = BTreeSet::new();
    for a in valid {
        if let GameAction::AwakenFruit { fruit_instance_id } = a
            && attached.iter().any(|id| id == fruit_instance_id)
        {
            fruits.insert(fruit_instance_id.clone());
        }
    }
    fruits
}

/// Every awakened fruit whose special `attacker_id` may declare right now
/// (§8.28 follow-up × §8.48). `attacker_id` is a card instance id **or** a
/// captain key, exactly as the engine enumerates it.
pub fn fruit_special_ids(valid: &[GameAction], attacker_id: &str) -> BTreeSet<String> {
    let mut fruits = BTreeSet::new();
    for a in valid {
        if let GameAction::FruitSpecialAttack {
            attacker_instance_id,
            fruit_instance_id,
            ..
        } = a
            && attacker_instance_id == attacker_id
        {
            fruits.insert(fruit_instance_id.clone());
        }
    }
    fruits
}

/// TS `equipTargets` — the allies the selected object may be attached to.
pub fn equip_targets(mode: &UiMode, valid: &[GameAction]) -> BTreeSet<String> {
    let mut targets = BTreeSet::new();
    let UiMode::SelectingEquipTarget { object_id } = mode else {
        return targets;
    };
    for a in valid {
        // Decision §8.28 (follow-up): an equip target may be the player's own
        // captain, which is addressed by its `captain_{playerId}` key — the
        // same key the captain command card is drawn under, so it highlights
        // like any other bearer. A board cell never holds that key, so no cell
        // can be lit by mistake.
        if let GameAction::EquipObject {
            object_instance_id,
            target_instance_id,
            ..
        } = a
            && object_instance_id == object_id
        {
            targets.insert(target_instance_id.clone());
        }
    }
    targets
}

/// TS `supportTargets` — the units a support action may be aimed at (heal,
/// immobilise, buff…). Support actions without a target are dispatched
/// immediately instead, see [`support_needs_target`].
pub fn support_targets(mode: &UiMode, valid: &[GameAction]) -> BTreeSet<String> {
    let mut targets = BTreeSet::new();
    let UiMode::SelectingSupportTarget { instance_id } = mode else {
        return targets;
    };
    for a in valid {
        if let GameAction::BaseSupportAction {
            instance_id: id,
            target_instance_id: Some(target),
        } = a
            && id == instance_id
        {
            targets.insert(target.clone());
        }
    }
    targets
}

/// TS `hasSupportTargets` in `ActionMenu.onSupportAction` — does this unit's
/// support action need a target picked on the board?
pub fn support_needs_target(valid: &[GameAction], instance_id: &str) -> bool {
    valid.iter().any(|a| {
        matches!(
            a,
            GameAction::BaseSupportAction { instance_id: id, target_instance_id: Some(_) }
                if id == instance_id
        )
    })
}

/// TS `attackTargets` — every legal target of the attack being aimed, as a set
/// of ids where the enemy captain is the synthetic `captain_<ai>` key.
///
/// A captain's targets are the `captainAttack` entries whose `isSpecial`
/// matches the ability being aimed. The TS never narrowed on that flag because
/// its engine had no captain special attack at all; the Rust engine does
/// (§8.2 item 34(a)) and only offers it while it is affordable and unused, so
/// the base and the ★ ability genuinely have different target lists.
pub fn attack_targets(
    mode: &UiMode,
    valid: &[GameAction],
    ai_player: PlayerId,
) -> BTreeSet<String> {
    let mut targets = BTreeSet::new();
    let UiMode::SelectingTarget { attacker_id, kind } = mode else {
        return targets;
    };
    let captain_attacker = is_captain_key(attacker_id);

    for a in valid {
        // `(target id, targets the captain?)` when this action is the very
        // ability the mode is aiming, `None` otherwise. Narrowing here rather
        // than at the call site is what keeps the ★ special, the surcharge and
        // each awakened fruit on their own target lists.
        let hit = match (a, kind) {
            (
                GameAction::CaptainAttack {
                    target_instance_id,
                    target_is_captain,
                    is_special,
                },
                AttackKind::Base | AttackKind::Special,
            ) if captain_attacker && is_special.unwrap_or(false) == kind.is_special() => {
                Some((target_instance_id, *target_is_captain))
            }
            (
                GameAction::UseSurcharge {
                    target_instance_id,
                    target_is_captain,
                },
                AttackKind::Surcharge,
            ) if captain_attacker => Some((target_instance_id, *target_is_captain)),
            (
                GameAction::BaseAttack {
                    attacker_instance_id,
                    target_instance_id,
                    target_is_captain,
                },
                AttackKind::Base,
            ) if attacker_instance_id == attacker_id => {
                Some((target_instance_id, *target_is_captain))
            }
            (
                GameAction::SpecialAttack {
                    attacker_instance_id,
                    target_instance_id,
                    target_is_captain,
                },
                AttackKind::Special,
            ) if attacker_instance_id == attacker_id => {
                Some((target_instance_id, *target_is_captain))
            }
            (
                GameAction::FruitSpecialAttack {
                    attacker_instance_id,
                    fruit_instance_id,
                    target_instance_id,
                    target_is_captain,
                },
                AttackKind::Fruit {
                    fruit_instance_id: aimed,
                },
            ) if attacker_instance_id == attacker_id && fruit_instance_id == aimed => {
                Some((target_instance_id, *target_is_captain))
            }
            _ => None,
        };
        let Some((target, is_captain)) = hit else {
            continue;
        };
        if is_captain.unwrap_or(false) {
            targets.insert(captain_key(ai_player));
        } else {
            targets.insert(target.clone());
        }
    }
    targets
}

/// TS `attackIsZone` — does the attack being aimed carry the `zone` trait?
/// Used to light the whole enemy front line as the impact area.
///
/// Captains read their **verso** action, exactly like the TS.
pub fn attack_is_zone(mode: &UiMode, state: &GameState, registry: &CardRegistry) -> bool {
    let UiMode::SelectingTarget { attacker_id, kind } = mode else {
        return false;
    };

    fn has_zone(traits: Option<&Vec<AttackTrait>>) -> bool {
        traits.is_some_and(|t| t.contains(&AttackTrait::Zone))
    }

    // The awakened fruit carries its own `attackTraits`, wherever it is worn —
    // on a character or, since §8.28's follow-up, on the captain — so it is
    // resolved before the bearer is even looked at.
    if let AttackKind::Fruit { fruit_instance_id } = kind {
        return fruit_awakening_special(state, registry, fruit_instance_id)
            .is_some_and(|spec| has_zone(spec.attack_traits.as_ref()));
    }

    if let Some(owner) = captain_key_owner(attacker_id) {
        let Some(def) = registry.captain_def(&state.player(owner).captain.def_id) else {
            return false;
        };
        return match kind {
            AttackKind::Base => has_zone(def.verso.base_action.attack_traits.as_ref()),
            AttackKind::Special => has_zone(def.verso.special_attack.attack_traits.as_ref()),
            // §8.34(b): a surcharge is always the **verso**'s — the recto
            // captain cannot attack, so the engine never reads `recto.surcharge`.
            AttackKind::Surcharge => def
                .verso
                .surcharge
                .as_ref()
                .is_some_and(|s| has_zone(s.attack_traits.as_ref())),
            AttackKind::Fruit { .. } => false,
        };
    }

    let Some(instance) = state.card(attacker_id) else {
        return false;
    };
    let Some(def) = registry.card_def(&instance.def_id) else {
        return false;
    };
    if kind.is_special() {
        def.special_attack
            .as_ref()
            .is_some_and(|a| has_zone(a.attack_traits.as_ref()))
    } else {
        def.base_action
            .as_ref()
            .is_some_and(|a| has_zone(a.attack_traits.as_ref()))
    }
}

/// The `fruitEffects.awakening.specialAttack` block of an **instance** of a
/// Devil Fruit — the definition behind an [`AttackKind::Fruit`].
pub fn fruit_awakening_special<'a>(
    state: &GameState,
    registry: &'a CardRegistry,
    fruit_instance_id: &str,
) -> Option<&'a tcgop_engine::types::FruitAwakeningSpecialAttack> {
    let fruit = state.card(fruit_instance_id)?;
    registry
        .card_def(&fruit.def_id)?
        .fruit_effects
        .as_ref()?
        .awakening
        .as_ref()?
        .special_attack
        .as_ref()
}

/// TS `canPlay` in the hand loop — is any legal action tied to this hand card?
pub fn hand_card_playable(valid: &[GameAction], instance_id: &str) -> bool {
    valid.iter().any(|a| match a {
        GameAction::DeployCharacter {
            instance_id: id, ..
        }
        | GameAction::DeployShip { instance_id: id }
        | GameAction::BaseSupportAction {
            instance_id: id, ..
        }
        | GameAction::PlayEvent {
            instance_id: id, ..
        }
        | GameAction::PlayCounter { instance_id: id } => id == instance_id,
        GameAction::EquipObject {
            object_instance_id, ..
        } => object_instance_id == instance_id,
        _ => false,
    })
}

// ============================================================
// Cell highlighting (renderLine)
// ============================================================

/// Everything `renderLine` computes for one board cell before drawing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CellHighlight {
    /// Green ring: a character (or the captain verso) may be placed here.
    pub valid_deploy: bool,
    /// Red ring: the occupant is a legal attack target, or a support target.
    pub valid_target: bool,
    /// Gold ring: the occupant may receive the selected equipment.
    pub equip_target: bool,
    /// Orange splash: front-line cell inside a zone attack's blast.
    pub impact: bool,
    /// Dimmed: a selection is running and this cell is not eligible.
    pub dimmed: bool,
}

impl CellHighlight {
    /// TS `eligible` — the cell answers the current selection.
    pub fn eligible(&self) -> bool {
        self.valid_target || self.valid_deploy || self.equip_target
    }
}

/// TS `renderLine`'s per-cell flags.
///
/// `occupant` is the instance id in that slot (`None` for an empty slot), and
/// the three sets come from [`deploy_slots`] / [`attack_targets`] /
/// [`support_targets`] / [`equip_targets`]; `zone` from [`attack_is_zone`].
#[allow(clippy::too_many_arguments)]
pub fn cell_highlight(
    mode: &UiMode,
    slot: Slot,
    is_player_side: bool,
    occupant: Option<&str>,
    deploy: &BTreeSet<Slot>,
    attack: &BTreeSet<String>,
    support: &BTreeSet<String>,
    equip: &BTreeSet<String>,
    zone: bool,
) -> CellHighlight {
    let selecting = mode.is_selecting();
    let is_valid_deploy = is_player_side && deploy.contains(&slot);
    // Side is deliberately **not** tested here. The TS only ever aimed enemies,
    // but the Rust engine offers three declarations whose target stands on the
    // caster's own half: an ally-facing support special (§8.38 `RH-009`
    // Stimulant — `buffAllyAtk` + `cleanse`), a self-resolving support special
    // and a self-transformation (§8.39, Chopper's Monster Point). Gating on
    // `!is_player_side` left every one of them lit nowhere and clickable
    // nowhere. Membership in `attack` is the authority — it is built from
    // `Session::valid`, so a cell only lights when the engine offered it.
    let is_attack_target = matches!(mode, UiMode::SelectingTarget { .. })
        && occupant.is_some_and(|id| attack.contains(id));
    let is_support_target = matches!(mode, UiMode::SelectingSupportTarget { .. })
        && occupant.is_some_and(|id| support.contains(id));
    let valid_target = is_attack_target || is_support_target;
    let equip_target = is_player_side
        && matches!(mode, UiMode::SelectingEquipTarget { .. })
        && occupant.is_some_and(|id| equip.contains(id));
    let impact =
        !is_player_side && zone && slot.row() == tcgop_engine::types::Row::Front && valid_target;

    let highlight = CellHighlight {
        valid_deploy: is_valid_deploy,
        valid_target,
        equip_target,
        impact,
        dimmed: false,
    };
    CellHighlight {
        dimmed: selecting && !highlight.eligible(),
        ..highlight
    }
}

// ============================================================
// Click handling
// ============================================================

/// What a click resolves to.
///
/// `Dispatch` implies "and reset the UI": every TS handler calls `resetUI()`
/// right after dispatching, so callers must write the action to
/// [`DispatchAction`](crate::bridge::DispatchAction) **and** apply
/// [`UiCommand::Reset`]'s effect (mode → `Idle`, selected hand card → `None`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiCommand {
    /// The click is not actionable — swallow it.
    Ignore,
    /// Send this action to the engine, then reset the UI.
    Dispatch(GameAction),
    /// Send this action to the engine and **leave the UI as it is**.
    ///
    /// Every TS dispatch site pairs `dispatch(...)` with `resetUI()` — except
    /// the four counter-window buttons (`Game.tsx:503,511,518,525`), which
    /// deliberately do not: a panel or a half-finished selection the player had
    /// open when the AI's attack arrived must survive answering it.
    DispatchKeepUi(GameAction),
    /// Move the state machine to this mode.
    SetMode(UiMode),
    /// Move to this mode and remember the hand card that started it.
    SelectHandCard { instance_id: String, mode: UiMode },
    /// Back to `Idle`, clearing the selected hand card (TS `resetUI`).
    Reset,
}

/// TS `handleHandCardClick` — a *playable* hand card was clicked.
///
/// Non-playable cards open their detail instead; that branch lives in the hand
/// module because it needs the def id (`setUiMode({type:"cardDetail", …})`).
pub fn on_hand_card_click(card_type: CardType, instance_id: &str) -> UiCommand {
    let mode = match card_type {
        CardType::Character => UiMode::SelectingSlot {
            card_id: instance_id.to_string(),
        },
        CardType::Object => UiMode::SelectingEquipTarget {
            object_id: instance_id.to_string(),
        },
        CardType::Event => UiMode::ConfirmEvent {
            instance_id: instance_id.to_string(),
        },
        CardType::Ship => UiMode::ConfirmShip {
            instance_id: instance_id.to_string(),
        },
        // Counters are only playable from the counter window.
        CardType::Counter => return UiCommand::Ignore,
    };
    UiCommand::SelectHandCard {
        instance_id: instance_id.to_string(),
        mode,
    }
}

/// TS `handleSlotClick(slot, isPlayerSide)`.
///
/// `occupant` is the instance id sitting in `slot` (needed by the
/// `selectingTarget` branch, which attacks whatever stands there).
pub fn on_slot_click(
    mode: &UiMode,
    valid: &[GameAction],
    ai_player: PlayerId,
    slot: Slot,
    is_player_side: bool,
    occupant: Option<&str>,
) -> UiCommand {
    let deploy = deploy_slots(mode, valid);
    match mode {
        UiMode::SelectingSlot { card_id } if is_player_side && deploy.contains(&slot) => {
            UiCommand::Dispatch(GameAction::DeployCharacter {
                instance_id: card_id.clone(),
                slot,
            })
        }
        UiMode::SelectingCaptainSlot if is_player_side && deploy.contains(&slot) => {
            UiCommand::Dispatch(GameAction::FlipCaptain { slot })
        }
        // §8.29 — the free move lands on an empty slot of your own half.
        UiMode::SelectingMoveSlot { instance_id } if is_player_side && deploy.contains(&slot) => {
            UiCommand::Dispatch(GameAction::MoveCharacter {
                instance_id: instance_id.clone(),
                target_slot: slot,
            })
        }
        UiMode::SelectingTarget { attacker_id, kind } => {
            let targets = attack_targets(mode, valid, ai_player);
            match occupant {
                Some(target) if targets.contains(target) => {
                    // TS `handleSlotClick` had **no** captain branch: it always
                    // emitted baseAttack / specialAttack, carrying the
                    // synthetic `captain_<player>` id as `attackerInstanceId`,
                    // and the TS engine decoded that prefix. The Rust engine
                    // does not: `declare_base_attack_inner` and
                    // `declare_special_attack_inner` both look the attacker up
                    // in `state.cards` and refuse with `"Attacker not found"`
                    // before any prefix is read, so a captain aimed from a slot
                    // click would be a dead click plus a refusal notice.
                    // Every kind therefore goes through [`target_action`],
                    // which classifies the attacker exactly like
                    // `handleBoardCharClick` does.
                    UiCommand::Dispatch(target_action(attacker_id, kind, target, false))
                }
                _ => UiCommand::Ignore,
            }
        }
        _ => UiCommand::Ignore,
    }
}

/// TS `handleBoardCharClick(instanceId, isPlayerSide)`.
///
/// `def_id` is only used by the last branch (opening an enemy unit's detail).
pub fn on_board_char_click(
    mode: &UiMode,
    valid: &[GameAction],
    ai_player: PlayerId,
    instance_id: &str,
    is_player_side: bool,
    def_id: &str,
) -> UiCommand {
    if let UiMode::SelectingEquipTarget { object_id } = mode
        && is_player_side
        && equip_targets(mode, valid).contains(instance_id)
    {
        return UiCommand::Dispatch(GameAction::EquipObject {
            object_instance_id: object_id.clone(),
            target_instance_id: instance_id.to_string(),
            target_is_captain: None,
        });
    }
    if let UiMode::SelectingSupportTarget { instance_id: actor } = mode
        && support_targets(mode, valid).contains(instance_id)
    {
        return UiCommand::Dispatch(GameAction::BaseSupportAction {
            instance_id: actor.clone(),
            target_instance_id: Some(instance_id.to_string()),
        });
    }
    // Membership in the target set, not the side — see [`cell_highlight`]: an
    // ally-facing support special and a self-transformation both aim a unit on
    // the player's own half.
    if let UiMode::SelectingTarget { attacker_id, kind } = mode
        && attack_targets(mode, valid, ai_player).contains(instance_id)
    {
        return UiCommand::Dispatch(target_action(attacker_id, kind, instance_id, false));
    }
    if is_player_side {
        UiCommand::SetMode(UiMode::ActionMenu {
            instance_id: instance_id.to_string(),
        })
    } else {
        UiCommand::SetMode(UiMode::CardDetail {
            def_id: def_id.to_string(),
            instance_id: Some(instance_id.to_string()),
        })
    }
}

/// TS `onCaptainClick(playerId)` — attack targeting wins, otherwise the captain
/// menu opens (only from `idle`, like the TS).
pub fn on_captain_click(
    mode: &UiMode,
    valid: &[GameAction],
    ai_player: PlayerId,
    player_id: PlayerId,
) -> UiCommand {
    if let UiMode::SelectingTarget { attacker_id, kind } = mode {
        let key = captain_key(ai_player);
        if player_id == ai_player && attack_targets(mode, valid, ai_player).contains(&key) {
            return UiCommand::Dispatch(target_action(attacker_id, kind, &key, true));
        }
    }
    // Decision §8.28 (follow-up): the three signature SR Devil Fruits are worn
    // by the captain they name, so a click on your own captain while an object
    // is waiting for a bearer equips it — the captain counterpart of
    // `on_board_char_click`'s equip branch.
    if let UiMode::SelectingEquipTarget { object_id } = mode
        && player_id != ai_player
        && let Some(action) = valid.iter().find(|a| {
            matches!(
                a,
                GameAction::EquipObject { object_instance_id, target_is_captain: Some(true), .. }
                    if object_instance_id == object_id
            )
        })
    {
        return UiCommand::Dispatch(action.clone());
    }
    if mode.is_idle() {
        return UiCommand::SetMode(UiMode::CaptainMenu { player_id });
    }
    UiCommand::Ignore
}

/// TS `act()` inside `renderLine`: a click on a board cell goes to the deploy
/// branch when the cell is a lit deploy slot, otherwise to the occupant.
pub fn on_cell_click(
    mode: &UiMode,
    valid: &[GameAction],
    ai_player: PlayerId,
    slot: Slot,
    is_player_side: bool,
    occupant: Option<(&str, &str)>,
) -> UiCommand {
    let deploy = deploy_slots(mode, valid);
    let is_valid_deploy = is_player_side && deploy.contains(&slot);
    if is_valid_deploy {
        return on_slot_click(
            mode,
            valid,
            ai_player,
            slot,
            is_player_side,
            occupant.map(|(id, _)| id),
        );
    }
    match occupant {
        Some((instance_id, def_id)) => {
            on_board_char_click(mode, valid, ai_player, instance_id, is_player_side, def_id)
        }
        None => UiCommand::Ignore,
    }
}

/// TS `BoardSlot.onDrop` — the drop end of a hand-to-board drag.
///
/// ```tsx
/// onDragOver={e => e.preventDefault()}
/// onDrop={e => { e.preventDefault(); onDrop?.(); }}   // onDrop={act}
/// ```
///
/// `handleHandDragStart` already put the UI in `selectingSlot` /
/// `selectingEquipTarget`, so finishing the gesture is exactly the click path:
/// the same `act()` the pointer would have reached. Anything else is swallowed.
///
/// "Anything else" is narrower than "not selecting": a drop only ever finishes
/// the **two** modes a hand drag can arm. `selectingTarget` and its siblings
/// are reached by clicking the board, never by dragging, so forwarding a drop
/// while one of them is armed would fire an attack with a gesture the web
/// cannot even begin. (Whose card was released is checked one level up, by
/// [`HandDrag::carries`](crate::hand::HandDrag::carries) — this function only
/// sees the mode.)
pub fn on_cell_drop(
    mode: &UiMode,
    valid: &[GameAction],
    ai_player: PlayerId,
    slot: Slot,
    is_player_side: bool,
    occupant: Option<(&str, &str)>,
) -> UiCommand {
    if !matches!(
        mode,
        UiMode::SelectingSlot { .. } | UiMode::SelectingEquipTarget { .. }
    ) {
        return UiCommand::Ignore;
    }
    on_cell_click(mode, valid, ai_player, slot, is_player_side, occupant)
}

/// The captain end of the same gesture — TS has no equivalent because the DOM
/// captain card carried the very same `onDrop={act}` as a board slot.
///
/// §8.31 keeps the flipped captain out of `player.board`, so a drop on the cell
/// it occupies finds `occupant == None` and [`on_cell_click`] falls straight
/// through to [`UiCommand::Ignore`]. That silently killed the only way `MG-014`
/// *Gomu Gomu no Mi* is ever equipped: its `restriction: "Luffy"` matches no
/// card in the catalogue, only the captain `Monkey D. Luffy`, so the §8.28
/// follow-up `EquipObject { target_is_captain: Some(true) }` in
/// [`on_captain_click`] is its sole route — and `hand_drag_command` arms the
/// drag for Objects, so dragging the fruit out of the hand was a dead end onto
/// a cell the board had already lit with `Ring::Deploy`.
///
/// Same gate as [`on_cell_drop`]: a release only completes a gesture the hand
/// actually armed.
pub fn on_captain_drop(
    mode: &UiMode,
    valid: &[GameAction],
    ai_player: PlayerId,
    player_id: PlayerId,
) -> UiCommand {
    if !matches!(
        mode,
        UiMode::SelectingSlot { .. } | UiMode::SelectingEquipTarget { .. }
    ) {
        return UiCommand::Ignore;
    }
    on_captain_click(mode, valid, ai_player, player_id)
}

/// Build the right attack action for an attacker id (card **or** captain key)
/// — TS `handleBoardCharClick`, which branches on
/// `uiMode.attackerId.startsWith("captain_")`.
pub fn target_action(
    attacker_id: &str,
    kind: &AttackKind,
    target: &str,
    target_is_captain: bool,
) -> GameAction {
    let captain_flag = if target_is_captain { Some(true) } else { None };
    // The awakened fruit is its own action whoever wears it, so it is decided
    // before the attacker is classified (§8.28 follow-up × §8.48).
    if let AttackKind::Fruit { fruit_instance_id } = kind {
        return GameAction::FruitSpecialAttack {
            attacker_instance_id: attacker_id.to_string(),
            fruit_instance_id: fruit_instance_id.clone(),
            target_instance_id: target.to_string(),
            target_is_captain: captain_flag,
        };
    }
    if is_captain_key(attacker_id) {
        // §8.34(b): the surcharge is a variant of its own, not a flag on
        // `captainAttack`.
        if matches!(kind, AttackKind::Surcharge) {
            return GameAction::UseSurcharge {
                target_instance_id: target.to_string(),
                target_is_captain: captain_flag,
            };
        }
        return GameAction::CaptainAttack {
            target_instance_id: target.to_string(),
            target_is_captain: captain_flag,
            // The Rust engine enumerates the captain's ★ special on the same
            // `captainAttack` action with `isSpecial: true` (§8.2 item 34(a)) —
            // a rule the TS engine never had, and the reason this cannot simply
            // mirror `handleBoardCharClick`'s hard-coded base variant. The
            // CaptainMenu prices that ability and offers a button for it; the
            // flag is what makes the button reach the engine.
            is_special: kind.is_special().then_some(true),
        };
    }
    unit_attack_action(attacker_id, kind.is_special(), target, target_is_captain)
}

/// Build a `baseAttack` / `specialAttack` for `attacker_id` **whatever it is**
/// — TS `handleSlotClick`, which has no captain branch.
///
/// A `captain_<player>` id is a legal `attackerInstanceId`: the engine strips
/// the prefix in `get_attacker_owner` rather than looking the id up in
/// `state.cards`.
fn unit_attack_action(
    attacker_id: &str,
    is_special: bool,
    target: &str,
    target_is_captain: bool,
) -> GameAction {
    let captain_flag = if target_is_captain { Some(true) } else { None };
    if is_special {
        GameAction::SpecialAttack {
            attacker_instance_id: attacker_id.to_string(),
            target_instance_id: target.to_string(),
            target_is_captain: captain_flag,
        }
    } else {
        GameAction::BaseAttack {
            attacker_instance_id: attacker_id.to_string(),
            target_instance_id: target.to_string(),
            target_is_captain: captain_flag,
        }
    }
}

// ============================================================
// Status banner
// ============================================================

/// Colour role of the header status tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusTone {
    /// Green, no pulse — "Votre tour".
    Ready,
    /// Yellow, pulsing — the AI is thinking.
    Waiting,
    /// Red, pulsing — an attack is incoming.
    Danger,
    /// Red — pick an attack target.
    Target,
    /// Cyan — pick a support target.
    Support,
    /// Green — pick a deploy slot.
    Deploy,
    /// Amber — pick a captain slot / an equipment holder.
    Captain,
}

/// TS `statusText` — the header tag, in the same priority order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusHint {
    pub text: &'static str,
    /// A glyph drawn before the label, in the symbol face.
    ///
    /// Mock-up `<span class="you-turn">✦ À toi</span>`: the pill carries the
    /// mark, and it has to be a separate run because the label's family
    /// (Poppins) has no coverage past Latin-1.
    pub glyph: Option<&'static str>,
    pub tone: StatusTone,
    pub pulse: bool,
}

/// §8.38 — is the attack currently being aimed a **support** special?
///
/// A support special short-circuits into `resolve_support_special`: it never
/// builds a `PendingAttack`, deals no damage, and its legal target may well be
/// one of your own allies (`RH-009` Stimulant). Aiming it still runs through
/// [`UiMode::SelectingTarget`] — it is an attack *declaration* as far as the
/// action list is concerned — so this is what lets the header say "Cible du
/// pouvoir" in support cyan instead of "Choisissez une cible" in attack red.
///
/// Only [`AttackKind::Special`] can be support: the base action has its own
/// `baseSupportAction` route (and its own mode), the surcharge and the
/// awakened fruit's special always build a blow.
pub fn aim_is_support(mode: &UiMode, state: &GameState, registry: &CardRegistry) -> bool {
    let UiMode::SelectingTarget { attacker_id, kind } = mode else {
        return false;
    };
    if !matches!(kind, AttackKind::Special) {
        return false;
    }
    if let Some(owner) = captain_key_owner(attacker_id) {
        let captain = &state.player(owner).captain;
        return registry
            .captain_def(&captain.def_id)
            .is_some_and(|def| def.verso.special_attack.is_support.unwrap_or(false));
    }
    state
        .card(attacker_id)
        .and_then(|c| registry.card_def(&c.def_id))
        .and_then(|d| d.special_attack.as_ref())
        .is_some_and(|s| s.is_support.unwrap_or(false))
}

/// TS `statusText` IIFE.
///
/// `support_aim` is [`aim_is_support`], threaded in by the caller because the
/// mode alone cannot tell an attack from a §8.38 support special.
pub fn status_hint(
    mode: &UiMode,
    is_ai_turn: bool,
    in_counter_window: bool,
    support_aim: bool,
) -> StatusHint {
    let hint = |text, tone, pulse| StatusHint {
        text,
        glyph: None,
        tone,
        pulse,
    };
    if is_ai_turn {
        return hint("Tour de l'adversaire…", StatusTone::Waiting, true);
    }
    if in_counter_window {
        return hint("Réaction !", StatusTone::Danger, true);
    }
    match mode {
        // §8.38 — a support special is aimed through the same mode but deals
        // no damage, so it wears the support colour, ally target included.
        UiMode::SelectingTarget { .. } if support_aim => {
            hint("Cible du pouvoir", StatusTone::Support, true)
        }
        UiMode::SelectingTarget { .. } => hint("Choisissez une cible", StatusTone::Target, true),
        UiMode::SelectingSupportTarget { .. } => {
            hint("Cible du pouvoir", StatusTone::Support, true)
        }
        UiMode::SelectingSlot { .. } => hint("Choisissez un emplacement", StatusTone::Deploy, true),
        UiMode::SelectingCaptainSlot => hint("Placez le capitaine", StatusTone::Captain, true),
        // §8.29 — the free repositioning.
        UiMode::SelectingMoveSlot { .. } => {
            hint("Choisissez une case adjacente", StatusTone::Deploy, true)
        }
        UiMode::SelectingEquipTarget { .. } => {
            hint("Équipez un personnage", StatusTone::Captain, true)
        }
        // Mock-up `.you-turn`: "✦ À toi", glyph included.
        _ => StatusHint {
            text: "À toi",
            glyph: Some(crate::hand::card_face::READY_MARK),
            tone: StatusTone::Ready,
            pulse: false,
        },
    }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tcgop_engine::types::{CardType, Row};

    use crate::bridge::testkit::{advance, session as make_session};

    const AI: PlayerId = PlayerId::Player2;

    fn deploy(id: &str, slot: Slot) -> GameAction {
        GameAction::DeployCharacter {
            instance_id: id.into(),
            slot,
        }
    }

    fn base_attack(attacker: &str, target: &str, captain: bool) -> GameAction {
        GameAction::BaseAttack {
            attacker_instance_id: attacker.into(),
            target_instance_id: target.into(),
            target_is_captain: captain.then_some(true),
        }
    }

    fn special_attack(attacker: &str, target: &str) -> GameAction {
        GameAction::SpecialAttack {
            attacker_instance_id: attacker.into(),
            target_instance_id: target.into(),
            target_is_captain: None,
        }
    }

    // --- deploy_slots ---------------------------------------

    #[test]
    fn deploy_slots_only_lists_the_selected_card() {
        let valid = vec![
            deploy("a", Slot::V1),
            deploy("a", Slot::A2),
            deploy("b", Slot::V3),
        ];
        let mode = UiMode::SelectingSlot {
            card_id: "a".into(),
        };
        assert_eq!(
            deploy_slots(&mode, &valid),
            BTreeSet::from([Slot::V1, Slot::A2])
        );
    }

    #[test]
    fn deploy_slots_switches_to_flip_slots_for_the_captain() {
        let valid = vec![
            deploy("a", Slot::V1),
            GameAction::FlipCaptain { slot: Slot::V2 },
            GameAction::FlipCaptain { slot: Slot::A3 },
        ];
        assert_eq!(
            deploy_slots(&UiMode::SelectingCaptainSlot, &valid),
            BTreeSet::from([Slot::V2, Slot::A3])
        );
    }

    #[test]
    fn deploy_slots_is_empty_outside_the_two_placement_modes() {
        let valid = vec![
            deploy("a", Slot::V1),
            GameAction::FlipCaptain { slot: Slot::V2 },
        ];
        assert!(deploy_slots(&UiMode::Idle, &valid).is_empty());
        assert!(
            deploy_slots(
                &UiMode::ActionMenu {
                    instance_id: "a".into()
                },
                &valid
            )
            .is_empty()
        );
    }

    // --- equip / support ------------------------------------

    #[test]
    fn equip_targets_filter_on_the_selected_object() {
        let valid = vec![
            GameAction::EquipObject {
                object_instance_id: "obj".into(),
                target_instance_id: "zoro".into(),
                target_is_captain: None,
            },
            GameAction::EquipObject {
                object_instance_id: "other".into(),
                target_instance_id: "nami".into(),
                target_is_captain: None,
            },
        ];
        let mode = UiMode::SelectingEquipTarget {
            object_id: "obj".into(),
        };
        assert_eq!(
            equip_targets(&mode, &valid),
            BTreeSet::from(["zoro".to_string()])
        );
        assert!(equip_targets(&UiMode::Idle, &valid).is_empty());
    }

    #[test]
    fn support_targets_ignore_the_targetless_variant() {
        let valid = vec![
            GameAction::BaseSupportAction {
                instance_id: "nami".into(),
                target_instance_id: None,
            },
            GameAction::BaseSupportAction {
                instance_id: "nami".into(),
                target_instance_id: Some("zoro".into()),
            },
            GameAction::BaseSupportAction {
                instance_id: "brook".into(),
                target_instance_id: Some("sanji".into()),
            },
        ];
        let mode = UiMode::SelectingSupportTarget {
            instance_id: "nami".into(),
        };
        assert_eq!(
            support_targets(&mode, &valid),
            BTreeSet::from(["zoro".to_string()])
        );
        assert!(support_needs_target(&valid, "nami"));
        assert!(!support_needs_target(&valid, "usopp"));
    }

    // --- attack_targets -------------------------------------

    #[test]
    fn attack_targets_split_base_and_special() {
        let valid = vec![
            base_attack("zoro", "coby", false),
            special_attack("zoro", "smoker"),
            base_attack("sanji", "garp", false),
        ];
        let base = UiMode::SelectingTarget {
            attacker_id: "zoro".into(),
            kind: AttackKind::Base,
        };
        let special = UiMode::SelectingTarget {
            attacker_id: "zoro".into(),
            kind: AttackKind::Special,
        };
        assert_eq!(
            attack_targets(&base, &valid, AI),
            BTreeSet::from(["coby".to_string()])
        );
        assert_eq!(
            attack_targets(&special, &valid, AI),
            BTreeSet::from(["smoker".to_string()])
        );
    }

    #[test]
    fn attack_targets_map_the_enemy_captain_to_its_key() {
        let valid = vec![base_attack("zoro", "captain_player2", true)];
        let mode = UiMode::SelectingTarget {
            attacker_id: "zoro".into(),
            kind: AttackKind::Base,
        };
        assert_eq!(
            attack_targets(&mode, &valid, AI),
            BTreeSet::from([captain_key(AI)])
        );
    }

    /// The engine offers the captain's base and ★ attacks as the *same* action
    /// with different `isSpecial`, and only offers the ★ one while it is
    /// affordable and unused — so the two have genuinely different target
    /// lists and the mode's flag has to narrow them.
    #[test]
    fn a_captain_aims_the_ability_that_was_armed() {
        let valid = vec![
            GameAction::CaptainAttack {
                target_instance_id: "coby".into(),
                target_is_captain: None,
                is_special: None,
            },
            GameAction::CaptainAttack {
                target_instance_id: "captain_player2".into(),
                target_is_captain: Some(true),
                is_special: Some(true),
            },
            base_attack("zoro", "smoker", false),
        ];
        let aiming = |kind| UiMode::SelectingTarget {
            attacker_id: captain_key(PlayerId::Player1),
            kind,
        };
        assert_eq!(
            attack_targets(&aiming(AttackKind::Base), &valid, AI),
            BTreeSet::from(["coby".to_string()]),
            "the base attack only reaches what the base attack was offered on"
        );
        assert_eq!(
            attack_targets(&aiming(AttackKind::Special), &valid, AI),
            BTreeSet::from([captain_key(AI)]),
            "the ★ special only reaches its own targets"
        );
    }

    /// …and picking one of those targets dispatches `captainAttack` with the
    /// flag set, which is the only path the engine's §8.2 item 34(a) special
    /// attack has to the board.
    #[test]
    fn a_captain_special_attack_carries_its_flag() {
        let valid = vec![GameAction::CaptainAttack {
            target_instance_id: "coby".into(),
            target_is_captain: None,
            is_special: Some(true),
        }];
        let mode = UiMode::SelectingTarget {
            attacker_id: captain_key(PlayerId::Player1),
            kind: AttackKind::Special,
        };
        assert_eq!(
            on_board_char_click(&mode, &valid, AI, "coby", false, "MG-002"),
            UiCommand::Dispatch(GameAction::CaptainAttack {
                target_instance_id: "coby".into(),
                target_is_captain: None,
                is_special: Some(true),
            })
        );

        // The base attack keeps answering with the base variant.
        let base = vec![GameAction::CaptainAttack {
            target_instance_id: "coby".into(),
            target_is_captain: None,
            is_special: None,
        }];
        let mode = UiMode::SelectingTarget {
            attacker_id: captain_key(PlayerId::Player1),
            kind: AttackKind::Base,
        };
        assert_eq!(
            on_board_char_click(&mode, &base, AI, "coby", false, "MG-002"),
            UiCommand::Dispatch(GameAction::CaptainAttack {
                target_instance_id: "coby".into(),
                target_is_captain: None,
                is_special: None,
            })
        );
    }

    /// A drop only ever finishes a gesture a hand drag can start. Forwarding
    /// it while a *targeting* mode is armed would fire an attack with a
    /// gesture the web cannot even begin (board tiles carry no `draggable`).
    #[test]
    fn a_drop_only_finishes_the_two_modes_a_drag_can_arm() {
        let valid = vec![deploy("c1", Slot::V1), base_attack("zoro", "smoker", false)];
        let deploying = UiMode::SelectingSlot {
            card_id: "c1".into(),
        };
        assert_eq!(
            on_cell_drop(&deploying, &valid, AI, Slot::V1, true, None),
            UiCommand::Dispatch(deploy("c1", Slot::V1))
        );

        for mode in [
            UiMode::SelectingTarget {
                attacker_id: "zoro".into(),
                kind: AttackKind::Base,
            },
            UiMode::SelectingSupportTarget {
                instance_id: "zoro".into(),
            },
            UiMode::SelectingCaptainSlot,
            UiMode::Idle,
        ] {
            assert_eq!(
                on_cell_drop(
                    &mode,
                    &valid,
                    AI,
                    Slot::V1,
                    false,
                    Some(("smoker", "MG-002"))
                ),
                UiCommand::Ignore,
                "{mode:?} must not be completable by a drop"
            );
        }
    }

    /// Mock-up `<span class="you-turn">✦ À toi</span>`.
    #[test]
    fn the_ready_pill_is_the_mock_ups() {
        let hint = status_hint(&UiMode::Idle, false, false, false);
        assert_eq!(hint.text, "À toi");
        assert_eq!(hint.glyph, Some("\u{2726}"));
        assert_eq!(hint.tone, StatusTone::Ready);
        assert!(!hint.pulse);
        // Every other state keeps its bare label.
        assert_eq!(status_hint(&UiMode::Idle, true, false, false).glyph, None);
    }

    // --- highlighting ---------------------------------------

    #[test]
    fn cell_highlight_dims_every_non_eligible_cell_while_selecting() {
        let valid = vec![deploy("a", Slot::V1)];
        let mode = UiMode::SelectingSlot {
            card_id: "a".into(),
        };
        let deploy_set = deploy_slots(&mode, &valid);
        let empty_str: BTreeSet<String> = BTreeSet::new();

        let lit = cell_highlight(
            &mode,
            Slot::V1,
            true,
            None,
            &deploy_set,
            &empty_str,
            &empty_str,
            &empty_str,
            false,
        );
        assert!(lit.valid_deploy && !lit.dimmed && lit.eligible());

        let dark = cell_highlight(
            &mode,
            Slot::V2,
            true,
            None,
            &deploy_set,
            &empty_str,
            &empty_str,
            &empty_str,
            false,
        );
        assert!(!dark.eligible() && dark.dimmed);
    }

    #[test]
    fn zone_attacks_flag_the_enemy_front_line_as_impact() {
        let valid = vec![
            base_attack("zoro", "coby", false),
            base_attack("zoro", "garp", false),
        ];
        let mode = UiMode::SelectingTarget {
            attacker_id: "zoro".into(),
            kind: AttackKind::Base,
        };
        let attack = attack_targets(&mode, &valid, AI);
        let none: BTreeSet<String> = BTreeSet::new();
        let none_slot: BTreeSet<Slot> = BTreeSet::new();

        let front = cell_highlight(
            &mode,
            Slot::V1,
            false,
            Some("coby"),
            &none_slot,
            &attack,
            &none,
            &none,
            true,
        );
        assert!(front.impact, "front line of a zone attack splashes");

        let back = cell_highlight(
            &mode,
            Slot::A1,
            false,
            Some("garp"),
            &none_slot,
            &attack,
            &none,
            &none,
            true,
        );
        assert!(!back.impact, "back line never splashes");
        assert_eq!(Slot::V1.row(), Row::Front);
    }

    // --- click routing --------------------------------------

    #[test]
    fn hand_clicks_enter_the_mode_matching_the_card_type() {
        assert_eq!(
            on_hand_card_click(CardType::Character, "c1"),
            UiCommand::SelectHandCard {
                instance_id: "c1".into(),
                mode: UiMode::SelectingSlot {
                    card_id: "c1".into()
                }
            }
        );
        assert!(matches!(
            on_hand_card_click(CardType::Object, "o1"),
            UiCommand::SelectHandCard {
                mode: UiMode::SelectingEquipTarget { .. },
                ..
            }
        ));
        assert!(matches!(
            on_hand_card_click(CardType::Event, "e1"),
            UiCommand::SelectHandCard {
                mode: UiMode::ConfirmEvent { .. },
                ..
            }
        ));
        assert!(matches!(
            on_hand_card_click(CardType::Ship, "s1"),
            UiCommand::SelectHandCard {
                mode: UiMode::ConfirmShip { .. },
                ..
            }
        ));
        assert_eq!(
            on_hand_card_click(CardType::Counter, "x"),
            UiCommand::Ignore
        );
    }

    #[test]
    fn clicking_a_lit_slot_deploys_and_a_dark_one_does_nothing() {
        let valid = vec![deploy("c1", Slot::V1)];
        let mode = UiMode::SelectingSlot {
            card_id: "c1".into(),
        };
        assert_eq!(
            on_slot_click(&mode, &valid, AI, Slot::V1, true, None),
            UiCommand::Dispatch(deploy("c1", Slot::V1))
        );
        assert_eq!(
            on_slot_click(&mode, &valid, AI, Slot::V2, true, None),
            UiCommand::Ignore
        );
        assert_eq!(
            on_slot_click(&mode, &valid, AI, Slot::V1, false, None),
            UiCommand::Ignore,
            "the enemy half is never a deploy target"
        );
    }

    #[test]
    fn clicking_a_lit_enemy_slot_attacks_its_occupant() {
        let valid = vec![base_attack("zoro", "coby", false)];
        let mode = UiMode::SelectingTarget {
            attacker_id: "zoro".into(),
            kind: AttackKind::Base,
        };
        assert_eq!(
            on_slot_click(&mode, &valid, AI, Slot::V1, false, Some("coby")),
            UiCommand::Dispatch(base_attack("zoro", "coby", false))
        );
        assert_eq!(
            on_slot_click(&mode, &valid, AI, Slot::V2, false, Some("smoker")),
            UiCommand::Ignore
        );
    }

    #[test]
    fn captain_flip_dispatches_from_the_captain_slot_mode() {
        let valid = vec![GameAction::FlipCaptain { slot: Slot::V2 }];
        assert_eq!(
            on_slot_click(
                &UiMode::SelectingCaptainSlot,
                &valid,
                AI,
                Slot::V2,
                true,
                None
            ),
            UiCommand::Dispatch(GameAction::FlipCaptain { slot: Slot::V2 })
        );
    }

    #[test]
    fn board_clicks_fall_back_to_menus() {
        let valid: Vec<GameAction> = Vec::new();
        assert_eq!(
            on_board_char_click(&UiMode::Idle, &valid, AI, "zoro", true, "MG-001"),
            UiCommand::SetMode(UiMode::ActionMenu {
                instance_id: "zoro".into()
            })
        );
        assert_eq!(
            on_board_char_click(&UiMode::Idle, &valid, AI, "coby", false, "MR-001"),
            UiCommand::SetMode(UiMode::CardDetail {
                def_id: "MR-001".into(),
                instance_id: Some("coby".into())
            })
        );
    }

    #[test]
    fn equipping_and_supporting_take_priority_over_the_menus() {
        let equip = vec![GameAction::EquipObject {
            object_instance_id: "wado".into(),
            target_instance_id: "zoro".into(),
            target_is_captain: None,
        }];
        let mode = UiMode::SelectingEquipTarget {
            object_id: "wado".into(),
        };
        assert_eq!(
            on_board_char_click(&mode, &equip, AI, "zoro", true, "MG-001"),
            UiCommand::Dispatch(equip[0].clone())
        );

        let support = vec![GameAction::BaseSupportAction {
            instance_id: "chopper".into(),
            target_instance_id: Some("zoro".into()),
        }];
        let mode = UiMode::SelectingSupportTarget {
            instance_id: "chopper".into(),
        };
        assert_eq!(
            on_board_char_click(&mode, &support, AI, "zoro", true, "MG-001"),
            UiCommand::Dispatch(support[0].clone())
        );
    }

    /// Decision §8.28 (follow-up) — the captain wears the fruit printed for it,
    /// so it is an equip target: clicking your own captain while an object is
    /// waiting for a bearer dispatches the equip, and it lights up like one.
    #[test]
    fn your_captain_is_an_equip_target() {
        let human = PlayerId::Player1;
        let key = captain_key(human);
        let equip = GameAction::EquipObject {
            object_instance_id: "gomu".into(),
            target_instance_id: key.clone(),
            target_is_captain: Some(true),
        };
        let valid = vec![equip.clone()];
        let mode = UiMode::SelectingEquipTarget {
            object_id: "gomu".into(),
        };

        assert_eq!(equip_targets(&mode, &valid), BTreeSet::from([key.clone()]));
        assert_eq!(
            on_captain_click(&mode, &valid, AI, human),
            UiCommand::Dispatch(equip)
        );
        // The foe's captain is never a bearer of *your* object.
        assert_eq!(on_captain_click(&mode, &valid, AI, AI), UiCommand::Ignore);
    }

    /// …and the **drag** has to land there too. §8.31 keeps the flipped
    /// captain out of `player.board`, so a drop on the cell it occupies finds
    /// no occupant and `on_cell_drop` swallows the release — which made
    /// `MG-014` *Gomu Gomu no Mi* undroppable, and it has no other route:
    /// its `restriction: "Luffy"` matches no card in the catalogue, only the
    /// captain `Monkey D. Luffy`.
    #[test]
    fn your_captain_accepts_the_equip_as_a_drop_too() {
        let human = PlayerId::Player1;
        let key = captain_key(human);
        let equip = GameAction::EquipObject {
            object_instance_id: "gomu".into(),
            target_instance_id: key.clone(),
            target_is_captain: Some(true),
        };
        let valid = vec![equip.clone()];
        let mode = UiMode::SelectingEquipTarget {
            object_id: "gomu".into(),
        };

        assert_eq!(
            on_captain_drop(&mode, &valid, AI, human),
            UiCommand::Dispatch(equip),
            "the gesture the board lights with `Ring::Deploy` must complete"
        );
        // Same gate as `on_cell_drop`: a release no hand drag armed is inert.
        assert_eq!(
            on_captain_drop(&UiMode::Idle, &valid, AI, human),
            UiCommand::Ignore
        );
        // And the foe's captain is no more a bearer on drop than on click.
        assert_eq!(on_captain_drop(&mode, &valid, AI, AI), UiCommand::Ignore);
    }

    #[test]
    fn captain_clicks_attack_then_open_the_menu() {
        let key = captain_key(AI);
        let valid = vec![base_attack("zoro", &key, true)];
        let mode = UiMode::SelectingTarget {
            attacker_id: "zoro".into(),
            kind: AttackKind::Base,
        };
        assert_eq!(
            on_captain_click(&mode, &valid, AI, AI),
            UiCommand::Dispatch(base_attack("zoro", &key, true))
        );
        // Own captain, same mode → not a target, and not idle → nothing.
        assert_eq!(
            on_captain_click(&mode, &valid, AI, PlayerId::Player1),
            UiCommand::Ignore
        );
        assert_eq!(
            on_captain_click(&UiMode::Idle, &valid, AI, PlayerId::Player1),
            UiCommand::SetMode(UiMode::CaptainMenu {
                player_id: PlayerId::Player1
            })
        );
    }

    #[test]
    fn a_captain_attacker_emits_a_captain_attack() {
        let key = captain_key(AI);
        let valid = vec![GameAction::CaptainAttack {
            target_instance_id: key.clone(),
            target_is_captain: Some(true),
            is_special: None,
        }];
        let mode = UiMode::SelectingTarget {
            attacker_id: captain_key(PlayerId::Player1),
            kind: AttackKind::Base,
        };
        assert_eq!(
            on_captain_click(&mode, &valid, AI, AI),
            UiCommand::Dispatch(GameAction::CaptainAttack {
                target_instance_id: key,
                target_is_captain: Some(true),
                is_special: None,
            })
        );
    }

    #[test]
    fn cell_clicks_route_deploy_before_occupant() {
        let valid = vec![deploy("c1", Slot::V1)];
        let mode = UiMode::SelectingSlot {
            card_id: "c1".into(),
        };
        // Lit deploy slot wins even when something stands there.
        assert_eq!(
            on_cell_click(&mode, &valid, AI, Slot::V1, true, None),
            UiCommand::Dispatch(deploy("c1", Slot::V1))
        );
        // Not lit → occupant handling (action menu).
        assert_eq!(
            on_cell_click(&mode, &valid, AI, Slot::V2, true, Some(("zoro", "MG-001"))),
            UiCommand::SetMode(UiMode::ActionMenu {
                instance_id: "zoro".into()
            })
        );
        // Not lit, empty → nothing.
        assert_eq!(
            on_cell_click(&mode, &valid, AI, Slot::V3, true, None),
            UiCommand::Ignore
        );
    }

    // --- modes / status -------------------------------------

    #[test]
    fn selecting_and_modal_partition_the_modes() {
        assert!(UiMode::SelectingCaptainSlot.is_selecting());
        assert!(!UiMode::SelectingCaptainSlot.is_modal());
        assert!(
            UiMode::CaptainMenu {
                player_id: PlayerId::Player1
            }
            .is_modal()
        );
        assert!(UiMode::default().is_idle());
    }

    #[test]
    fn status_hint_follows_the_ts_priority() {
        let target = UiMode::SelectingTarget {
            attacker_id: "zoro".into(),
            kind: AttackKind::Base,
        };
        assert_eq!(
            status_hint(&target, true, false, false).tone,
            StatusTone::Waiting
        );
        assert_eq!(
            status_hint(&target, false, true, false).tone,
            StatusTone::Danger
        );
        assert_eq!(
            status_hint(&target, false, false, false).tone,
            StatusTone::Target
        );
        // §8.38 — the same mode, aiming a support special: cyan, not red.
        assert_eq!(
            status_hint(&target, false, false, true).tone,
            StatusTone::Support
        );
        assert_eq!(
            status_hint(&target, false, false, true).text,
            "Cible du pouvoir"
        );
        assert_eq!(
            status_hint(&UiMode::Idle, false, false, false).tone,
            StatusTone::Ready
        );
        assert!(!status_hint(&UiMode::Idle, false, false, false).pulse);
    }

    // --- against a real engine session -----------------------

    #[test]
    fn a_real_session_lights_deploy_slots_for_a_hand_character() {
        let mut session = make_session(7);

        // Turn 1 only affords `endTurn`; play on until a deploy is legal.
        assert!(
            advance(&mut session, 60, |s| s
                .valid
                .iter()
                .any(|a| matches!(a, GameAction::DeployCharacter { .. }))),
            "a deploy must become legal within 60 actions"
        );

        let card_id = session
            .valid
            .iter()
            .find_map(|a| match a {
                GameAction::DeployCharacter { instance_id, .. } => Some(instance_id.clone()),
                _ => None,
            })
            .expect("the loop above stopped on a deploy");

        let mode = UiMode::SelectingSlot {
            card_id: card_id.clone(),
        };
        let slots = deploy_slots(&mode, &session.valid);
        assert!(!slots.is_empty(), "a deployable card must light some slots");
        assert!(hand_card_playable(&session.valid, &card_id));
        assert!(!hand_card_playable(&session.valid, "not-a-card"));

        // Nothing is aimed, so no zone preview.
        assert!(!attack_is_zone(
            &UiMode::Idle,
            &session.state,
            &session.registry
        ));
    }

    #[test]
    fn attack_is_zone_reads_the_defs_attack_traits() {
        let session = make_session(7);

        // An unknown attacker never lights the board up.
        let bogus = UiMode::SelectingTarget {
            attacker_id: "ghost".into(),
            kind: AttackKind::Special,
        };
        assert!(!attack_is_zone(&bogus, &session.state, &session.registry));

        // A captain key resolves against the *verso* face and must not panic.
        let cap = UiMode::SelectingTarget {
            attacker_id: captain_key(PlayerId::Player1),
            kind: AttackKind::Special,
        };
        let _ = attack_is_zone(&cap, &session.state, &session.registry);
    }

    // --- drag & drop ------------------------------------------

    /// Dropping a dragged character on a lit slot **dispatches**, exactly like
    /// clicking it: the drag is only the way the mode was armed.
    #[test]
    fn dropping_a_character_on_a_lit_slot_deploys_it() {
        let valid = vec![GameAction::DeployCharacter {
            instance_id: "c1".into(),
            slot: Slot::V2,
        }];
        let mode = UiMode::SelectingSlot {
            card_id: "c1".into(),
        };
        assert_eq!(
            on_cell_drop(&mode, &valid, PlayerId::Player2, Slot::V2, true, None),
            UiCommand::Dispatch(GameAction::DeployCharacter {
                instance_id: "c1".into(),
                slot: Slot::V2,
            })
        );
    }

    /// …and on an equip holder.
    #[test]
    fn dropping_an_object_on_a_holder_equips_it() {
        let valid = vec![GameAction::EquipObject {
            object_instance_id: "o1".into(),
            target_instance_id: "u1".into(),
            target_is_captain: None,
        }];
        let mode = UiMode::SelectingEquipTarget {
            object_id: "o1".into(),
        };
        assert_eq!(
            on_cell_drop(
                &mode,
                &valid,
                PlayerId::Player2,
                Slot::A1,
                true,
                Some(("u1", "MG-001"))
            ),
            UiCommand::Dispatch(GameAction::EquipObject {
                object_instance_id: "o1".into(),
                target_instance_id: "u1".into(),
                target_is_captain: None,
            })
        );
    }

    /// A drop with nothing armed, or onto an illegal cell, is swallowed — it
    /// must never open a menu the player did not ask for.
    #[test]
    fn a_drop_with_nothing_armed_is_swallowed() {
        let valid = vec![GameAction::DeployCharacter {
            instance_id: "c1".into(),
            slot: Slot::V2,
        }];
        assert_eq!(
            on_cell_drop(
                &UiMode::Idle,
                &valid,
                PlayerId::Player2,
                Slot::V2,
                true,
                None
            ),
            UiCommand::Ignore
        );
        let mode = UiMode::SelectingSlot {
            card_id: "c1".into(),
        };
        assert_eq!(
            on_cell_drop(&mode, &valid, PlayerId::Player2, Slot::V3, true, None),
            UiCommand::Ignore,
            "V3 is not a legal slot for c1"
        );
    }

    // --- attack routing ---------------------------------------

    /// Both handlers build the action the **Rust** engine accepts for a
    /// captain attacker: `captainAttack`. The TS client rode the synthetic
    /// `captain_<player>` id inside a plain `baseAttack` and let the engine
    /// decode the prefix; `declare_base_attack_inner` refuses that outright
    /// (`"Attacker not found"`), so the slot handler classifies the attacker
    /// through [`target_action`] exactly like `handleBoardCharClick`.
    #[test]
    fn the_two_handlers_build_the_captain_attack_the_engine_accepts() {
        let key = captain_key(PlayerId::Player1);
        // What the engine actually offers when a captain is the attacker.
        let valid = vec![GameAction::CaptainAttack {
            target_instance_id: "foe1".into(),
            target_is_captain: None,
            is_special: None,
        }];
        let mode = UiMode::SelectingTarget {
            attacker_id: key,
            kind: AttackKind::Base,
        };

        // Slot handler → captainAttack, not a baseAttack carrying the
        // synthetic id (which the engine refuses before decoding the prefix).
        assert_eq!(
            on_slot_click(
                &mode,
                &valid,
                PlayerId::Player2,
                Slot::V1,
                false,
                Some("foe1")
            ),
            UiCommand::Dispatch(GameAction::CaptainAttack {
                target_instance_id: "foe1".into(),
                target_is_captain: None,
                is_special: None,
            })
        );

        // Board-character handler → captainAttack.
        assert_eq!(
            on_board_char_click(&mode, &valid, PlayerId::Player2, "foe1", false, "MG-001"),
            UiCommand::Dispatch(GameAction::CaptainAttack {
                target_instance_id: "foe1".into(),
                target_is_captain: None,
                is_special: None,
            })
        );
    }
    // ========================================================
    // Every action the engine can offer is reachable — one test
    // per declaration the TS client never had (§8.2 items 28,
    // 29, 34, 47, 48).
    //
    // Shape of each: rig a seeded session so `Session::valid`
    // actually contains the action, walk the very path a click
    // takes (menu view → `UiCommand` → `apply_ui_command` →
    // click handler), and assert the dispatched `GameAction` is
    // **one the engine offered**, byte for byte.
    // ========================================================

    mod reachable {
        use super::*;
        use std::sync::Arc;
        use tcgop_engine::state::CardInstance;
        use tcgop_engine::types::{SpecialAttack, StatusEffectType, Zone};

        use crate::board::interaction::apply_ui_command;
        use crate::bridge::Session;
        use crate::panels::model::{action_menu_view, captain_menu_view};

        /// A fresh seeded game, parked on the human's turn in the main phase.
        fn table(seed: u64) -> Session {
            let mut session = make_session(seed);
            let human = session.human;
            assert!(
                advance(&mut session, 200, |s: &Session| {
                    s.state.current_player == human && s.state.pending_attack.is_none()
                }),
                "the human never got the turn"
            );
            session.state.players.get_mut(human).volonte = 10;
            session
        }

        /// Drop a definition straight onto `owner`'s board, deployed long
        /// enough ago to have no summoning sickness.
        fn put(session: &mut Session, def_id: &str, owner: PlayerId, slot: Slot) -> String {
            let pv = session
                .registry
                .card_def(def_id)
                .and_then(|d| d.pv)
                .unwrap_or(1);
            let id = session.ctx.generate_instance_id(def_id);
            let mut instance = CardInstance::new(id.clone(), def_id.to_string(), owner, pv);
            instance.zone = Zone::Board;
            instance.slot = Some(slot);
            instance.deployed_turn = Some(0);
            session.state.cards.insert(id.clone(), instance);
            *session.state.players.get_mut(owner).board.slot_mut(slot) = Some(id.clone());
            id
        }

        /// Attach `def_id` (an object) to `bearer`, awakened or not.
        fn attach(session: &mut Session, def_id: &str, bearer: &str, awakened: bool) -> String {
            let owner = session.state.card(bearer).expect("bearer on board").owner;
            let id = session.ctx.generate_instance_id(def_id);
            let mut object = CardInstance::new(id.clone(), def_id.to_string(), owner, 0);
            object.zone = Zone::Board;
            object.is_awakened = awakened.then_some(true);
            session.state.cards.insert(id.clone(), object);
            session
                .state
                .card_mut(bearer)
                .expect("bearer on board")
                .attached_objects
                .push(id.clone());
            id
        }

        /// Engage the human's captain in `slot` without paying for it, so the
        /// verso powers are on the table whatever the flip condition says.
        fn engage(session: &mut Session, slot: Slot) {
            let human = session.human;
            let captain = &mut session.state.players.get_mut(human).captain;
            captain.flipped = true;
            captain.slot = Some(slot);
            captain.deployed_turn = Some(0);
            captain.tapped = false;
            captain.used_base_action = false;
            captain.used_special_attack = false;
        }

        /// Run `command` through the real reducer and return the action it
        /// asked the bridge to dispatch (`None` for a mode change).
        fn run(mode: &mut UiMode, command: UiCommand) -> Option<GameAction> {
            let mut selected = SelectedHandCard(None);
            apply_ui_command(command, mode, &mut selected)
        }

        /// The dispatched action must be one the engine is offering.
        fn assert_offered(session: &Session, action: &GameAction) {
            assert!(
                session.valid.contains(action),
                "the UI produced an action the engine never offered: {action:?}"
            );
        }

        // --- §8.29 `moveCharacter` -------------------------------

        #[test]
        fn the_action_menu_repositions_a_unit_with_the_engines_own_move() {
            let mut session = table(4);
            let human = session.human;
            let zoro = put(&mut session, "MG-002", human, Slot::V2);
            session.refresh_valid();

            let offered: Vec<GameAction> = session
                .valid
                .iter()
                .filter(|a| matches!(a, GameAction::MoveCharacter { instance_id, .. } if *instance_id == zoro))
                .cloned()
                .collect();
            assert!(!offered.is_empty(), "the engine offers no free move");

            let view = action_menu_view(&session.state, &session.registry, &session.valid, &zoro)
                .expect("the unit has an action menu");
            let free_move = view.free_move.expect("the menu carries a *Déplacer* row");
            assert!(!free_move.disabled, "the row is live: {free_move:?}");

            // The button arms the slot picker…
            let mut mode = UiMode::ActionMenu {
                instance_id: zoro.clone(),
            };
            assert_eq!(run(&mut mode, free_move.command), None);
            assert_eq!(
                mode,
                UiMode::SelectingMoveSlot {
                    instance_id: zoro.clone()
                }
            );

            // …which lights exactly the slots the engine offered…
            let lit = deploy_slots(&mode, &session.valid);
            let expected: BTreeSet<Slot> = offered
                .iter()
                .map(|a| match a {
                    GameAction::MoveCharacter { target_slot, .. } => *target_slot,
                    _ => unreachable!(),
                })
                .collect();
            assert_eq!(lit, expected);
            assert_eq!(move_slots(&session.valid, &zoro), expected);

            // …and a click on one of them dispatches that very action.
            let slot = *lit.iter().next().unwrap();
            let command =
                on_slot_click(&mode, &session.valid, session.ai_player(), slot, true, None);
            let action = run(&mut mode, command).expect("the click dispatches");
            assert_eq!(
                action,
                GameAction::MoveCharacter {
                    instance_id: zoro,
                    target_slot: slot
                }
            );
            assert_offered(&session, &action);
            assert!(mode.is_idle(), "a dispatch resets the UI");
        }

        // --- §8.47/§8.48 `awakenFruit` ---------------------------

        #[test]
        fn the_action_menu_awakens_the_fruit_the_engine_offers() {
            let mut session = table(11);
            let human = session.human;
            session.state.turn_number = 6; // `MG-015` awakens from turn 5.
            let robin = put(&mut session, "MG-006", human, Slot::A1);
            let fruit = attach(&mut session, "MG-015", &robin, false);
            session.refresh_valid();

            let expected = GameAction::AwakenFruit {
                fruit_instance_id: fruit.clone(),
            };
            assert!(
                session.valid.contains(&expected),
                "the engine does not offer the awakening: {:?}",
                session.valid
            );

            let view = action_menu_view(&session.state, &session.registry, &session.valid, &robin)
                .expect("the bearer has an action menu");
            assert_eq!(view.awakenings.len(), 1, "one fruit, one *Éveiller* row");
            let awaken = view.awakenings[0].clone();
            assert!(!awaken.disabled, "the row is live: {awaken:?}");
            assert_eq!(awaken.cost, 2, "the printed `volCost`");

            let mut mode = UiMode::ActionMenu {
                instance_id: robin.clone(),
            };
            let action = run(&mut mode, awaken.command).expect("the button dispatches");
            assert_eq!(action, expected);
            assert_offered(&session, &action);
        }

        // --- §8.28 follow-up × §8.48 `fruitSpecialAttack` --------

        #[test]
        fn the_action_menu_fires_an_awakened_fruits_own_special() {
            let mut session = table(13);
            let human = session.human;
            let ai = session.ai_player();
            session.state.turn_number = 6;
            let robin = put(&mut session, "MG-006", human, Slot::V1);
            let fruit = attach(&mut session, "MG-015", &robin, true);
            let prey = put(&mut session, "MR-001", ai, Slot::V1);
            session.refresh_valid();

            assert!(
                session.valid.iter().any(|a| matches!(
                    a,
                    GameAction::FruitSpecialAttack { fruit_instance_id, .. }
                        if *fruit_instance_id == fruit
                )),
                "the engine does not offer the awakened special"
            );

            let view = action_menu_view(&session.state, &session.registry, &session.valid, &robin)
                .expect("the bearer has an action menu");
            assert_eq!(view.fruit_specials.len(), 1, "one awakened fruit, one row");
            let row = view.fruit_specials[0].clone();
            assert_eq!(row.name, "Gigante Fleur");
            assert!(!row.disabled, "the row is live: {row:?}");

            // The row arms a targeting mode that knows *which* fruit is aimed.
            let mut mode = UiMode::ActionMenu {
                instance_id: robin.clone(),
            };
            assert_eq!(run(&mut mode, row.command), None);
            assert_eq!(
                mode,
                UiMode::SelectingTarget {
                    attacker_id: robin.clone(),
                    kind: AttackKind::Fruit {
                        fruit_instance_id: fruit.clone()
                    }
                }
            );
            assert!(attack_targets(&mode, &session.valid, ai).contains(&prey));

            let command = on_board_char_click(&mode, &session.valid, ai, &prey, false, "MR-001");
            let action = run(&mut mode, command).expect("the click dispatches");
            assert_eq!(
                action,
                GameAction::FruitSpecialAttack {
                    attacker_instance_id: robin,
                    fruit_instance_id: fruit,
                    target_instance_id: prey,
                    target_is_captain: None,
                }
            );
            assert_offered(&session, &action);
        }

        // --- §8.34(a) the captain's ★ special --------------------

        #[test]
        fn the_captain_menu_fires_the_versos_special_attack() {
            let mut session = table(17);
            let human = session.human;
            let ai = session.ai_player();
            engage(&mut session, Slot::V2);
            let prey = put(&mut session, "MR-001", ai, Slot::V1);
            session.refresh_valid();

            assert!(
                session.valid.iter().any(|a| matches!(
                    a,
                    GameAction::CaptainAttack {
                        is_special: Some(true),
                        ..
                    }
                )),
                "the engine does not offer the captain special"
            );

            let view = captain_menu_view(
                &session.state,
                &session.registry,
                &session.valid,
                human,
                human,
            )
            .expect("the captain has a menu");
            assert!(view.show_special_attack && view.can_special_attack);
            assert_eq!(view.special_attack_reason, None);

            let mut mode = UiMode::CaptainMenu { player_id: human };
            assert_eq!(run(&mut mode, view.special_attack_command), None);
            assert_eq!(
                mode,
                UiMode::SelectingTarget {
                    attacker_id: captain_key(human),
                    kind: AttackKind::Special
                }
            );

            let command = on_board_char_click(&mode, &session.valid, ai, &prey, false, "MR-001");
            let action = run(&mut mode, command).expect("the click dispatches");
            assert_eq!(
                action,
                GameAction::CaptainAttack {
                    target_instance_id: prey,
                    target_is_captain: None,
                    is_special: Some(true),
                }
            );
            assert_offered(&session, &action);
        }

        // --- §8.34(b) `useSurcharge` -----------------------------

        /// The shipped catalogue prints no `surcharge` at all (the decision
        /// says so in as many words), so the only honest way to prove the UI
        /// reaches the action is to hand the session a captain that has one —
        /// exactly the data-driven plumbing item 34(b) added.
        fn with_a_surcharge(session: &mut Session, human: PlayerId) -> String {
            let def_id = session.state.player(human).captain.def_id.clone();
            let mut registry = (*session.registry).clone();
            let mut captain = registry
                .captain_def(&def_id)
                .expect("the captain is registered")
                .clone();
            captain.verso.surcharge = Some(SpecialAttack {
                name: "Surcharge d'Essai".to_string(),
                cost: 2,
                atk_bonus: 3,
                description: Some("Une surcharge de test.".to_string()),
                ..Default::default()
            });
            registry.register_captain(captain);
            session.registry = Arc::new(registry);
            "Surcharge d'Essai".to_string()
        }

        #[test]
        fn the_captain_menu_plays_a_surcharge_the_engine_offers() {
            let mut session = table(23);
            let human = session.human;
            let ai = session.ai_player();
            let name = with_a_surcharge(&mut session, human);
            engage(&mut session, Slot::V2);
            let prey = put(&mut session, "MR-001", ai, Slot::V1);
            session.refresh_valid();

            assert!(
                session
                    .valid
                    .iter()
                    .any(|a| matches!(a, GameAction::UseSurcharge { .. })),
                "the engine does not offer the surcharge"
            );

            let view = captain_menu_view(
                &session.state,
                &session.registry,
                &session.valid,
                human,
                human,
            )
            .expect("the captain has a menu");
            let surcharge = view
                .surcharge
                .clone()
                .expect("the menu prices the surcharge");
            assert_eq!(surcharge.label, name);
            assert_eq!(surcharge.cost, 2);
            assert!(!surcharge.disabled, "the button is live: {surcharge:?}");
            assert!(
                view.abilities
                    .iter()
                    .any(|a| a.kind == crate::panels::model::AbilityKind::Surcharge),
                "and lists it among the verso's powers"
            );

            let mut mode = UiMode::CaptainMenu { player_id: human };
            assert_eq!(run(&mut mode, surcharge.command), None);
            assert_eq!(
                mode,
                UiMode::SelectingTarget {
                    attacker_id: captain_key(human),
                    kind: AttackKind::Surcharge
                }
            );
            // The surcharge has its own target list, not the ★ special's.
            assert!(attack_targets(&mode, &session.valid, ai).contains(&prey));

            let command = on_board_char_click(&mode, &session.valid, ai, &prey, false, "MR-001");
            let action = run(&mut mode, command).expect("the click dispatches");
            assert_eq!(
                action,
                GameAction::UseSurcharge {
                    target_instance_id: prey,
                    target_is_captain: None,
                }
            );
            assert_offered(&session, &action);
        }

        /// The enemy captain is reachable too — the other half of `useSurcharge`.
        #[test]
        fn a_surcharge_can_be_aimed_at_the_enemy_captain() {
            let mut session = table(29);
            let human = session.human;
            let ai = session.ai_player();
            with_a_surcharge(&mut session, human);
            engage(&mut session, Slot::V2);
            session.refresh_valid();

            let mut mode = UiMode::SelectingTarget {
                attacker_id: captain_key(human),
                kind: AttackKind::Surcharge,
            };
            assert!(attack_targets(&mode, &session.valid, ai).contains(&captain_key(ai)));

            let command = on_captain_click(&mode, &session.valid, ai, ai);
            let action = run(&mut mode, command).expect("the click dispatches");
            assert_eq!(
                action,
                GameAction::UseSurcharge {
                    target_instance_id: captain_key(ai),
                    target_is_captain: Some(true),
                }
            );
            assert_offered(&session, &action);
        }

        /// §8.34(c) — a recto captain has no attack and no surcharge, and the
        /// menu says so instead of offering a dead button.
        #[test]
        fn a_recto_captain_is_told_it_cannot_attack() {
            let mut session = table(31);
            let human = session.human;
            with_a_surcharge(&mut session, human);
            session.refresh_valid();
            assert!(!session.state.player(human).captain.flipped);

            let view = captain_menu_view(
                &session.state,
                &session.registry,
                &session.valid,
                human,
                human,
            )
            .expect("the captain has a menu");
            let surcharge = view.surcharge.expect("the surcharge is still priced");
            assert!(surcharge.disabled);
            assert_eq!(
                surcharge.reason.as_deref(),
                Some("Le Capitaine recto ne peut pas attaquer")
            );
            assert!(
                !session
                    .valid
                    .iter()
                    .any(|a| matches!(a, GameAction::UseSurcharge { .. })),
                "and the engine agrees"
            );
        }

        // --- §8.6 / §8.33 the free flip --------------------------

        #[test]
        fn the_flip_button_prices_itself_and_says_when_it_is_free() {
            let mut session = table(37);
            let human = session.human;
            session.refresh_valid();

            let view = captain_menu_view(
                &session.state,
                &session.registry,
                &session.valid,
                human,
                human,
            )
            .expect("the captain has a menu");
            let printed = session
                .registry
                .captain_def(&session.state.player(human).captain.def_id)
                .and_then(|d| d.flip_condition.cost)
                .unwrap_or(0);
            assert_eq!(view.flip_cost, printed);
            // Nothing has been KO'd and it is early: no clause holds yet.
            assert_eq!(view.free_flip_reason, None);

            // Turn 7 is Shanks' `freeIfTurnGte`; whichever captain was dealt,
            // the label must agree with the engine's own predicate.
            session.state.turn_number = 9;
            session.refresh_valid();
            let later = captain_menu_view(
                &session.state,
                &session.registry,
                &session.valid,
                human,
                human,
            )
            .unwrap();
            let engine_says =
                tcgop_engine::captain::free_flip_reason(&session.state, &session.registry, human)
                    .unwrap();
            assert_eq!(
                later.free_flip_reason,
                engine_says.map(crate::panels::model::free_flip_label)
            );
        }

        // --- §8.38 an ally-facing support special ----------------

        /// `RH-009` Stimulant is `isSupport` + `buffAllyAtk` + `cleanse`: the
        /// engine offers it as a `specialAttack` whose targets are the caster's
        /// **own** board. The TS-shaped "enemy side only" highlight lit none of
        /// them and swallowed the click.
        #[test]
        fn an_ally_facing_support_special_lights_and_dispatches_on_your_own_half() {
            let mut session = table(41);
            let human = session.human;
            let ai = session.ai_player();
            let caster = put(&mut session, "RH-009", human, Slot::V1);
            let friend = put(&mut session, "MG-002", human, Slot::V2);
            session.refresh_valid();

            let offered: Vec<&GameAction> = session
                .valid
                .iter()
                .filter(|a| matches!(a, GameAction::SpecialAttack { attacker_instance_id, .. } if *attacker_instance_id == caster))
                .collect();
            assert!(
                !offered.is_empty(),
                "the engine does not offer the support special: {:?}",
                session.valid
            );

            let mut mode = UiMode::SelectingTarget {
                attacker_id: caster.clone(),
                kind: AttackKind::Special,
            };
            let targets = attack_targets(&mode, &session.valid, ai);
            assert!(
                targets.contains(&friend),
                "an ally must be a legal target: {targets:?}"
            );

            // The cell is lit even though it is on the player's own side.
            let highlight = cell_highlight(
                &mode,
                Slot::V2,
                true,
                Some(&friend),
                &BTreeSet::new(),
                &targets,
                &BTreeSet::new(),
                &BTreeSet::new(),
                false,
            );
            assert!(highlight.valid_target && !highlight.dimmed);

            let command = on_board_char_click(&mode, &session.valid, ai, &friend, true, "MG-002");
            let action = run(&mut mode, command).expect("the click dispatches");
            assert_eq!(
                action,
                GameAction::SpecialAttack {
                    attacker_instance_id: caster,
                    target_instance_id: friend,
                    target_is_captain: None,
                }
            );
            assert_offered(&session, &action);
        }

        /// §8.38 — the same aim runs through [`UiMode::SelectingTarget`], but a
        /// support special never builds a `PendingAttack`: it deals no damage
        /// and its legal target may be one of your own allies. The header has
        /// to fly the support colours, not the attack red.
        #[test]
        fn aiming_a_support_special_flies_the_support_colours() {
            let mut session = table(41);
            let human = session.human;
            let caster = put(&mut session, "RH-004", human, Slot::V1);
            let blade = put(&mut session, "MG-002", human, Slot::V2);
            session.refresh_valid();

            let support = UiMode::SelectingTarget {
                attacker_id: caster.clone(),
                kind: AttackKind::Special,
            };
            assert!(aim_is_support(&support, &session.state, &session.registry));
            let hint = status_hint(&support, false, false, true);
            assert_eq!(hint.tone, StatusTone::Support);
            assert_eq!(hint.text, "Cible du pouvoir");

            // A special that really is a blow keeps the attack red.
            let blow = UiMode::SelectingTarget {
                attacker_id: blade,
                kind: AttackKind::Special,
            };
            assert!(!aim_is_support(&blow, &session.state, &session.registry));
            assert_eq!(
                status_hint(
                    &blow,
                    false,
                    false,
                    aim_is_support(&blow, &session.state, &session.registry)
                )
                .tone,
                StatusTone::Target
            );

            // The base action of the very same unit is not this: it has its
            // own `baseSupportAction` route and its own mode.
            let base = UiMode::SelectingTarget {
                attacker_id: caster,
                kind: AttackKind::Base,
            };
            assert!(!aim_is_support(&base, &session.state, &session.registry));
        }

        // --- §8.5/§8.36/§8.38/§8.40 the permanent max-PV loss ----

        #[test]
        fn a_permanent_pv_loss_shortens_the_gauge_and_is_spelled_out() {
            use crate::board::model::{CellContent, board_view};

            let mut session = table(43);
            let human = session.human;
            let hurt = put(&mut session, "MG-002", human, Slot::V1);
            let printed = session
                .registry
                .card_def("MG-002")
                .and_then(|d| d.pv)
                .expect("a character has PV");
            {
                let card = session.state.card_mut(&hurt).unwrap();
                card.pv_max_loss = Some(2);
                card.current_pv = printed - 2;
                card.status_effects.push(tcgop_engine::types::StatusEffect {
                    effect_type: StatusEffectType::NoHeal,
                    turns_remaining: 2,
                    damage_per_turn: 0,
                    source: "test".into(),
                });
            }
            session.refresh_valid();

            let view = board_view(&session, &UiMode::Idle);
            let unit = view
                .you
                .front
                .iter()
                .find_map(|cell| match &cell.content {
                    CellContent::Unit(u) if u.instance_id == hurt => Some(u.clone()),
                    _ => None,
                })
                .expect("the unit is on the board");
            assert_eq!(unit.pv_max_loss, 2);
            assert_eq!(
                unit.max_pv,
                printed - 2,
                "the gauge is measured against the *reduced* maximum"
            );
            assert!(unit.damaged, "a shortened gauge is always drawn");
            assert!(
                (unit.hp_ratio - 1.0).abs() < 1e-6,
                "it is full, but shorter"
            );

            let menu =
                action_menu_view(&session.state, &session.registry, &session.valid, &hurt).unwrap();
            assert_eq!(menu.pv_max_loss, 2);
            assert!(menu.no_heal, "the `noHeal` status is surfaced too");
        }

        // --- the whole surface, in one sweep ---------------------

        /// The client's promise: **every** `GameAction` the engine can hand the
        /// human has a UI path. This walks a seeded game and, for every action
        /// that turns up in `Session::valid`, asserts the client knows how to
        /// produce it — through the hand, a board menu, the captain menu, the
        /// counter window or the footer.
        #[test]
        fn every_action_the_engine_offers_has_a_path_in_the_client() {
            let mut seen: BTreeSet<&'static str> = BTreeSet::new();
            for seed in [1u64, 5, 12, 19, 26, 33] {
                let mut session = make_session(seed);
                let human = session.human;
                for _ in 0..400 {
                    if session.winner().is_some() {
                        break;
                    }
                    for action in &session.valid {
                        assert!(
                            client_can_produce(&session, action),
                            "seed {seed}: no UI path produces {action:?}"
                        );
                        seen.insert(action.type_name());
                    }
                    // Playing the *first* legal action every time keeps the
                    // walk deterministic and drives the human into deploys,
                    // attacks and counter windows rather than passing.
                    let next = session
                        .valid
                        .first()
                        .cloned()
                        .unwrap_or(GameAction::EndTurn);
                    let ai = session.ai_player();
                    let level = session.ai_level;
                    let step =
                        if session.state.current_player == human || session.in_counter_window() {
                            next
                        } else {
                            let s = &mut session;
                            tcgop_engine::ai::ai_choose_action(
                                &s.state,
                                &s.registry,
                                &mut s.ctx,
                                ai,
                                level,
                            )
                            .unwrap_or(GameAction::EndTurn)
                        };
                    if session.dispatch(step).is_err() {
                        break;
                    }
                }
            }
            // A floor, so a sweep that silently stopped playing proves nothing:
            // these are the kinds a shipped Mugiwara-vs-Marines game must reach.
            for kind in [
                "endTurn",
                "deployCharacter",
                "deployShip",
                "equipObject",
                "playEvent",
                "baseAttack",
                "specialAttack",
                "baseSupportAction",
                "moveCharacter",
                "flipCaptain",
                "captainAttack",
                "useShield",
                "passCounter",
                "useHaki",
                "activateShip",
            ] {
                assert!(seen.contains(kind), "the sweep never saw {kind}: {seen:?}");
            }
            // The five the shipped catalogue cannot reach in a walk of its own
            // — `useSurcharge` has no data at all (§8.34(b)) and the fruit
            // path needs a legitimate bearer holding an awakened fruit — have
            // a rigged test each, above.
        }

        /// Is there a click, anywhere in the client, that produces `action`?
        fn client_can_produce(session: &Session, action: &GameAction) -> bool {
            let state = &session.state;
            let valid = &session.valid;
            match action {
                // Hand: click the card, then a slot / a bearer / *Confirmer*.
                GameAction::DeployCharacter { instance_id, .. }
                | GameAction::DeployShip { instance_id }
                | GameAction::PlayEvent { instance_id, .. } => {
                    hand_card_playable(valid, instance_id)
                }
                GameAction::EquipObject {
                    object_instance_id, ..
                } => hand_card_playable(valid, object_instance_id),
                // Counter window: one button per option.
                GameAction::PlayCounter { .. }
                | GameAction::UseShield { .. }
                | GameAction::PassCounter => {
                    crate::panels::model::counter_view(state, &session.registry, valid).is_some_and(
                        |view| {
                            view.options
                                .iter()
                                .any(|o| o.command == UiCommand::DispatchKeepUi(action.clone()))
                        },
                    )
                }
                // Board menus.
                GameAction::BaseAttack {
                    attacker_instance_id,
                    ..
                } => menu_aims(session, attacker_instance_id, AttackKind::Base),
                GameAction::SpecialAttack {
                    attacker_instance_id,
                    ..
                } => menu_aims(session, attacker_instance_id, AttackKind::Special),
                GameAction::BaseSupportAction { instance_id, .. } => {
                    action_menu_view(state, &session.registry, valid, instance_id)
                        .is_some_and(|view| view.support.is_some_and(|row| !row.disabled))
                }
                GameAction::MoveCharacter { instance_id, .. } => {
                    action_menu_view(state, &session.registry, valid, instance_id)
                        .is_some_and(|view| view.free_move.is_some_and(|row| !row.disabled))
                }
                GameAction::AwakenFruit { fruit_instance_id } => {
                    bearer_menu_offers(session, fruit_instance_id, |view| {
                        view.awakenings
                            .iter()
                            .any(|row| row.command == UiCommand::Dispatch(action.clone()))
                    })
                }
                GameAction::FruitSpecialAttack {
                    attacker_instance_id,
                    fruit_instance_id,
                    ..
                } => menu_aims(
                    session,
                    attacker_instance_id,
                    AttackKind::Fruit {
                        fruit_instance_id: fruit_instance_id.clone(),
                    },
                ),
                // The captain's own controls.
                GameAction::FlipCaptain { .. } => captain_menu_view(
                    state,
                    &session.registry,
                    valid,
                    session.human,
                    session.human,
                )
                .is_some_and(|view| view.can_flip),
                GameAction::CaptainAttack { is_special, .. } => captain_menu_view(
                    state,
                    &session.registry,
                    valid,
                    session.human,
                    session.human,
                )
                .is_some_and(|view| {
                    if is_special.unwrap_or(false) {
                        view.can_special_attack
                    } else {
                        view.can_attack
                    }
                }),
                GameAction::UseSurcharge { .. } => captain_menu_view(
                    state,
                    &session.registry,
                    valid,
                    session.human,
                    session.human,
                )
                .is_some_and(|view| view.surcharge.is_some_and(|row| !row.disabled)),
                GameAction::UseHaki { haki_type, .. } => match haki_type {
                    tcgop_engine::types::HakiType::King => captain_menu_view(
                        state,
                        &session.registry,
                        valid,
                        session.human,
                        session.human,
                    )
                    .is_some_and(|view| view.can_king_haki),
                    // Observation is a counter-window button; Armament is a
                    // passive the engine never offers as an action (§8.42).
                    _ => crate::panels::model::counter_view(state, &session.registry, valid)
                        .is_some_and(|view| {
                            view.options
                                .iter()
                                .any(|o| o.command == UiCommand::DispatchKeepUi(action.clone()))
                        }),
                },
                // Ship menu / footer.
                GameAction::ActivateShip { ship_instance_id } => {
                    crate::panels::model::ship_menu_view(
                        state,
                        &session.registry,
                        valid,
                        ship_instance_id,
                        true,
                    )
                    .is_some_and(|view| view.can_activate)
                }
                // The gold CTA.
                GameAction::EndTurn => true,
            }
        }

        /// Does the menu of `attacker_id` carry a live row that arms `kind`?
        fn menu_aims(session: &Session, attacker_id: &str, kind: AttackKind) -> bool {
            let armed = UiCommand::SetMode(UiMode::SelectingTarget {
                attacker_id: attacker_id.to_string(),
                kind: kind.clone(),
            });
            if is_captain_key(attacker_id) {
                return captain_menu_view(
                    &session.state,
                    &session.registry,
                    &session.valid,
                    session.human,
                    session.human,
                )
                .is_some_and(|view| match kind {
                    AttackKind::Base => view.can_attack,
                    AttackKind::Special => view.can_special_attack,
                    AttackKind::Surcharge => view.surcharge.is_some_and(|row| !row.disabled),
                    AttackKind::Fruit { .. } => view
                        .fruit_specials
                        .iter()
                        .any(|row| !row.disabled && row.command == armed),
                });
            }
            action_menu_view(
                &session.state,
                &session.registry,
                &session.valid,
                attacker_id,
            )
            .is_some_and(|view| {
                let rows = view
                    .base
                    .iter()
                    .chain(view.special.iter())
                    .chain(view.fruit_specials.iter());
                rows.filter(|row| !row.disabled)
                    .any(|row| row.command == armed)
            })
        }

        /// Run `check` on the menu of whoever wears `fruit_id`.
        fn bearer_menu_offers(
            session: &Session,
            fruit_id: &str,
            check: impl Fn(&crate::panels::model::ActionMenuView) -> bool,
        ) -> bool {
            for (id, card) in &session.state.cards {
                if card.attached_objects.iter().any(|o| o == fruit_id)
                    && let Some(view) =
                        action_menu_view(&session.state, &session.registry, &session.valid, id)
                    && check(&view)
                {
                    return true;
                }
            }
            // The captain is a bearer too (§8.28 follow-up): its *Éveiller*
            // rows live on the captain menu.
            captain_menu_view(
                &session.state,
                &session.registry,
                &session.valid,
                session.human,
                session.human,
            )
            .is_some_and(|view| {
                view.awakenings.iter().any(|row| {
                    row.command
                        == UiCommand::Dispatch(GameAction::AwakenFruit {
                            fruit_instance_id: fruit_id.to_string(),
                        })
                })
            })
        }
    }
}
