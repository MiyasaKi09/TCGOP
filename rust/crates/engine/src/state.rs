//! Runtime game state — port of the runtime section of `src/types/index.ts`
//! (`CardInstance`, `CaptainInstance`, `PlayerState`, `PendingAttack`,
//! `LogEntry`, `GameState`), of `src/engine/gameState.ts` (construction and
//! the full turn lifecycle: `startTurn` / `endTurn`), of `init.ts::createGame`
//! and of the small helpers in `src/engine/utils.ts` and `src/engine/volonte.ts`.
//!
//! JSON compatibility: every struct serialises with the TS field names and
//! nothing else — `GameState` has exactly the nine keys of the TS interface.
//! The TS module globals (`Math.random()`, `utils.ts::instanceCounter`,
//! `Date.now()`) live in [`EngineContext`], passed explicitly.

#![allow(clippy::collapsible_if)]
// ^ The nested `if` / `if let` blocks in this module mirror the TypeScript
// source branch for branch (see the per-function `PORT:` references). Merging
// them into let-chains would break that 1:1 reading, which is the whole point
// of the port, so the lint is turned off for this file only.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::board::remove_from_board;
use crate::context::EngineContext;
use crate::error::EngineError;
use crate::passives::{
    apply_enemy_debuff_auras, apply_on_ko_effects, apply_start_of_turn_passives,
    recalculate_passive_buffs,
};
use crate::registry::CardRegistry;
use crate::types::{
    AttackTrait, DeckDef, Element, Modifier, ModifierDuration, PassiveEffect, Phase, PlayerId,
    Slot, StatusEffect, StatusEffectType, Zone,
};

// ============================================================
// Constants (utils.ts)
// ============================================================

/// TS `VOLONTE_CAP` — Volonte cap.
pub const VOLONTE_CAP: i32 = 10;
/// TS `STARTING_HAND_SIZE` — starting hand size.
pub const STARTING_HAND_SIZE: usize = 6;
/// TS `HAND_LIMIT` — maximum hand size, excess is discarded at end of turn (Rulebook v3.1 §10).
pub const HAND_LIMIT: usize = 10;
/// TS `ALLY_KO_BONUS_VOL` — bonus Vol on ally KO.
pub const ALLY_KO_BONUS_VOL: i32 = 2;

// ------------------------------------------------------------
// `usedOnceAbilities` keys (decision §8.50)
// ------------------------------------------------------------
//
// The list stays a `Vec<String>` holding the exact strings the TypeScript
// engine wrote: the UI compares an ability *name* (`shipActive.name`,
// `specialAttack.name`, …) against its entries, so typing the key would break
// both that comparison and the wire format. The engine's own sentinels — the
// ones that are not an ability name — go through the constants below instead
// of being spelled out at each site.

/// The once-per-character "this character already survived a lethal hit at
/// 1 PV" tag (`survivesLethal` / `MG-005`), written by the damage step in
/// `combat.rs`.
pub const ONCE_SURVIVED: &str = "survived";

/// The once-per-character Straw Hat tag (`MG-001`, the hat's one-shot
/// protection), written by the damage step in `combat.rs`.
pub const ONCE_STRAWHAT: &str = "strawhat";

/// Prefix of the once-per-game key of a captain **surcharge** (decision
/// §8.34): the surcharge's own name is appended — see [`once_surcharge`].
pub const ONCE_SURCHARGE_PREFIX: &str = "surcharge_";

/// The `usedOnceAbilities` key guarding a `oncePerGame` captain surcharge
/// named `name` (decision §8.34/§8.50).
pub fn once_surcharge(name: &str) -> String {
    format!("{ONCE_SURCHARGE_PREFIX}{name}")
}

// ============================================================
// Instance ids (utils.ts `generateInstanceId`)
// ============================================================

/// TS `generateInstanceId(defId)` = `${defId}_${++instanceCounter}_${Date.now().toString(36)}`
/// — see [`crate::context::generate_instance_id`]; the counter and the clock
/// are the [`EngineContext`] globals.
pub use crate::context::generate_instance_id;

// ============================================================
// Runtime instances
// ============================================================

/// TS `CardInstance`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardInstance {
    pub instance_id: String,
    pub def_id: String,
    pub owner: PlayerId,
    pub zone: Zone,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<Slot>,
    pub tapped: bool,
    pub current_pv: i32,
    /// Object instanceIds attached to this character
    pub attached_objects: Vec<String>,
    pub modifiers: Vec<Modifier>,
    pub status_effects: Vec<StatusEffect>,
    /// Turn this card was deployed (for summoning sickness).
    ///
    /// `i64`, not `u32`: TS clears summoning sickness by writing the sentinel
    /// `deployedTurn = -1` (`turnManager.ts` `rushBuff`, `captain.ts`
    /// `grantSelfRush`), and that value has to survive serialisation so the
    /// JSON matches the TS engine key for key and byte for byte.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deployed_turn: Option<i64>,
    /// Has used base action this turn
    pub used_base_action: bool,
    /// Has used special attack this turn
    pub used_special_attack: bool,
    /// Logia: has already ignored damage this turn
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logia_used_this_turn: Option<bool>,
    /// 1x/game abilities already used
    pub used_once_abilities: Vec<String>,
    /// Is this a Devil Fruit that has been awakened?
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_awakened: Option<bool>,
    /// Permanent loss of maximum PV (decision §8.5): the heal cap is
    /// `def.pv - pv_max_loss`, never the printed PV alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pv_max_loss: Option<i32>,
    /// Decision §8.37 (`betrayal`): the player who *controls* this instance
    /// while it is on loan. `owner` never changes, so KO bonuses, graveyards
    /// and win conditions keep pointing at the original owner.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controlled_by: Option<PlayerId>,
    /// Decision §8.37 (`betrayal`): the slot of the owner's board the loan
    /// returns to at the borrower's end of turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loan_return_slot: Option<Slot>,
}

impl CardInstance {
    /// The object literal built in gameState.ts `createPlayerState`
    /// (zone `"deck"`, untapped, `currentPv = cardDef.pv ?? 0`, empty lists).
    pub fn new(instance_id: String, def_id: String, owner: PlayerId, current_pv: i32) -> Self {
        CardInstance {
            instance_id,
            def_id,
            owner,
            zone: Zone::Deck,
            slot: None,
            tapped: false,
            current_pv,
            attached_objects: Vec::new(),
            modifiers: Vec::new(),
            status_effects: Vec::new(),
            deployed_turn: None,
            used_base_action: false,
            used_special_attack: false,
            logia_used_this_turn: None,
            used_once_abilities: Vec::new(),
            is_awakened: None,
            pv_max_loss: None,
            controlled_by: None,
            loan_return_slot: None,
        }
    }

    /// The player this instance currently acts for: the borrower while it is
    /// on loan (decision §8.37 `betrayal`), otherwise its owner.
    pub fn controller(&self) -> PlayerId {
        self.controlled_by.unwrap_or(self.owner)
    }

