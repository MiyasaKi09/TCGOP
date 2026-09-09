//! Devil Fruits — port of `src/engine/fruits.ts`.
//!
//! A fruit is an `object` card with `subtype: "fruit"` and structured
//! `fruitEffects`. Equipping it runs [`apply_fruit_base_effects`]; from
//! `awakening.minTurns` on, the legitimate bearer may pay `awakening.volCost`
//! to [`awaken_fruit`], which unlocks the extra traits, stat bonuses and the
//! awakening special attack used by
//! [`crate::combat::declare_fruit_special_attack`].

use crate::error::EngineError;
use crate::passives::recalculate_passive_buffs;
use crate::registry::CardRegistry;
use crate::state::{CardInstance, GameState};
use crate::types::{
    Modifier, ModifierDuration, ModifierStat, ObjectSubtype, PlayerId, Trait, Zone,
};

/// TS `Object.values(state.cards).find((c) => c.zone === "board" && c.owner === playerId
/// && c.attachedObjects.includes(fruitInstanceId))` (fruits.ts:80 and :116).
///
/// `state.cards` is a `BTreeMap`, so the scan is deterministic (a fruit can
/// only ever be attached to a single character anyway).
fn find_bearer<'a>(
    state: &'a GameState,
    player_id: PlayerId,
    fruit_instance_id: &str,
) -> Option<&'a CardInstance> {
    state.cards.values().find(|c| {
        c.zone == Zone::Board
            && c.owner == player_id
            && c.attached_objects.iter().any(|id| id == fruit_instance_id)
    })
}

/// TS `applyFruitBaseEffects(state, fruitInstanceId, bearerInstanceId)`
/// — `src/engine/fruits.ts:12`.
///
/// Pushes the permanent `` `fruit_atk_{fruitInstanceId}` `` / `` `fruit_def_…` ``
/// modifiers (source `` `fruit_{fruitDefId}` ``) — but note the TS **quirk**:
/// both are inside `if (base.grantsTraits)`, so a fruit without granted traits
/// gets no stat bonus at all. Always logs
/// `"{bearerName} mange le {fruitName} ! {base.passiveDescription ?? ""}"`
/// (trailing space kept when the description is absent).
/// A missing fruit instance or a fruit without `fruitEffects` is a silent no-op.
pub fn apply_fruit_base_effects(
    state: &mut GameState,
    registry: &CardRegistry,
    fruit_instance_id: &str,
    bearer_instance_id: &str,
) -> Result<(), EngineError> {
    // const fruitCard = state.cards[fruitInstanceId]; if (!fruitCard) return state;
    let Some(fruit_card) = state.cards.get(fruit_instance_id) else {
        return Ok(());
    };
    let fruit_owner = fruit_card.owner;
    let fruit_def_id = fruit_card.def_id.clone();

    // const fruitDef = getCardDef(fruitCard.defId);
    let fruit_def = registry.get_card_def(&fruit_def_id)?;
    // if (!fruitDef.fruitEffects) return state;
    let Some(fruit_effects) = fruit_def.fruit_effects.as_ref() else {
        return Ok(());
    };
    let base = &fruit_effects.base;
    // JS truthiness: `[]` is truthy, so an empty (but present) list still gates in.
    let grants_traits = base.grants_traits.is_some();
    // JS truthiness: `0` is falsy, so a zero bonus pushes no modifier.
    let atk_bonus = base.atk_bonus.unwrap_or(0);
    let def_bonus = base.def_bonus.unwrap_or(0);
    let passive_description = base.passive_description.clone().unwrap_or_default();
    let fruit_name = fruit_def.name.clone();
    let fruit_source = format!("fruit_{}", fruit_def.id);

    // const bearerDef = getCardDef(state.cards[bearerInstanceId].defId);
    // (TS throws a TypeError when the bearer instance is missing.)
    let bearer_card = state
        .cards
        .get(bearer_instance_id)
        .ok_or_else(|| EngineError::UnknownInstance(bearer_instance_id.to_string()))?;
    let bearer_name = registry.get_card_def(&bearer_card.def_id)?.name.clone();

    // Apply granted traits via modifiers
    if grants_traits {
        if let Some(bearer) = state.cards.get_mut(bearer_instance_id) {
            if atk_bonus != 0 {
                bearer.modifiers.push(Modifier {
                    id: format!("fruit_atk_{fruit_instance_id}"),
                    stat: ModifierStat::Atk,
                    amount: atk_bonus,
                    source: fruit_source.clone(),
                    duration: ModifierDuration::Permanent,
                    turns_remaining: None,
                });
            }
            if def_bonus != 0 {
                bearer.modifiers.push(Modifier {
                    id: format!("fruit_def_{fruit_instance_id}"),
                    stat: ModifierStat::Def,
                    amount: def_bonus,
                    source: fruit_source,
                    duration: ModifierDuration::Permanent,
                    turns_remaining: None,
                });
            }
        }
    }

    state.add_log(
        fruit_owner,
        format!("{bearer_name} mange le {fruit_name} ! {passive_description}"),
    );

    Ok(())
}

