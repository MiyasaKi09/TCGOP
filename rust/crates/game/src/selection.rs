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

/// Exactly the twelve arms of `Game.tsx`'s `UIMode` union.
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
    /// `{ type: "selectingTarget", attackerId, isSpecial }`.
    ///
    /// `attacker_id` is either a card instance id or the synthetic
    /// `"captain_<player>"` id (see [`captain_key`]).
    SelectingTarget {
        attacker_id: String,
        is_special: bool,
    },
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
        _ => {}
    }
    slots
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
    let UiMode::SelectingTarget {
        attacker_id,
        is_special,
    } = mode
    else {
        return targets;
    };

    if is_captain_key(attacker_id) {
        for a in valid {
            if let GameAction::CaptainAttack {
                target_instance_id,
                target_is_captain,
                is_special: action_special,
            } = a
            {
                if action_special.unwrap_or(false) != *is_special {
                    continue;
                }
                if target_is_captain.unwrap_or(false) {
                    targets.insert(captain_key(ai_player));
                } else {
                    targets.insert(target_instance_id.clone());
                }
            }
        }
        return targets;
    }

    for a in valid {
        let (attacker, target, is_captain, matches_kind) = match a {
            GameAction::BaseAttack {
                attacker_instance_id,
                target_instance_id,
                target_is_captain,
            } => (
                attacker_instance_id,
                target_instance_id,
                target_is_captain,
                !*is_special,
            ),
            GameAction::SpecialAttack {
                attacker_instance_id,
                target_instance_id,
                target_is_captain,
            } => (
                attacker_instance_id,
                target_instance_id,
                target_is_captain,
                *is_special,
            ),
            _ => continue,
        };
        if !matches_kind || attacker != attacker_id {
            continue;
        }
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
    let UiMode::SelectingTarget {
        attacker_id,
        is_special,
    } = mode
    else {
        return false;
    };

    fn has_zone(traits: Option<&Vec<AttackTrait>>) -> bool {
        traits.is_some_and(|t| t.contains(&AttackTrait::Zone))
    }

    if let Some(owner) = captain_key_owner(attacker_id) {
        let Some(def) = registry.captain_def(&state.player(owner).captain.def_id) else {
            return false;
        };
        return if *is_special {
            has_zone(def.verso.special_attack.attack_traits.as_ref())
        } else {
            has_zone(def.verso.base_action.attack_traits.as_ref())
        };
    }

    let Some(instance) = state.card(attacker_id) else {
        return false;
    };
    let Some(def) = registry.card_def(&instance.def_id) else {
        return false;
    };
    if *is_special {
        def.special_attack
            .as_ref()
            .is_some_and(|a| has_zone(a.attack_traits.as_ref()))
    } else {
        def.base_action
            .as_ref()
            .is_some_and(|a| has_zone(a.attack_traits.as_ref()))
    }
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
        | GameAction::PlayCounter { instance_id: id }
        | GameAction::MoveCharacter {
            instance_id: id, ..
        } => id == instance_id,
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
    let is_attack_target = !is_player_side
        && matches!(mode, UiMode::SelectingTarget { .. })
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
        UiMode::SelectingTarget {
            attacker_id,
            is_special,
        } if !is_player_side => {
            let targets = attack_targets(mode, valid, ai_player);
            match occupant {
                Some(target) if targets.contains(target) => {
                    // TS `handleSlotClick` has **no** captain branch: it always
                    // emits baseAttack / specialAttack, carrying the synthetic
                    // `captain_<player>` id as `attackerInstanceId`. The engine
                    // decodes that prefix itself (`get_attacker_owner`), so the
                    // action is legal; only `handleBoardCharClick` re-routes to
                    // `captainAttack`, and [`on_board_char_click`] mirrors that.
                    UiCommand::Dispatch(unit_attack_action(attacker_id, *is_special, target, false))
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
    if let UiMode::SelectingTarget {
        attacker_id,
        is_special,
    } = mode
        && !is_player_side
        && attack_targets(mode, valid, ai_player).contains(instance_id)
    {
        return UiCommand::Dispatch(attack_action(attacker_id, *is_special, instance_id, false));
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
    if let UiMode::SelectingTarget {
        attacker_id,
        is_special,
    } = mode
    {
        let key = captain_key(ai_player);
        if player_id == ai_player && attack_targets(mode, valid, ai_player).contains(&key) {
            return UiCommand::Dispatch(attack_action(attacker_id, *is_special, &key, true));
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

/// Build the right attack action for an attacker id (card **or** captain key)
/// — TS `handleBoardCharClick`, which branches on
/// `uiMode.attackerId.startsWith("captain_")`.
fn attack_action(
    attacker_id: &str,
    is_special: bool,
    target: &str,
    target_is_captain: bool,
) -> GameAction {
    if is_captain_key(attacker_id) {
        return GameAction::CaptainAttack {
            target_instance_id: target.to_string(),
            target_is_captain: if target_is_captain { Some(true) } else { None },
            // The Rust engine enumerates the captain's ★ special on the same
            // `captainAttack` action with `isSpecial: true` (§8.2 item 34(a)) —
            // a rule the TS engine never had, and the reason this cannot simply
            // mirror `handleBoardCharClick`'s hard-coded base variant. The
            // CaptainMenu prices that ability and offers a button for it; the
            // flag is what makes the button reach the engine.
            is_special: if is_special { Some(true) } else { None },
        };
    }
    unit_attack_action(attacker_id, is_special, target, target_is_captain)
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

/// TS `statusText` IIFE.
pub fn status_hint(mode: &UiMode, is_ai_turn: bool, in_counter_window: bool) -> StatusHint {
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
        UiMode::SelectingTarget { .. } => hint("Choisissez une cible", StatusTone::Target, true),
        UiMode::SelectingSupportTarget { .. } => {
            hint("Cible du pouvoir", StatusTone::Support, true)
        }
        UiMode::SelectingSlot { .. } => hint("Choisissez un emplacement", StatusTone::Deploy, true),
        UiMode::SelectingCaptainSlot => hint("Placez le capitaine", StatusTone::Captain, true),
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
            is_special: false,
        };
        let special = UiMode::SelectingTarget {
            attacker_id: "zoro".into(),
            is_special: true,
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
            is_special: false,
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
        let aiming = |is_special| UiMode::SelectingTarget {
            attacker_id: captain_key(PlayerId::Player1),
            is_special,
        };
        assert_eq!(
            attack_targets(&aiming(false), &valid, AI),
            BTreeSet::from(["coby".to_string()]),
            "the base attack only reaches what the base attack was offered on"
        );
        assert_eq!(
            attack_targets(&aiming(true), &valid, AI),
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
            is_special: true,
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
            is_special: false,
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
                is_special: false,
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
        let hint = status_hint(&UiMode::Idle, false, false);
        assert_eq!(hint.text, "À toi");
        assert_eq!(hint.glyph, Some("\u{2726}"));
        assert_eq!(hint.tone, StatusTone::Ready);
        assert!(!hint.pulse);
        // Every other state keeps its bare label.
        assert_eq!(status_hint(&UiMode::Idle, true, false).glyph, None);
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
            is_special: false,
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
            is_special: false,
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

    #[test]
    fn captain_clicks_attack_then_open_the_menu() {
        let key = captain_key(AI);
        let valid = vec![base_attack("zoro", &key, true)];
        let mode = UiMode::SelectingTarget {
            attacker_id: "zoro".into(),
            is_special: false,
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
            is_special: false,
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
            is_special: false,
        };
        assert_eq!(status_hint(&target, true, false).tone, StatusTone::Waiting);
        assert_eq!(status_hint(&target, false, true).tone, StatusTone::Danger);
        assert_eq!(status_hint(&target, false, false).tone, StatusTone::Target);
        assert_eq!(
            status_hint(&UiMode::Idle, false, false).tone,
            StatusTone::Ready
        );
        assert!(!status_hint(&UiMode::Idle, false, false).pulse);
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
            is_special: true,
        };
        assert!(!attack_is_zone(&bogus, &session.state, &session.registry));

        // A captain key resolves against the *verso* face and must not panic.
        let cap = UiMode::SelectingTarget {
            attacker_id: captain_key(PlayerId::Player1),
            is_special: true,
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

    /// `handleSlotClick` has no captain branch: a captain attacker rides in the
    /// `attackerInstanceId` of a plain baseAttack / specialAttack, which the
    /// engine decodes by prefix. `handleBoardCharClick` is the one that routes
    /// to `captainAttack`.
    #[test]
    fn the_two_handlers_build_the_attack_the_typescript_builds() {
        let key = captain_key(PlayerId::Player1);
        // What the engine actually offers when a captain is the attacker.
        let valid = vec![GameAction::CaptainAttack {
            target_instance_id: "foe1".into(),
            target_is_captain: None,
            is_special: None,
        }];
        let mode = UiMode::SelectingTarget {
            attacker_id: key.clone(),
            is_special: false,
        };

        // Slot handler → baseAttack carrying the synthetic id.
        assert_eq!(
            on_slot_click(
                &mode,
                &valid,
                PlayerId::Player2,
                Slot::V1,
                false,
                Some("foe1")
            ),
            UiCommand::Dispatch(GameAction::BaseAttack {
                attacker_instance_id: key.clone(),
                target_instance_id: "foe1".into(),
                target_is_captain: None,
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
}