    /// The instance's maximum PV: the printed `def.pv` minus any permanent
    /// max-PV loss (decision §8.5). `None` for a definition without `pv`.
    pub fn max_pv(&self, printed_pv: Option<i32>) -> Option<i32> {
        printed_pv.map(|pv| pv - self.pv_max_loss.unwrap_or(0))
    }

    /// TS `card.statusEffects.some((e) => e.type === t)`.
    pub fn has_status(&self, t: StatusEffectType) -> bool {
        self.status_effects.iter().any(|e| e.effect_type == t)
    }

    /// TS `card.statusEffects.find((e) => e.type === t)`.
    pub fn status(&self, t: StatusEffectType) -> Option<&StatusEffect> {
        self.status_effects.iter().find(|e| e.effect_type == t)
    }

    /// TS `card.usedOnceAbilities.includes(name)`.
    pub fn used_once(&self, name: &str) -> bool {
        self.used_once_abilities.iter().any(|x| x == name)
    }
}

/// TS `CaptainInstance`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptainInstance {
    pub def_id: String,
    pub owner: PlayerId,
    pub flipped: bool,
    pub current_pv: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<Slot>,
    pub tapped: bool,
    /// Decision §8.28 (follow-up) — the objects the captain wears.
    ///
    /// The three signature SR Devil Fruits are printed "Équipable sur Luffy /
    /// Crocodile / Akainu", and those three names exist in the game **only** as
    /// captains (no set ships a character whose name contains them), so the
    /// captain has to be able to carry an object for the card to mean anything.
    ///
    /// Serialised exactly like a new optional field: `#[serde(default)]` plus
    /// `skip_serializing_if`, so a captain with no object is byte-for-byte the
    /// captain the TypeScript engine wrote.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attached_objects: Vec<String>,
    pub modifiers: Vec<Modifier>,
    pub status_effects: Vec<StatusEffect>,
    /// Turn the captain flipped onto the board — `i64` for the same reason as
    /// [`CardInstance::deployed_turn`]: `grantSelfRush` writes `-1`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deployed_turn: Option<i64>,
    pub used_base_action: bool,
    pub used_special_attack: bool,
    pub used_once_abilities: Vec<String>,
    /// Decision §8.40 — Logia intangibility on a captain is once per turn,
    /// exactly like [`CardInstance::logia_used_this_turn`] on a character.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logia_used_this_turn: Option<bool>,
    /// Decision §8.40 — permanent maximum-PV loss (the Sand element on a
    /// captain), the captain counterpart of [`CardInstance::pv_max_loss`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pv_max_loss: Option<i32>,
}

impl CaptainInstance {
    /// The object literal built in gameState.ts `createPlayerState`
    /// (`flipped: false`, `currentPv: captainDef.recto.pv`, untapped).
    pub fn new(def_id: String, owner: PlayerId, recto_pv: i32) -> Self {
        CaptainInstance {
            def_id,
            owner,
            flipped: false,
            current_pv: recto_pv,
            slot: None,
            tapped: false,
            attached_objects: Vec::new(),
            modifiers: Vec::new(),
            status_effects: Vec::new(),
            deployed_turn: None,
            used_base_action: false,
            used_special_attack: false,
            used_once_abilities: Vec::new(),
            logia_used_this_turn: None,
            pv_max_loss: None,
        }
    }

    pub fn has_status(&self, t: StatusEffectType) -> bool {
        self.status_effects.iter().any(|e| e.effect_type == t)
    }

    pub fn used_once(&self, name: &str) -> bool {
        self.used_once_abilities.iter().any(|x| x == name)
    }
}

// ============================================================
// Board
// ============================================================

/// TS `PlayerState.board: Record<Slot, string | null>` — every slot is always
/// present, so it is a struct rather than a map. Serialises as
/// `{ "V1": null, "V2": ..., ... }` exactly like the TS object.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Board {
    #[serde(rename = "V1")]
    pub v1: Option<String>,
    #[serde(rename = "V2")]
    pub v2: Option<String>,
    #[serde(rename = "V3")]
    pub v3: Option<String>,
    #[serde(rename = "A1")]
    pub a1: Option<String>,
    #[serde(rename = "A2")]
    pub a2: Option<String>,
    #[serde(rename = "A3")]
    pub a3: Option<String>,
}

impl Board {
    /// TS `emptyBoard` (all six slots `null`).
    pub fn empty() -> Self {
        Board::default()
    }

    /// TS `player.board[slot]`.
    pub fn get(&self, slot: Slot) -> Option<&String> {
        match slot {
            Slot::V1 => self.v1.as_ref(),
            Slot::V2 => self.v2.as_ref(),
            Slot::V3 => self.v3.as_ref(),
            Slot::A1 => self.a1.as_ref(),
            Slot::A2 => self.a2.as_ref(),
            Slot::A3 => self.a3.as_ref(),
        }
    }

    /// Mutable access to the slot cell (`player.board[slot] = ...`).
    pub fn slot_mut(&mut self, slot: Slot) -> &mut Option<String> {
        match slot {
            Slot::V1 => &mut self.v1,
            Slot::V2 => &mut self.v2,
            Slot::V3 => &mut self.v3,
            Slot::A1 => &mut self.a1,
            Slot::A2 => &mut self.a2,
            Slot::A3 => &mut self.a3,
        }
    }

    /// `player.board[slot] = instanceId` — returns the previous occupant.
    pub fn set(&mut self, slot: Slot, instance_id: Option<String>) -> Option<String> {
        std::mem::replace(self.slot_mut(slot), instance_id)
    }

    /// TS `player.board[slot] !== null`.
    pub fn is_occupied(&self, slot: Slot) -> bool {
        self.get(slot).is_some()
    }

    /// Iterate `(slot, occupant)` in TS `Object.values(player.board)` order
    /// (V1, V2, V3, A1, A2, A3).
    pub fn iter(&self) -> impl Iterator<Item = (Slot, Option<&String>)> + '_ {
        Slot::ALL.into_iter().map(move |s| (s, self.get(s)))
    }

    /// Occupied slots and their instance ids, in board order.
    pub fn occupied(&self) -> Vec<(Slot, &String)> {
        self.iter()
            .filter_map(|(s, id)| id.map(|id| (s, id)))
            .collect()
    }

    /// Instance ids on the board, in board order (TS
    /// `Object.values(player.board).filter((id): id is string => !!id)`).
    pub fn instance_ids(&self) -> Vec<&String> {
        self.iter().filter_map(|(_, id)| id).collect()
    }

    /// Slots with no occupant, in board order.
    pub fn empty_slots(&self) -> Vec<Slot> {
        self.iter()
            .filter(|(_, id)| id.is_none())
            .map(|(s, _)| s)
            .collect()
    }

    /// Which slot holds `instance_id`, if any.
    pub fn slot_of(&self, instance_id: &str) -> Option<Slot> {
        self.iter()
            .find(|(_, id)| id.is_some_and(|id| id == instance_id))
            .map(|(s, _)| s)
    }

    /// Number of occupied slots.
    pub fn count(&self) -> usize {
        self.iter().filter(|(_, id)| id.is_some()).count()
    }
}