/// TS `canAwakenFruit(state, playerId, fruitInstanceId)`
/// — `src/engine/fruits.ts:65`.
///
/// False when the fruit is missing or already awakened, when it has no
/// `awakening`, when no board character of `playerId` has it attached, when the
/// bearer's name does not `includes(awakening.porteurLegitime)`, when
/// `turnNumber < awakening.minTurns`, or when `volCost` is unaffordable.
///
/// The bearer search is TS `Object.values(state.cards).find(...)` — reproduce it
/// over the `BTreeMap` in key order (deterministic; a fruit can only be attached
/// to one character anyway).
pub fn can_awaken_fruit(
    state: &GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
    fruit_instance_id: &str,
) -> Result<bool, EngineError> {
    // if (!fruitCard || fruitCard.isAwakened) return false;
    let Some(fruit_card) = state.cards.get(fruit_instance_id) else {
        return Ok(false);
    };
    if fruit_card.is_awakened.unwrap_or(false) {
        return Ok(false);
    }

    // if (!fruitDef.fruitEffects?.awakening) return false;
    let fruit_def = registry.get_card_def(&fruit_card.def_id)?;
    let Some(awakening) = fruit_def
        .fruit_effects
        .as_ref()
        .and_then(|f| f.awakening.as_ref())
    else {
        return Ok(false);
    };

    // Check bearer is the legitimate one
    let Some(bearer) = find_bearer(state, player_id, fruit_instance_id) else {
        return Ok(false);
    };

    let bearer_def = registry.get_card_def(&bearer.def_id)?;
    // JS truthiness on the string: an empty `porteurLegitime` skips the check.
    if !awakening.porteur_legitime.is_empty()
        && !bearer_def.name.contains(&awakening.porteur_legitime)
    {
        return Ok(false);
    }

    // Check minimum turns
    if state.turn_number < awakening.min_turns {
        return Ok(false);
    }

    // Check Vol cost
    if !state.can_afford(player_id, awakening.vol_cost) {
        return Ok(false);
    }

    Ok(true)
}