// ============================================================
// Player state
// ============================================================

/// TS `PlayerState`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerState {
    pub id: PlayerId,
    pub captain: CaptainInstance,
    /// instanceIds (top = index 0)
    pub deck: Vec<String>,
    /// instanceIds
    pub hand: Vec<String>,
    /// instanceIds
    pub graveyard: Vec<String>,
    pub board: Board,
    /// Active ship instanceId (max 1)
    pub active_ship: Option<String>,
    pub volonte: i32,
    /// Has used free move this turn
    pub used_free_move: bool,
    /// Has drawn this turn
    pub has_drawn: bool,
    /// Haki: observation used this turn
    pub observation_used: bool,
    /// Haki: armament used this turn
    pub armament_used: bool,
    /// Haki du Roi used this game
    pub king_used: bool,
    /// One of this player's allies was KO'd this turn (free captain flip) (TS `allyKOedThisTurn`)
    #[serde(
        rename = "allyKOedThisTurn",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub ally_ko_ed_this_turn: Option<bool>,
    /// One of this player's characters has been KO'd this game (Flashback) (TS `charKOedThisGame`)
    #[serde(
        rename = "charKOedThisGame",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub char_ko_ed_this_game: Option<bool>,
    /// This player's attacks pierce Logia this turn (granted Haki this turn)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub haki_this_turn: Option<bool>,
    /// Decision §8.37 (`embargo`, `MR-027`): while `> 0` this player can
    /// neither equip an object nor deploy a ship. Decremented at the end of
    /// this player's own turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embargo_turns: Option<i32>,
}

impl PlayerState {
    /// TS `player.allyKOedThisTurn` truthiness.
    pub fn ally_koed_this_turn(&self) -> bool {
        self.ally_ko_ed_this_turn.unwrap_or(false)
    }
    /// TS `player.charKOedThisGame` truthiness.
    pub fn char_koed_this_game(&self) -> bool {
        self.char_ko_ed_this_game.unwrap_or(false)
    }
    /// TS `player.hakiThisTurn` truthiness.
    pub fn has_haki_this_turn(&self) -> bool {
        self.haki_this_turn.unwrap_or(false)
    }
    /// Decision §8.37: is this player under `MR-027` Embargo right now?
    pub fn is_embargoed(&self) -> bool {
        self.embargo_turns.unwrap_or(0) > 0
    }
}

/// TS `GameState.players: Record<PlayerId, PlayerState>` — always exactly the
/// two players, serialised as `{ "player1": ..., "player2": ... }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Players {
    #[serde(rename = "player1")]
    pub player1: PlayerState,
    #[serde(rename = "player2")]
    pub player2: PlayerState,
}

impl Players {
    /// TS `state.players[id]`.
    pub fn get(&self, id: PlayerId) -> &PlayerState {
        match id {
            PlayerId::Player1 => &self.player1,
            PlayerId::Player2 => &self.player2,
        }
    }

    /// TS `draft.players[id]`.
    pub fn get_mut(&mut self, id: PlayerId) -> &mut PlayerState {
        match id {
            PlayerId::Player1 => &mut self.player1,
            PlayerId::Player2 => &mut self.player2,
        }
    }

    /// Both players, player1 first.
    pub fn iter(&self) -> impl Iterator<Item = &PlayerState> {
        [&self.player1, &self.player2].into_iter()
    }
}

// ============================================================
// Pending attack / log
// ============================================================

/// TS `PendingAttack`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingAttack {
    pub attacker_id: String,
    pub target_id: String,
    /// Is this targeting the captain?
    pub target_is_captain: bool,
    /// Is this a special attack?
    pub is_special: bool,
    /// Calculated raw damage before counter (vs the primary target's DEF)
    pub raw_damage: i32,
    /// Attacker's effective attack power before target DEF (for Zone/Total spread)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attack_power: Option<i32>,
    /// Attack element if any
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub element: Option<Element>,
    /// Attack traits
    pub attack_traits: Vec<AttackTrait>,
    /// Does this attack have haki?
    pub has_haki: bool,
    /// If true, the Bouclier/Shield block reaction cannot intercept this attack
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ignore_shield: Option<bool>,
    /// If true, Observation Haki / dodge effects cannot cancel this attack
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cannot_be_dodged: Option<bool>,
    /// On-hit control effects carried from the attack
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub immobilize: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sleep: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pushback: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pushback_slots: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strip_stealth: Option<bool>,
    /// TS `applyCounterSurvive` writes an ad-hoc property on the pending
    /// attack — `(draft.pendingAttack as PendingAttack & { survivePlayed?:
    /// boolean }).survivePlayed = true` (combat.ts:585) — which
    /// `applyCaptainDamage` / `applyCharacterDamage` read back
    /// (combat.ts:907, :948) to clamp the protected target to 1 PV instead of
    /// KO-ing it. It serialises as `"survivePlayed"`, so a state round-trips
    /// through the TS engine unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub survive_played: Option<bool>,
    /// Decision §8.15 — the instance [`crate::combat::apply_counter_survive`]
    /// protects. The 1-PV floor (and the `"survived"` once-tag) is applied by
    /// `applyCharacterDamage` **only** when the attack still resolves against
    /// this instance, so a shield block retargeting the attack no longer
    /// floors the blocker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub survive_target_id: Option<String>,
    /// Decision §8.13 — the total already subtracted by `reduceDamage`
    /// counters. A Bouclier block recomputes the damage from `attackPower`
    /// and has to re-apply this, or the block would refund a counter the
    /// defender already paid for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub damage_reduction: Option<i32>,
    /// Decision §8.13 — the `ignoreDef` the declaration applied to the
    /// target's DEF. A Bouclier block recomputes the damage against the
    /// blocker's DEF and has to apply the same clamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ignore_def: Option<i32>,
    /// Decision §8.38 — `SpecialAttack.permanentPvLoss`: the target loses N
    /// points of **maximum** PV once the attack lands (Crocodile's Desert
    /// Girasol, "La cible perd 2 PV permanent (Sable)").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permanent_pv_loss: Option<i32>,
    /// Decision §8.38 — `SpecialAttack.noHeal`: the target cannot be healed
    /// for 2 turns once the attack lands.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub no_heal: Option<bool>,
}

/// TS `LogEntry`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub turn: u32,
    pub player: PlayerId,
    pub message: String,
}

// ============================================================
// Game state
// ============================================================

/// TS `GameState`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameState {
    /// All card instances by instanceId.
    ///
    /// `BTreeMap` (not `HashMap`) so that iteration is deterministic — the TS
    /// code iterates `Object.values(state.cards)` in fruits.ts.
    pub cards: BTreeMap<String, CardInstance>,
    pub players: Players,
    pub turn_number: u32,
    /// Whose turn is it? (overall turn count — T1 = player1, T2 = player2, etc.)
    pub current_player: PlayerId,
    pub phase: Phase,
    /// Pending attack waiting for counter response
    pub pending_attack: Option<PendingAttack>,
    pub log: Vec<LogEntry>,
    pub winner: Option<PlayerId>,
    /// Turn of first player (alternates)
    pub first_player: PlayerId,
}

// ------------------------------------------------------------
// All-or-nothing mutation
// ------------------------------------------------------------

/// Run `f` against `state` and roll the state back when it fails.
///
/// The TypeScript engine is written with `immer`: `executeAction`,
/// `declareBaseAttack`, `playEvent`, … are pure `(state, …) => GameState`
/// functions that build a **new** state and only hand it back on success, so a
/// `throw` anywhere inside leaves the caller's state bit-for-bit unchanged —
/// no Volonte spent, no card tapped, nothing half-applied. The Rust port
/// mutates `&mut GameState` in place, so every fallible entry point that would
/// otherwise commit a partial mutation runs its body through this helper to get
/// the same all-or-nothing contract.
///
/// Only `GameState` is restored. The TS module globals (`Math.random`'s
/// stream, `instanceCounter`) are *not* rolled back by a `throw` either, so
/// [`EngineContext`](crate::context::EngineContext) is deliberately left alone.
pub fn transactional<T, F>(state: &mut GameState, f: F) -> Result<T, EngineError>
where
    F: FnOnce(&mut GameState) -> Result<T, EngineError>,
{
    let snapshot = state.clone();
    match f(state) {
        Ok(value) => Ok(value),
        Err(err) => {
            *state = snapshot;
            Err(err)
        }
    }
}

/// Decision §8.25 — how much PV a poison tick actually removes from a unit
/// currently at `current_pv`.
///
/// "Le poison ne peut pas tuer": the tick is clamped to what it takes to bring
/// the unit down to 1 PV, so it can never kill — and, because the clamp is on
/// the *damage* rather than on the resulting PV, it can never heal either. A
/// unit already at or below 1 PV (a burn tick in the same pass may have taken
/// it to 0) takes nothing at all instead of being restored to 1.
fn poison_damage(damage_per_turn: i32, current_pv: i32) -> i32 {
    damage_per_turn.min(current_pv - 1).max(0)
}

/// Decision §8.22 — one modifier-expiry pass, run once per owner turn from
/// [`GameState::reset_turn_flags`].
///
/// * [`ModifierDuration::Turn`] modifiers are dropped outright (unchanged
///   behaviour: they only ever live for the turn that created them);
/// * every other modifier that carries `turns_remaining = Some(n)` has `n`
///   decremented and is dropped when it reaches `0`;
/// * `turns_remaining = None` still means "no countdown" (permanent).
///
/// [`ModifierDuration::NextTurn`] means "expires at the end of the owner's
/// next turn", i.e. a countdown of two owner turns; it is created with
/// `turns_remaining = Some(2)` ([`Modifier::next_turn`]), and a `nextTurn`
/// modifier deserialised without one is normalised here so that the two spell
/// the same rule.
fn tick_modifiers(modifiers: &mut Vec<Modifier>) {
    modifiers.retain_mut(|m| {
        if m.duration == ModifierDuration::Turn {
            return false;
        }
        if m.duration == ModifierDuration::NextTurn && m.turns_remaining.is_none() {
            m.turns_remaining = Some(Modifier::NEXT_TURN_COUNT);
        }
        match m.turns_remaining {
            Some(n) => {
                let left = n - 1;
                m.turns_remaining = Some(left);
                left > 0
            }
            None => true,
        }
    });
}

// ------------------------------------------------------------
// Construction (gameState.ts `createPlayerState` / `createInitialState`,
// init.ts `createGame`)
// ------------------------------------------------------------

/// TS `createPlayerState(playerId, deckDef, allCards)`.
///
/// Builds one `CardInstance` per deck entry copy (deck-list order), shuffles,
/// draws `STARTING_HAND_SIZE` from the top, and creates the captain instance.
/// Ids come from `ctx.generate_instance_id` (the TS module-global counter).
pub fn create_player_state(
    player_id: PlayerId,
    deck_def: &DeckDef,
    registry: &CardRegistry,
    all_cards: &mut BTreeMap<String, CardInstance>,
    ctx: &mut EngineContext,
) -> Result<PlayerState, EngineError> {
    let captain_def = registry.get_captain_def(&deck_def.captain_id)?;

    // Build deck instances
    let mut deck_instance_ids: Vec<String> = Vec::new();
    for entry in &deck_def.cards {
        for _ in 0..entry.count {
            let instance_id = ctx.generate_instance_id(&entry.card_id);
            let card_def = registry.get_card_def(&entry.card_id)?;
            let instance = CardInstance::new(
                instance_id.clone(),
                entry.card_id.clone(),
                player_id,
                card_def.pv.unwrap_or(0),
            );
            all_cards.insert(instance_id.clone(), instance);
            deck_instance_ids.push(instance_id);
        }
    }

    // Shuffle deck
    let mut shuffled_deck = deck_instance_ids;
    ctx.rng.shuffle(&mut shuffled_deck);

    // Draw starting hand (`shuffledDeck.splice(0, STARTING_HAND_SIZE)`)
    let hand: Vec<String> = crate::rng::draw_top_n(&mut shuffled_deck, STARTING_HAND_SIZE);
    for id in &hand {
        if let Some(c) = all_cards.get_mut(id) {
            c.zone = Zone::Hand;
        }
    }

    // Captain instance
    let captain =
        CaptainInstance::new(deck_def.captain_id.clone(), player_id, captain_def.recto.pv);

    Ok(PlayerState {
        id: player_id,
        captain,
        deck: shuffled_deck,
        hand,
        graveyard: Vec::new(),
        board: Board::empty(),
        active_ship: None,
        volonte: 0,
        used_free_move: false,
        has_drawn: false,
        observation_used: false,
        armament_used: false,
        king_used: false,
        ally_ko_ed_this_turn: None,
        char_ko_ed_this_game: None,
        haki_this_turn: None,
        embargo_turns: None,
    })
}

/// TS `createInitialState(p1Deck, p2Deck)` with the module globals injected.
///
/// T1 starts in `main`, `pendingAttack = null`, empty log, no winner,
/// `firstPlayer = player1`. Like the TS function this does NOT run
/// `startTurn` (see [`create_game`]) and does NOT reset the instance counter:
/// a second game built from the same `ctx` keeps numbering where the first
/// one stopped, exactly like the TS module global.
pub fn create_initial_state(
    p1_deck: &DeckDef,
    p2_deck: &DeckDef,
    registry: &CardRegistry,
    ctx: &mut EngineContext,
) -> Result<GameState, EngineError> {
    let mut all_cards: BTreeMap<String, CardInstance> = BTreeMap::new();

    let player1 = create_player_state(PlayerId::Player1, p1_deck, registry, &mut all_cards, ctx)?;
    let player2 = create_player_state(PlayerId::Player2, p2_deck, registry, &mut all_cards, ctx)?;

    Ok(GameState {
        cards: all_cards,
        players: Players { player1, player2 },
        turn_number: 1,
        current_player: PlayerId::Player1,
        phase: Phase::Main, // T1 starts in main (untap/draw/will handled by startTurn)
        pending_attack: None,
        log: Vec::new(),
        winner: None,
        first_player: PlayerId::Player1,
    })
}