/// TS `awakenFruit(state, playerId, fruitInstanceId)` — `src/engine/fruits.ts:102`.
///
/// Pays `awakening.volCost`, sets `isAwakened`, pushes the permanent
/// `` `fruit_awaken_atk_{fruitInstanceId}` `` / `` `fruit_awaken_def_…` ``
/// modifiers (source `` `fruit_awaken_{fruitDefId}` ``), logs
/// `"⭐ EVEIL ! {bearerName} eveille le {fruitName} ! {awakening.passiveDescription ?? ""}"`
/// and finally calls `recalculate_passive_buffs(playerId)`.
///
/// Errors: `Cannot awaken this fruit`, `No bearer found`.
pub fn awaken_fruit(
    state: &mut GameState,
    registry: &CardRegistry,
    player_id: PlayerId,
    fruit_instance_id: &str,
) -> Result<(), EngineError> {
    if !can_awaken_fruit(state, registry, player_id, fruit_instance_id)? {
        return Err(EngineError::illegal("Cannot awaken this fruit"));
    }

    let fruit_card = state
        .cards
        .get(fruit_instance_id)
        .ok_or_else(|| EngineError::UnknownInstance(fruit_instance_id.to_string()))?;
    let fruit_def = registry.get_card_def(&fruit_card.def_id)?;
    let awakening = fruit_def
        .fruit_effects
        .as_ref()
        .and_then(|f| f.awakening.as_ref())
        .ok_or_else(|| EngineError::illegal("Cannot awaken this fruit"))?;
    let vol_cost = awakening.vol_cost;
    // JS truthiness: `0` is falsy, so a zero bonus pushes no modifier.
    let atk_bonus = awakening.atk_bonus.unwrap_or(0);
    let def_bonus = awakening.def_bonus.unwrap_or(0);
    let passive_description = awakening.passive_description.clone().unwrap_or_default();
    let fruit_name = fruit_def.name.clone();
    let awaken_source = format!("fruit_awaken_{}", fruit_def.id);

    // Find bearer
    let Some(bearer) = find_bearer(state, player_id, fruit_instance_id) else {
        return Err(EngineError::illegal("No bearer found"));
    };
    let bearer_instance_id = bearer.instance_id.clone();
    let bearer_name = registry.get_card_def(&bearer.def_id)?.name.clone();

    state.spend_volonte(player_id, vol_cost)?;

    // Mark as awakened
    if let Some(fruit) = state.cards.get_mut(fruit_instance_id) {
        fruit.is_awakened = Some(true);
    }

    // Apply awakening bonuses
    if let Some(b) = state.cards.get_mut(&bearer_instance_id) {
        if atk_bonus != 0 {
            b.modifiers.push(Modifier {
                id: format!("fruit_awaken_atk_{fruit_instance_id}"),
                stat: ModifierStat::Atk,
                amount: atk_bonus,
                source: awaken_source.clone(),
                duration: ModifierDuration::Permanent,
                turns_remaining: None,
            });
        }
        if def_bonus != 0 {
            b.modifiers.push(Modifier {
                id: format!("fruit_awaken_def_{fruit_instance_id}"),
                stat: ModifierStat::Def,
                amount: def_bonus,
                source: awaken_source,
                duration: ModifierDuration::Permanent,
                turns_remaining: None,
            });
        }
    }

    state.add_log(
        player_id,
        format!("⭐ EVEIL ! {bearer_name} eveille le {fruit_name} ! {passive_description}"),
    );

    recalculate_passive_buffs(state, registry, player_id)?;

    Ok(())
}