/// TS init.ts `createGame(p1Deck, p2Deck)` — the actual game entry point:
/// `createInitialState` then `startTurn` (untap, draw skipped for P1 T1,
/// gain 1 Vol., start-of-turn passives). `initializeRegistry()` is the
/// `registry` argument.
pub fn create_game(
    p1_deck: &DeckDef,
    p2_deck: &DeckDef,
    registry: &CardRegistry,
    ctx: &mut EngineContext,
) -> Result<GameState, EngineError> {
    let mut state = create_initial_state(p1_deck, p2_deck, registry, ctx)?;
    // Start the first turn (untap, draw skipped for P1 T1, gain 1 Vol.)
    state.start_turn(registry, ctx)?;
    Ok(state)
}

// ------------------------------------------------------------
// Accessors / helpers
// ------------------------------------------------------------

impl GameState {
    /// TS `createGame(p1Deck, p2Deck)` from a fresh deterministic context
    /// (`EngineContext::seeded(seed)`).
    pub fn new_game(
        p1_deck: &DeckDef,
        p2_deck: &DeckDef,
        registry: &CardRegistry,
        seed: u64,
    ) -> Result<GameState, EngineError> {
        create_game(p1_deck, p2_deck, registry, &mut EngineContext::seeded(seed))
    }

    /// TS `createGame(p1Deck, p2Deck)` (init.ts) with the registry first —
    /// the shape the hosts and the AI harness use.
    ///
    /// `registry` is what TS `initializeRegistry()` fills the two module
    /// globals with; build it once with
    /// [`cards::registry()`](crate::cards::registry). The body is exactly
    /// `createInitialState(p1Deck, p2Deck)` followed by `startTurn(state)`,
    /// driven by a fresh deterministic [`EngineContext::seeded(seed)`].
    pub fn new_game_from_decks(
        registry: &CardRegistry,
        deck_a: &DeckDef,
        deck_b: &DeckDef,
        seed: u64,
    ) -> Result<GameState, EngineError> {
        let mut ctx = EngineContext::seeded(seed);
        let mut state = create_initial_state(deck_a, deck_b, registry, &mut ctx)?;
        // Start the first turn (untap, draw skipped for P1 T1, gain 1 Vol.)
        state.start_turn(registry, &ctx)?;
        Ok(state)
    }

    /// TS `state.players[id]`.
    pub fn player(&self, id: PlayerId) -> &PlayerState {
        self.players.get(id)
    }

    /// TS `draft.players[id]`.
    pub fn player_mut(&mut self, id: PlayerId) -> &mut PlayerState {
        self.players.get_mut(id)
    }

    /// TS `state.players[state.currentPlayer]`.
    pub fn current_player_state(&self) -> &PlayerState {
        self.players.get(self.current_player)
    }

    /// TS `draft.players[draft.currentPlayer]`.
    pub fn current_player_state_mut(&mut self) -> &mut PlayerState {
        self.players.get_mut(self.current_player)
    }

    /// TS `getOpponent(state.currentPlayer)`.
    pub fn opponent(&self) -> PlayerId {
        self.current_player.opponent()
    }

    /// TS `state.cards[instanceId]` (may be `undefined`).
    pub fn card(&self, instance_id: &str) -> Option<&CardInstance> {
        self.cards.get(instance_id)
    }

    /// TS `draft.cards[instanceId]` (may be `undefined`).
    pub fn card_mut(&mut self, instance_id: &str) -> Option<&mut CardInstance> {
        self.cards.get_mut(instance_id)
    }

    /// TS `const card = state.cards[id]; if (!card) throw new Error("Card not found")`.
    pub fn get_card(&self, instance_id: &str) -> Result<&CardInstance, EngineError> {
        self.cards
            .get(instance_id)
            .ok_or_else(|| EngineError::UnknownInstance(instance_id.to_string()))
    }

    /// Mutable variant of [`GameState::get_card`].
    pub fn get_card_mut(&mut self, instance_id: &str) -> Result<&mut CardInstance, EngineError> {
        self.cards
            .get_mut(instance_id)
            .ok_or_else(|| EngineError::UnknownInstance(instance_id.to_string()))
    }

    /// Register a freshly built instance (`allCards[instanceId] = instance`),
    /// e.g. for tokens deployed by effects. Returns the instance id.
    pub fn add_instance(&mut self, instance: CardInstance) -> String {
        let id = instance.instance_id.clone();
        self.cards.insert(id.clone(), instance);
        id
    }

    /// TS `addLog(state, player, message)`.
    pub fn add_log(&mut self, player: PlayerId, message: impl Into<String>) {
        self.log.push(LogEntry {
            turn: self.turn_number,
            player,
            message: message.into(),
        });
    }

    /// Which player owns the board slot holding `instance_id`, and the slot.
    pub fn find_on_board(&self, instance_id: &str) -> Option<(PlayerId, Slot)> {
        for p in self.players.iter() {
            if let Some(slot) = p.board.slot_of(instance_id) {
                return Some((p.id, slot));
            }
        }
        None
    }

    /// TS `checkWinCondition(state)`.
    pub fn check_win_condition(&self) -> Option<PlayerId> {
        if let Some(w) = self.winner {
            return Some(w);
        }
        let p1_pv = self.players.player1.captain.current_pv;
        let p2_pv = self.players.player2.captain.current_pv;

        if p1_pv <= 0 && p2_pv <= 0 {
            // Both die simultaneously — active player loses
            return Some(self.current_player.opponent());
        }
        if p1_pv <= 0 {
            return Some(PlayerId::Player2);
        }
        if p2_pv <= 0 {
            return Some(PlayerId::Player1);
        }
        None
    }

    // --------------------------------------------------------
    // Turn lifecycle pieces (gameState.ts)
    // --------------------------------------------------------

    /// TS `drawCard(state, playerId)` — draws from the top (`deck.shift()`).
    /// Deck empty ⇒ the player loses (`winner = opponent`).
    pub fn draw_card(&mut self, player_id: PlayerId) {
        let player = self.players.get_mut(player_id);
        if player.deck.is_empty() {
            // Deck empty — player loses at next draw
            self.winner = Some(player_id.opponent());
            return;
        }
        let drawn_id = player.deck.remove(0);
        player.hand.push(drawn_id.clone());
        player.has_drawn = true;
        if let Some(c) = self.cards.get_mut(&drawn_id) {
            c.zone = Zone::Hand;
        }
    }

    /// TS `untapAll(state)` — untap all board characters and the captain of
    /// the current player.
    pub fn untap_all(&mut self) {
        let cp = self.current_player;
        let ids: Vec<String> = self
            .players
            .get(cp)
            .board
            .instance_ids()
            .into_iter()
            .cloned()
            .collect();
        for id in ids {
            if let Some(card) = self.cards.get_mut(&id) {
                card.tapped = false;
            }
        }
        self.players.get_mut(cp).captain.tapped = false;
    }

    /// TS `resetTurnFlags(state)` — per-turn flags of the current player and
    /// their characters, and expiry of `"turn"`-duration modifiers.
    pub fn reset_turn_flags(&mut self) {
        let cp = self.current_player;
        let ids: Vec<String> = self
            .players
            .get(cp)
            .board
            .instance_ids()
            .into_iter()
            .cloned()
            .collect();

        let player = self.players.get_mut(cp);
        player.used_free_move = false;
        player.has_drawn = false;
        player.observation_used = false;
        player.armament_used = false;
        player.ally_ko_ed_this_turn = Some(false);
        player.haki_this_turn = Some(false);

        // Reset character per-turn flags
        for id in &ids {
            if let Some(card) = self.cards.get_mut(id) {
                card.used_base_action = false;
                card.used_special_attack = false;
                card.logia_used_this_turn = Some(false);
            }
        }
        let player = self.players.get_mut(cp);
        player.captain.used_base_action = false;
        player.captain.used_special_attack = false;
        // Decision §8.40: the captain's Logia intangibility is once per turn.
        player.captain.logia_used_this_turn = Some(false);

        // Expire turn-duration modifiers on all player's cards, and tick down
        // the ones that carry a countdown (decision §8.22).
        for id in &ids {
            if let Some(card) = self.cards.get_mut(id) {
                tick_modifiers(&mut card.modifiers);
            }
        }
        let player = self.players.get_mut(cp);
        tick_modifiers(&mut player.captain.modifiers);
    }

    /// TS `processStartOfTurnEffects(state)` — burn / poison / desiccation
    /// ticks and status-effect countdowns for the current player's board and
    /// captain. `selfKO` is left untouched (handled by a dedicated pass in
    /// `startTurn`).
    ///
    /// Decisions §8.24 (a `turnsRemaining` of `0` is already expired: it deals
    /// nothing and is dropped here; `-1` is permanent for every status type)
    /// and §8.25 (the poison floor is applied to the poison damage alone, see
    /// [`poison_damage`]).
    pub fn process_start_of_turn_effects(&mut self) {
        let cp = self.current_player;
        let ids: Vec<String> = self
            .players
            .get(cp)
            .board
            .instance_ids()
            .into_iter()
            .cloned()
            .collect();

        // Process status effects on board characters
        for id in &ids {
            let Some(card) = self.cards.get_mut(id) else {
                continue;
            };
            let mut remaining: Vec<StatusEffect> = Vec::new();
            for mut effect in std::mem::take(&mut card.status_effects) {
                // selfKO is handled by a dedicated pass in startTurn — keep it untouched here.
                if effect.effect_type == StatusEffectType::SelfKo {
                    remaining.push(effect);
                    continue;
                }
                // Decision §8.24: `0` means "already expired" — no damage, and
                // the effect is dropped by this very tick.
                if effect.turns_remaining == 0 {
                    continue;
                }
                // Apply damage
                if effect.effect_type == StatusEffectType::Burn {
                    card.current_pv -= effect.damage_per_turn;
                }
                if effect.effect_type == StatusEffectType::Poison {
                    // Decision §8.25: "le poison ne peut pas tuer" is a
                    // property of the poison tick alone — it stops at 1 PV and
                    // can never *raise* the PV of a unit another effect has
                    // already finished off.
                    card.current_pv -= poison_damage(effect.damage_per_turn, card.current_pv);
                }
                if effect.effect_type == StatusEffectType::Desiccation {
                    card.current_pv -= effect.damage_per_turn;
                }
                // Decrement turns
                if effect.turns_remaining > 0 {
                    effect.turns_remaining -= 1;
                    if effect.turns_remaining > 0 {
                        remaining.push(effect);
                    }
                } else if effect.turns_remaining == -1 {
                    // Permanent (poison)
                    remaining.push(effect);
                }
            }
            card.status_effects = remaining;
        }

        // Process captain status effects
        let cap = &mut self.players.get_mut(cp).captain;
        let mut cap_remaining: Vec<StatusEffect> = Vec::new();
        for mut effect in std::mem::take(&mut cap.status_effects) {
            // Decision §8.24 — as above: a 0-turn status is already expired.
            if effect.turns_remaining == 0 {
                continue;
            }
            if effect.effect_type == StatusEffectType::Burn {
                cap.current_pv -= effect.damage_per_turn;
            }
            if effect.effect_type == StatusEffectType::Poison {
                // Decision §8.25 — same floor, same no-resurrection rule.
                cap.current_pv -= poison_damage(effect.damage_per_turn, cap.current_pv);
            }
            if effect.turns_remaining > 0 {
                effect.turns_remaining -= 1;
                if effect.turns_remaining > 0 {
                    cap_remaining.push(effect);
                }
            } else if effect.turns_remaining == -1 {
                cap_remaining.push(effect);
            }
        }
        cap.status_effects = cap_remaining;
    }

    /// TS `startTurn` step 3: J1 does NOT draw on T1.
    pub fn is_first_player_first_turn(&self) -> bool {
        self.turn_number == 1 && self.current_player == self.first_player
    }