/// TS `getFruitTraits(state, instanceId)` — `src/engine/fruits.ts:164`.
///
/// The traits granted by the character's equipped fruits, in
/// `attachedObjects` order: `base.grantsTraits` always, plus
/// `awakening.grantsTraits` once the fruit is awakened. Duplicates are kept,
/// exactly like the TS `push(...)`.
pub fn get_fruit_traits(
    state: &GameState,
    registry: &CardRegistry,
    instance_id: &str,
) -> Result<Vec<Trait>, EngineError> {
    // const card = state.cards[instanceId]; if (!card) return [];
    let Some(card) = state.cards.get(instance_id) else {
        return Ok(Vec::new());
    };

    let mut traits: Vec<Trait> = Vec::new();
    for obj_id in &card.attached_objects {
        let Some(obj_card) = state.cards.get(obj_id) else {
            continue;
        };
        let obj_def = registry.get_card_def(&obj_card.def_id)?;
        // if (objDef.subtype !== "fruit" || !objDef.fruitEffects) continue;
        if obj_def.subtype != Some(ObjectSubtype::Fruit) {
            continue;
        }
        let Some(fruit_effects) = obj_def.fruit_effects.as_ref() else {
            continue;
        };

        let base = &fruit_effects.base;
        if let Some(granted) = base.grants_traits.as_ref() {
            traits.extend(granted.iter().copied());
        }

        // If awakened, add awakening traits
        if obj_card.is_awakened.unwrap_or(false) {
            if let Some(granted) = fruit_effects
                .awakening
                .as_ref()
                .and_then(|a| a.grants_traits.as_ref())
            {
                traits.extend(granted.iter().copied());
            }
        }
    }

    Ok(traits)
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Board, CaptainInstance, GameState, PlayerState, Players};
    use crate::types::{
        BaseAction, CaptainDef, CaptainRecto, CaptainVerso, CardDef, CardType, EntryEffect,
        Faction, FlipCondition, FruitAwakening, FruitBaseEffects, FruitEffects, PassiveDef, Phase,
        Rarity, Slot, SpecialAttack,
    };
    use std::collections::BTreeMap;

    fn passive() -> PassiveDef {
        PassiveDef {
            name: "P".into(),
            description: "d".into(),
            effects: Vec::new(),
        }
    }

    fn captain_def() -> CaptainDef {
        CaptainDef {
            id: "cap_test".into(),
            name: "Cap".into(),
            faction: Faction::Pirate,
            tags: None,
            traits: None,
            recto: CaptainRecto {
                pv: 10,
                atk: 3,
                def: 2,
                passive: passive(),
                attacks: Vec::new(),
                surcharge: None,
            },
            flip_condition: FlipCondition::default(),
            verso: CaptainVerso {
                pv: 12,
                atk: 4,
                def: 3,
                passive: passive(),
                entry_effect: EntryEffect::Draw { amount: 1 },
                base_action: BaseAction::default(),
                special_attack: SpecialAttack::default(),
                surcharge: None,
                traits: None,
                natural_haki: None,
            },
        }
    }

    fn character_def(id: &str, name: &str) -> CardDef {
        let mut def = CardDef::new(
            id,
            name,
            CardType::Character,
            3,
            Faction::Pirate,
            Rarity::C,
            "TEST",
        );
        def.atk = Some(3);
        def.def = Some(2);
        def.pv = Some(4);
        def
    }

    /// A fruit close to the real Gomu Gomu no Mi shape.
    fn fruit_def(id: &str, name: &str, effects: FruitEffects) -> CardDef {
        let mut def = CardDef::new(
            id,
            name,
            CardType::Object,
            2,
            Faction::Pirate,
            Rarity::R,
            "TEST",
        );
        def.subtype = Some(ObjectSubtype::Fruit);
        def.fruit_effects = Some(effects);
        def
    }

    fn full_effects() -> FruitEffects {
        FruitEffects {
            base: FruitBaseEffects {
                grants_traits: Some(vec![Trait::Cursed, Trait::Logia]),
                passive_description: Some("Corps elastique".into()),
                atk_bonus: Some(1),
                def_bonus: Some(2),
            },
            awakening: Some(FruitAwakening {
                porteur_legitime: "Luffy".into(),
                min_turns: 5,
                vol_cost: 4,
                grants_traits: Some(vec![Trait::Conqueror]),
                atk_bonus: Some(3),
                def_bonus: Some(1),
                passive_description: Some("Nika".into()),
                special_attack: None,
            }),
        }
    }

    fn player(id: PlayerId) -> PlayerState {
        PlayerState {
            id,
            captain: CaptainInstance::new("cap_test".into(), id, 10),
            deck: Vec::new(),
            hand: Vec::new(),
            graveyard: Vec::new(),
            board: Board::empty(),
            active_ship: None,
            volonte: 10,
            used_free_move: false,
            has_drawn: false,
            observation_used: false,
            armament_used: false,
            king_used: false,
            ally_ko_ed_this_turn: None,
            char_ko_ed_this_game: None,
            haki_this_turn: None,
        }
    }

    /// A board with `char1` (Monkey D. Luffy) in V1 holding `fruit1`.
    fn setup(effects: FruitEffects) -> (GameState, CardRegistry) {
        let mut reg = CardRegistry::new();
        reg.register_card(character_def("ch_luffy", "Monkey D. Luffy"));
        reg.register_card(character_def("ch_zoro", "Roronoa Zoro"));
        reg.register_card(fruit_def("obj_gomu", "Gomu Gomu no Mi", effects));
        reg.register_captain(captain_def());

        let mut cards: BTreeMap<String, CardInstance> = BTreeMap::new();
        let mut bearer = CardInstance::new("char1".into(), "ch_luffy".into(), PlayerId::Player1, 4);
        bearer.zone = Zone::Board;
        bearer.slot = Some(Slot::V1);
        bearer.attached_objects = vec!["fruit1".into()];
        cards.insert("char1".into(), bearer);

        let mut fruit = CardInstance::new("fruit1".into(), "obj_gomu".into(), PlayerId::Player1, 0);
        fruit.zone = Zone::Board;
        cards.insert("fruit1".into(), fruit);

        let mut p1 = player(PlayerId::Player1);
        p1.board.set(Slot::V1, Some("char1".into()));

        let state = GameState {
            cards,
            players: Players {
                player1: p1,
                player2: player(PlayerId::Player2),
            },
            turn_number: 6,
            current_player: PlayerId::Player1,
            phase: Phase::Main,
            pending_attack: None,
            log: Vec::new(),
            winner: None,
            first_player: PlayerId::Player1,
        };
        (state, reg)
    }

    #[test]
    fn base_effects_push_modifiers_and_log() {
        let (mut state, reg) = setup(full_effects());
        apply_fruit_base_effects(&mut state, &reg, "fruit1", "char1").unwrap();

        let bearer = &state.cards["char1"];
        assert_eq!(bearer.modifiers.len(), 2);
        assert_eq!(bearer.modifiers[0].id, "fruit_atk_fruit1");
        assert_eq!(bearer.modifiers[0].amount, 1);
        assert_eq!(bearer.modifiers[0].source, "fruit_obj_gomu");
        assert_eq!(bearer.modifiers[0].stat, ModifierStat::Atk);
        assert_eq!(bearer.modifiers[0].duration, ModifierDuration::Permanent);
        assert_eq!(bearer.modifiers[1].id, "fruit_def_fruit1");
        assert_eq!(bearer.modifiers[1].amount, 2);

        assert_eq!(
            state.log.last().unwrap().message,
            "Monkey D. Luffy mange le Gomu Gomu no Mi ! Corps elastique"
        );
        assert_eq!(state.log.last().unwrap().player, PlayerId::Player1);
    }

    /// TS quirk: both stat bonuses live inside `if (base.grantsTraits)`.
    #[test]
    fn base_effects_without_granted_traits_skip_stat_bonuses() {
        let effects = FruitEffects {
            base: FruitBaseEffects {
                grants_traits: None,
                passive_description: None,
                atk_bonus: Some(5),
                def_bonus: Some(5),
            },
            awakening: None,
        };
        let (mut state, reg) = setup(effects);
        apply_fruit_base_effects(&mut state, &reg, "fruit1", "char1").unwrap();

        assert!(state.cards["char1"].modifiers.is_empty());
        // `passiveDescription ?? ""` keeps the trailing space.
        assert_eq!(
            state.log.last().unwrap().message,
            "Monkey D. Luffy mange le Gomu Gomu no Mi ! "
        );
    }

    /// JS truthiness: an *empty* `grantsTraits` array is still truthy, a `0`
    /// bonus is falsy.
    #[test]
    fn base_effects_empty_traits_are_truthy_zero_bonus_is_falsy() {
        let effects = FruitEffects {
            base: FruitBaseEffects {
                grants_traits: Some(Vec::new()),
                passive_description: Some("x".into()),
                atk_bonus: Some(0),
                def_bonus: Some(3),
            },
            awakening: None,
        };
        let (mut state, reg) = setup(effects);
        apply_fruit_base_effects(&mut state, &reg, "fruit1", "char1").unwrap();

        let mods = &state.cards["char1"].modifiers;
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].id, "fruit_def_fruit1");
    }

    #[test]
    fn base_effects_noop_for_missing_fruit_or_non_fruit() {
        let (mut state, reg) = setup(full_effects());
        apply_fruit_base_effects(&mut state, &reg, "nope", "char1").unwrap();
        assert!(state.log.is_empty());
        assert!(state.cards["char1"].modifiers.is_empty());

        // A card without fruitEffects is a silent no-op (no log either).
        apply_fruit_base_effects(&mut state, &reg, "char1", "char1").unwrap();
        assert!(state.log.is_empty());
    }

    #[test]
    fn can_awaken_checks_every_branch() {
        let (mut state, reg) = setup(full_effects());
        assert!(can_awaken_fruit(&state, &reg, PlayerId::Player1, "fruit1").unwrap());

        // missing fruit
        assert!(!can_awaken_fruit(&state, &reg, PlayerId::Player1, "nope").unwrap());

        // already awakened
        state.cards.get_mut("fruit1").unwrap().is_awakened = Some(true);
        assert!(!can_awaken_fruit(&state, &reg, PlayerId::Player1, "fruit1").unwrap());
        state.cards.get_mut("fruit1").unwrap().is_awakened = None;

        // wrong owner (bearer search is scoped to playerId)
        assert!(!can_awaken_fruit(&state, &reg, PlayerId::Player2, "fruit1").unwrap());

        // bearer not on the board
        state.cards.get_mut("char1").unwrap().zone = Zone::Graveyard;
        assert!(!can_awaken_fruit(&state, &reg, PlayerId::Player1, "fruit1").unwrap());
        state.cards.get_mut("char1").unwrap().zone = Zone::Board;

        // illegitimate bearer (name does not include "Luffy")
        state.cards.get_mut("char1").unwrap().def_id = "ch_zoro".into();
        assert!(!can_awaken_fruit(&state, &reg, PlayerId::Player1, "fruit1").unwrap());
        state.cards.get_mut("char1").unwrap().def_id = "ch_luffy".into();

        // too early
        state.turn_number = 4;
        assert!(!can_awaken_fruit(&state, &reg, PlayerId::Player1, "fruit1").unwrap());
        state.turn_number = 5;
        assert!(can_awaken_fruit(&state, &reg, PlayerId::Player1, "fruit1").unwrap());

        // cannot afford
        state.players.player1.volonte = 3;
        assert!(!can_awaken_fruit(&state, &reg, PlayerId::Player1, "fruit1").unwrap());
        state.players.player1.volonte = 4;
        assert!(can_awaken_fruit(&state, &reg, PlayerId::Player1, "fruit1").unwrap());
    }

    #[test]
    fn can_awaken_false_without_awakening_block() {
        let effects = FruitEffects {
            base: FruitBaseEffects::default(),
            awakening: None,
        };
        let (state, reg) = setup(effects);
        assert!(!can_awaken_fruit(&state, &reg, PlayerId::Player1, "fruit1").unwrap());
    }

    #[test]
    fn awaken_spends_vol_marks_and_logs() {
        let (mut state, reg) = setup(full_effects());
        awaken_fruit(&mut state, &reg, PlayerId::Player1, "fruit1").unwrap();

        assert_eq!(state.cards["fruit1"].is_awakened, Some(true));
        assert_eq!(state.players.player1.volonte, 6);

        let mods = &state.cards["char1"].modifiers;
        assert_eq!(mods.len(), 2);
        assert_eq!(mods[0].id, "fruit_awaken_atk_fruit1");
        assert_eq!(mods[0].amount, 3);
        assert_eq!(mods[0].source, "fruit_awaken_obj_gomu");
        assert_eq!(mods[1].id, "fruit_awaken_def_fruit1");
        assert_eq!(mods[1].amount, 1);

        assert_eq!(
            state.log.last().unwrap().message,
            "⭐ EVEIL ! Monkey D. Luffy eveille le Gomu Gomu no Mi ! Nika"
        );
    }

    #[test]
    fn awaken_rejects_when_not_allowed() {
        let (mut state, reg) = setup(full_effects());
        state.turn_number = 2;
        let err = awaken_fruit(&mut state, &reg, PlayerId::Player1, "fruit1").unwrap_err();
        assert_eq!(err, EngineError::illegal("Cannot awaken this fruit"));
        assert_eq!(state.players.player1.volonte, 10);
    }

    #[test]
    fn fruit_traits_include_awakening_traits_once_awakened() {
        let (mut state, reg) = setup(full_effects());
        assert_eq!(
            get_fruit_traits(&state, &reg, "char1").unwrap(),
            vec![Trait::Cursed, Trait::Logia]
        );

        state.cards.get_mut("fruit1").unwrap().is_awakened = Some(true);
        assert_eq!(
            get_fruit_traits(&state, &reg, "char1").unwrap(),
            vec![Trait::Cursed, Trait::Logia, Trait::Conqueror]
        );

        // Unknown character → empty list, non-fruit attachments are skipped.
        assert!(get_fruit_traits(&state, &reg, "nope").unwrap().is_empty());
        assert!(get_fruit_traits(&state, &reg, "fruit1").unwrap().is_empty());
    }

    #[test]
    fn fruit_traits_skip_non_fruit_objects_and_dangling_ids() {
        let mut effects = full_effects();
        effects.base.grants_traits = Some(vec![Trait::Cursed]);
        let (mut state, mut reg) = setup(effects);

        // A weapon (non-fruit subtype) plus a dangling instance id.
        let mut weapon = CardDef::new(
            "obj_sword",
            "Sabre",
            CardType::Object,
            1,
            Faction::Pirate,
            Rarity::C,
            "TEST",
        );
        weapon.subtype = Some(ObjectSubtype::Weapon);
        weapon.fruit_effects = Some(full_effects());
        reg.register_card(weapon);

        let mut sword = CardInstance::new("obj1".into(), "obj_sword".into(), PlayerId::Player1, 0);
        sword.zone = Zone::Board;
        state.cards.insert("obj1".into(), sword);
        state.cards.get_mut("char1").unwrap().attached_objects =
            vec!["obj1".into(), "ghost".into(), "fruit1".into()];

        assert_eq!(
            get_fruit_traits(&state, &reg, "char1").unwrap(),
            vec![Trait::Cursed]
        );
    }
}