    /// TS `startTurn(state)` — start a new turn for the current player:
    /// untap → reset flags → draw (J1 skips on T1) → gain Volonte → phase
    /// `main` → Sandai Kitetsu curse → status ticks → selfKO timers → KO
    /// check → start-of-turn passives → passive buffs / enemy debuff auras →
    /// win check (decision §8.44).
    pub fn start_turn(
        &mut self,
        registry: &CardRegistry,
        ctx: &EngineContext,
    ) -> Result<(), EngineError> {
        // 1. Untap all characters
        self.untap_all();

        // 2. Reset per-turn flags
        self.reset_turn_flags();

        // 3. Draw 1 card (J1 does NOT draw on T1)
        if !self.is_first_player_first_turn() {
            self.draw_card(self.current_player);
        }

        // 4. Gain Volonte
        self.gain_volonte();

        // 5. Set phase to main
        self.phase = Phase::Main;

        // 5b. Cursed weapon (Sandai Kitetsu): the bearer takes 1 damage at the start of the turn.
        {
            let cp = self.current_player;
            let ids: Vec<String> = self.board_ids(cp);
            for id in ids {
                let Some(card) = self.cards.get(&id) else {
                    continue;
                };
                let has_sandai = card
                    .attached_objects
                    .iter()
                    .any(|oid| self.cards.get(oid).is_some_and(|o| o.def_id == "MG-010"));
                if has_sandai {
                    if let Some(card) = self.cards.get_mut(&id) {
                        card.current_pv -= 1;
                    }
                    self.add_log(cp, "Malédiction du Sandai Kitetsu : 1 dégât.");
                }
            }
        }

        // 6. Process start-of-turn effects (burn, poison, etc.)
        self.process_start_of_turn_effects();

        // 6a. Self-KO timers (Chopper Monster Point) — KO without granting the opponent +2 Vol.
        {
            let cp = self.current_player;
            let ids: Vec<String> = self.board_ids(cp);
            for id in ids {
                let Some(card) = self.cards.get(&id) else {
                    continue;
                };
                let Some(sk) = card.status(StatusEffectType::SelfKo) else {
                    continue;
                };
                if sk.turns_remaining <= 1 {
                    let name = registry.get_card_def(&card.def_id)?.name.clone();
                    self.add_log(cp, format!("{name} retombe (fin de transformation) — KO."));
                    remove_from_board(self, registry, &id)?;
                } else if let Some(c) = self.cards.get_mut(&id) {
                    if let Some(e) = c
                        .status_effects
                        .iter_mut()
                        .find(|x| x.effect_type == StatusEffectType::SelfKo)
                    {
                        e.turns_remaining -= 1;
                    }
                }
            }
        }

        // 6b. Check for KO from burn/desiccation damage
        let current_player_id = self.current_player;
        let ids: Vec<String> = self.board_ids(current_player_id);
        for id in ids {
            let Some(card) = self.cards.get(&id) else {
                continue;
            };
            if card.zone == Zone::Board && card.current_pv <= 0 {
                let card_def = registry.get_card_def(&card.def_id)?;
                let ko_owner = card.owner;
                let ko_def_id = card.def_id.clone();
                let name = card_def.name.clone();
                self.add_log(
                    current_player_id,
                    format!("{name} est KO (brulure/effet) !"),
                );
                // Burn/poison were inflicted by the opponent: the owner who lost the ally gets +2 Vol (Rulebook v3.1 §4).
                self.grant_ko_bonus(ko_owner);
                remove_from_board(self, registry, &id)?;
                apply_on_ko_effects(
                    self,
                    registry,
                    ctx,
                    ko_owner,
                    ko_owner.opponent(),
                    &ko_def_id,
                )?;
            }
        }

        // 7. Apply start-of-turn passives (healAdjacent, etc.)
        let cp = self.current_player;
        apply_start_of_turn_passives(self, registry, ctx, cp)?;

        // 8. Recalculate passive buffs (captain, synergies)
        recalculate_passive_buffs(self, registry, cp)?;
        recalculate_passive_buffs(self, registry, cp.opponent())?;
        apply_enemy_debuff_auras(self, registry)?;

        // 9. Decision §8.44: a captain that died to a burn / desiccation tick,
        // or a deck-out on the start-of-turn draw, ends the game right here —
        // `check_win_condition` returns the existing `winner` first, so this is
        // idempotent and can never overwrite an earlier one.
        if let Some(w) = self.check_win_condition() {
            self.winner = Some(w);
        }

        Ok(())
    }

    /// TS `endTurn(state)` — end the current player's turn and switch to the
    /// opponent. First the Crocodile end-of-turn desiccation (the **active
    /// face's** `endTurnDesiccation` — verso when flipped, recto otherwise:
    /// the first injured enemy loses N permanent PV, KO if it drops to 0;
    /// decision §8.17), then phase `end`, hand-limit
    /// discard and the player switch ([`GameState::end_turn_switch`]).
    ///
    /// Takes the [`EngineContext`] for the same reason
    /// [`GameState::start_turn`] does: a desiccation KO runs the standard KO
    /// chain (§8.12/§8.17), and the on-KO triggers mint modifier ids from
    /// `ctx.now()`.
    pub fn end_turn(
        &mut self,
        registry: &CardRegistry,
        ctx: &EngineContext,
    ) -> Result<(), EngineError> {
        // End-of-turn desiccation (Crocodile): an injured enemy loses 1 permanent PV.
        {
            let me = self.current_player;
            let opp = me.opponent();
            let captain = &self.players.get(me).captain;
            let cap_def = registry.get_captain_def(&captain.def_id)?;
            let cap_passive = if captain.flipped {
                &cap_def.verso.passive
            } else {
                &cap_def.recto.passive
            };
            let desicc: i32 = cap_passive
                .effects
                .iter()
                .map(|e| match e {
                    PassiveEffect::EndTurnDesiccation { amount } => *amount,
                    _ => 0,
                })
                .sum();
            // Decision §8.17: the face selection above *is* the rule — the
            // extra `captain.flipped` test made the recto branch unreachable.
            if desicc > 0 {
                let mut injured: Vec<String> = Vec::new();
                for id in self.players.get(opp).board.instance_ids() {
                    let Some(c) = self.cards.get(id) else {
                        continue;
                    };
                    let def = registry.get_card_def(&c.def_id)?;
                    if def.pv.is_some() && c.current_pv < def.pv.unwrap_or(0) {
                        injured.push(id.clone());
                    }
                }
                if let Some(tid) = injured.first().cloned() {
                    let card = self.get_card_mut(&tid)?;
                    card.current_pv -= desicc;
                    let name = registry.get_card_def(&card.def_id)?.name.clone();
                    self.add_log(
                        me,
                        format!("Déshydratation : {name} perd {desicc} PV permanent."),
                    );
                    if self.get_card(&tid)?.current_pv <= 0 {
                        // Decision §8.17, on the §8.11/§8.12 principle: a
                        // desiccation KO is a KO like any other. It was the
                        // last damage source in the engine whose KO skipped
                        // the chain — no log, no +2 Volonté for the player who
                        // lost the character (Rulebook v3.1 §4), no on-KO
                        // triggers (`allyKOedThisTurn`, `selfBuffOnAllyKO`,
                        // the synergy rage). Every sibling path — the ship
                        // broadside, the trap, captain thunder, the Épines
                        // recoil — sweeps its KOs, and this one is live in
                        // every game the Baroque deck plays.
                        let ko_owner = self.get_card(&tid)?.owner;
                        let ko_def_id = self.get_card(&tid)?.def_id.clone();
                        self.add_log(ko_owner, format!("{name} est KO (Déshydratation) !"));
                        self.grant_ko_bonus(ko_owner);
                        remove_from_board(self, registry, &tid)?;
                        apply_on_ko_effects(
                            self,
                            registry,
                            ctx,
                            ko_owner,
                            ko_owner.opponent(),
                            &ko_def_id,
                        )?;
                    }
                }
            }
        }

        // Decision §8.37 (`betrayal`, `BW-024`): every body on loan to the
        // player whose turn is ending goes home to `loanReturnSlot` — free by
        // construction, since only the borrower acted in between. `owner` was
        // never touched, so nothing else has to be undone.
        self.return_loans(registry)?;

        // Decision §8.37 (`embargo`, `MR-027`): the ban lasts exactly one of
        // the embargoed player's own turns.
        {
            let me = self.current_player;
            let player = self.players.get_mut(me);
            if let Some(turns) = player.embargo_turns {
                player.embargo_turns = if turns > 1 { Some(turns - 1) } else { None };
            }
        }

        self.end_turn_switch();
        Ok(())
    }

    /// Decision §8.37 (`betrayal`): hand every borrowed character back to its
    /// owner at the borrower's end of turn. A body that was KO'd while on loan
    /// is already in the graveyard — only the two loan fields (and a stale
    /// board cell) are cleaned up for it.
    fn return_loans(&mut self, registry: &CardRegistry) -> Result<(), EngineError> {
        let me = self.current_player;
        let borrowed: Vec<String> = self
            .cards
            .iter()
            .filter(|(_, c)| c.controlled_by == Some(me))
            .map(|(id, _)| id.clone())
            .collect();
        if borrowed.is_empty() {
            return Ok(());
        }
        for id in borrowed {
            let Some(card) = self.cards.get(&id) else {
                continue;
            };
            let owner = card.owner;
            let home = card.loan_return_slot;
            let here = card.slot;
            let on_board = card.zone == Zone::Board;

            // Where the body lands back home. The slot it left is *not*
            // guaranteed free: decision §8.38 made `AttackTrait::Impact` imply
            // a pushback, so the borrower's own attacks can shove one of the
            // owner's characters from V* into the A* cell the loan vacated.
            // Overwriting the cell there would orphan that occupant (still
            // `zone == Board` with a slot, but in no board cell), so a taken
            // home slot falls back to the first free slot of the owner's board
            // — the body comes back to its camp, just not to its old spot.
            let landing = if on_board {
                home.filter(|s| crate::board::is_slot_free(self, owner, *s))
                    .or_else(|| crate::board::get_empty_slots(self, owner).first().copied())
            } else {
                None
            };

            if on_board && landing.is_none() {
                // The owner's board is somehow completely full — the loan left
                // one cell and nothing on the borrower's turn can refill an
                // opponent's board, so this arm is unreachable on the shipped
                // catalogue. Decision §8.37 follow-up (b) authorises exactly
                // one thing here: "the body leaves the board through
                // `remove_from_board` instead of squatting in the borrower's
                // camp". It is a *homeless* loan, not a KO — no +2 Volonté, no
                // on-KO triggers, no log line — because nothing in §8 turns a
                // failed hand-back into a kill, and inventing one would move
                // Volonté and fire `onPartnerKO` chains for an event no card
                // describes.
                crate::board::remove_from_board(self, registry, &id)?;
                if let Some(card) = self.cards.get_mut(&id) {
                    card.controlled_by = None;
                    card.loan_return_slot = None;
                }
                continue;
            }

            if let Some(slot) = here {
                if self.players.get(me).board.get(slot) == Some(&id) {
                    self.players.get_mut(me).board.set(slot, None);
                }
            }
            if let Some(dest) = landing {
                self.players
                    .get_mut(owner)
                    .board
                    .set(dest, Some(id.clone()));
            }

            let card = self.cards.get_mut(&id).expect("just read");
            card.controlled_by = None;
            card.loan_return_slot = None;
            let attached = card.attached_objects.clone();
            if on_board {
                // The borrowed body spent its turn away from home: it comes
                // back tapped-out exactly as it left, but its slot is its own
                // again.
                card.slot = landing;
                // Decision §8.29: the equipment travels with its bearer — the
                // landing cell is often *not* the one the loan left (the free
                // slot fallback above), so the attachments follow the body
                // rather than keeping the borrower's cell.
                if let Some(dest) = landing {
                    crate::board::move_attached_objects(self, &attached, dest);
                }
            }
            if on_board {
                let def_id = self.cards[&id].def_id.clone();
                let name = registry.get_card_def(&def_id)?.name.clone();
                self.add_log(me, format!("{name} retourne dans son camp."));
            }
        }
        recalculate_passive_buffs(self, registry, me)?;
        recalculate_passive_buffs(self, registry, me.opponent())?;
        apply_enemy_debuff_auras(self, registry)?;
        Ok(())
    }

    /// The occupant ids of `player_id`'s board in slot order
    /// (TS `Object.values(player.board)` with the `null`s skipped), cloned so
    /// the caller can mutate the state while iterating.
    fn board_ids(&self, player_id: PlayerId) -> Vec<String> {
        self.players
            .get(player_id)
            .board
            .instance_ids()
            .into_iter()
            .cloned()
            .collect()
    }

    /// TS `endTurn` second half (inside `produce`): phase = end, hand-limit
    /// discard, switch player, bump `turnNumber` when player2 ends.
    pub fn end_turn_switch(&mut self) {
        self.phase = Phase::End;
        self.discard_hand_overflow();

        // Switch player
        let next_player = self.current_player.opponent();
        // If switching FROM player2 back to player1, increment turn number
        if self.current_player == PlayerId::Player2 {
            self.turn_number += 1;
        }
        self.current_player = next_player;
    }

    /// TS `endTurn` hand limit: discard the excess at end of turn
    /// (Rulebook v3.1 §10). Auto-discards the oldest cards (front of the hand).
    pub fn discard_hand_overflow(&mut self) {
        let cp = self.current_player;
        let turn = self.turn_number;
        let ending = self.players.get_mut(cp);
        if ending.hand.len() > HAND_LIMIT {
            let overflow = ending.hand.len() - HAND_LIMIT;
            let discarded: Vec<String> = ending.hand.drain(..overflow).collect();
            for id in discarded {
                if let Some(c) = self.cards.get_mut(&id) {
                    c.zone = Zone::Graveyard;
                }
                self.players.get_mut(cp).graveyard.push(id);
            }
            self.log.push(LogEntry {
                turn,
                player: cp,
                message: format!("Limite de main : {overflow} carte(s) défaussée(s)."),
            });
        }
    }

    // --------------------------------------------------------
    // Volonte (volonte.ts)
    // --------------------------------------------------------

    /// TS `gainVolonte(state)` — amount = turn number (cap 10); previous Vol.
    /// is lost, replaced by the new amount.
    pub fn gain_volonte(&mut self) {
        let amount = (self.turn_number as i32).min(VOLONTE_CAP);
        self.current_player_state_mut().volonte = amount;
    }

    /// TS `spendVolonte(state, playerId, amount)` — no-op for `amount <= 0`,
    /// `Err(NotEnoughVolonte)` (`Not enough Volonte: has X, needs Y`) if
    /// insufficient.
    pub fn spend_volonte(&mut self, player_id: PlayerId, amount: i32) -> Result<(), EngineError> {
        if amount <= 0 {
            return Ok(());
        }
        let player = self.players.get_mut(player_id);
        if player.volonte < amount {
            return Err(EngineError::NotEnoughVolonte {
                has: player.volonte,
                needs: amount,
            });
        }
        player.volonte -= amount;
        Ok(())
    }

    /// TS `canAfford(state, playerId, cost)`.
    pub fn can_afford(&self, player_id: PlayerId, cost: i32) -> bool {
        self.players.get(player_id).volonte >= cost
    }

    /// TS `grantKOBonus(state, playerId)` — +2 Vol and `allyKOedThisTurn = true`.
    pub fn grant_ko_bonus(&mut self, player_id: PlayerId) {
        let player = self.players.get_mut(player_id);
        player.volonte += ALLY_KO_BONUS_VOL;
        // The player lost an ally to the opponent this turn (enables Luffy's free flip).
        player.ally_ko_ed_this_turn = Some(true);
    }
}
