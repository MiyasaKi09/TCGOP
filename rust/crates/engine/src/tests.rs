//! Smoke tests for the foundation: state construction, serde round-trips,
//! deck sizes and the `GameAction` wire format.

use std::collections::BTreeSet;

use serde_json::{Value, json};

use crate::prelude::*;

/// A stand-in registry: one placeholder `CardDef` per id referenced by the
/// four decks (real card data is ported by later modules) and one captain per
/// deck.
fn fake_registry() -> CardRegistry {
    let mut reg = CardRegistry::new();
    let mut ids = BTreeSet::new();
    for deck in all_decks() {
        for e in &deck.cards {
            ids.insert(e.card_id.clone());
        }
    }
    let cards: Vec<CardDef> = ids
        .into_iter()
        .map(|id| {
            let mut def = CardDef::new(
                id.clone(),
                format!("Card {id}"),
                CardType::Character,
                2,
                Faction::Pirate,
                Rarity::C,
                "TEST",
            );
            def.atk = Some(2);
            def.def = Some(1);
            def.pv = Some(3);
            def.traits = Some(vec![Trait::Rush]);
            def.passive = Some(PassiveDef {
                name: "P".into(),
                description: "d".into(),
                effects: vec![
                    PassiveEffect::BuffAlly {
                        stat: BuffStat::Atk,
                        amount: 1,
                        filter: Some(AllyFilter {
                            faction: None,
                            tag: Some("mugiwara".into()),
                            trait_: Some(Trait::Shield),
                        }),
                    },
                    PassiveEffect::OnAllyKo {
                        effect: OnAllyKoEffectKind::BonusWill,
                        amount: 2,
                    },
                    PassiveEffect::ThreeWeaponSlots,
                ],
            });
            def
        })
        .collect();
    reg.register_set(cards);

    for (id, name) in [
        ("CAP-LUFFY", "Luffy"),
        ("CAP-AKAINU", "Akainu"),
        ("CAP-CROCODILE", "Crocodile"),
        ("CAP-SHANKS", "Shanks"),
    ] {
        reg.register_captain(fake_captain(id, name));
    }
    reg
}

fn fake_captain(id: &str, name: &str) -> CaptainDef {
    let passive = PassiveDef {
        name: "cap".into(),
        description: "cap passive".into(),
        effects: vec![PassiveEffect::EndTurnDesiccation { amount: 1 }],
    };
    CaptainDef {
        id: id.into(),
        name: name.into(),
        faction: Faction::Pirate,
        tags: Some(vec!["mugiwara".into()]),
        traits: Some(vec![Trait::Conqueror]),
        recto: CaptainRecto {
            pv: 20,
            atk: 3,
            def: 2,
            passive: passive.clone(),
            attacks: vec![SpecialAttack {
                name: "Punch".into(),
                cost: 1,
                atk_bonus: 0,
                ..Default::default()
            }],
            surcharge: None,
        },
        flip_condition: FlipCondition {
            cost: Some(4),
            free_if_ally_ko: Some(true),
            ..Default::default()
        },
        verso: CaptainVerso {
            pv: 25,
            atk: 5,
            def: 3,
            passive,
            entry_effect: EntryEffect::Multi {
                effects: vec![
                    EntryEffect::BuffAllies {
                        stat: AtkDefStat::Atk,
                        amount: 1,
                        duration: TurnDuration::Turn,
                    },
                    EntryEffect::DamageEnemies {
                        amount: 2,
                        target: EntryDamageTarget::AllFront,
                        cursed_bonus: None,
                        sand: Some(true),
                    },
                ],
            },
            base_action: BaseAction {
                name: "Gear".into(),
                atk: 5,
                ..Default::default()
            },
            special_attack: SpecialAttack {
                name: "Red Hawk".into(),
                cost: 3,
                atk_bonus: 3,
                element: Some(Element::Fire),
                once_per_game: Some(true),
                ..Default::default()
            },
            surcharge: None,
            traits: None,
            natural_haki: Some(vec![HakiType::King]),
        },
    }
}

#[test]
fn four_decks_have_fifty_cards() {
    for deck in all_decks() {
        assert_eq!(deck_size(&deck), 50, "deck {}", deck.name);
        verify_deck(&deck).unwrap();
    }
    // Exact ids / captains from decks.ts
    assert_eq!(mugiwara_deck().captain_id, "CAP-LUFFY");
    assert_eq!(marines_deck().captain_id, "CAP-AKAINU");
    assert_eq!(baroque_deck().captain_id, "CAP-CROCODILE");
    assert_eq!(redhair_deck().captain_id, "CAP-SHANKS");
    assert_eq!(mugiwara_deck().cards.len(), 28);
    assert_eq!(marines_deck().cards.len(), 28);
    assert_eq!(baroque_deck().cards.len(), 28);
    assert_eq!(redhair_deck().cards.len(), 27);

    let bad = DeckDef {
        name: "bad".into(),
        captain_id: "CAP-LUFFY".into(),
        cards: vec![DeckEntry {
            card_id: "MG-001".into(),
            count: 49,
        }],
    };
    assert_eq!(
        verify_deck(&bad),
        Err(EngineError::InvalidDeckSize {
            name: "bad".into(),
            total: 49,
            expected: 50
        })
    );
}

#[test]
fn decks_verify_against_registry() {
    let reg = fake_registry();
    for deck in all_decks() {
        verify_deck_against(&deck, &reg).unwrap();
    }
    assert!(matches!(
        reg.get_card_def("NOPE"),
        Err(EngineError::UnknownCard(_))
    ));
    assert!(matches!(
        reg.get_captain_def("NOPE"),
        Err(EngineError::UnknownCaptain(_))
    ));
}

#[test]
fn initial_state_matches_ts_initial_values() {
    let reg = fake_registry();
    let state = GameState::new_game(&mugiwara_deck(), &marines_deck(), &reg, 42).unwrap();

    assert_eq!(state.turn_number, 1);
    assert_eq!(state.current_player, PlayerId::Player1);
    assert_eq!(state.first_player, PlayerId::Player1);
    assert_eq!(state.phase, Phase::Main);
    assert!(state.pending_attack.is_none());
    assert!(state.winner.is_none());
    assert!(state.log.is_empty());
    assert_eq!(state.cards.len(), 100);
    assert_eq!(state.instance_counter, 100);

    for p in state.players.iter() {
        assert_eq!(p.hand.len(), STARTING_HAND_SIZE);
        assert_eq!(p.deck.len(), 50 - STARTING_HAND_SIZE);
        assert!(p.graveyard.is_empty());
        assert_eq!(p.volonte, 0);
        assert_eq!(p.captain.current_pv, 20); // recto.pv
        assert!(!p.captain.flipped);
        assert!(p.active_ship.is_none());
        assert_eq!(p.board, Board::empty());
        assert!(!p.used_free_move && !p.has_drawn && !p.observation_used);
        assert!(!p.armament_used && !p.king_used);
        assert_eq!(p.ally_ko_ed_this_turn, None);
        for id in &p.hand {
            assert_eq!(state.cards[id].zone, Zone::Hand);
            assert_eq!(state.cards[id].owner, p.id);
        }
        for id in &p.deck {
            assert_eq!(state.cards[id].zone, Zone::Deck);
            assert_eq!(state.cards[id].current_pv, 3);
        }
    }

    // Determinism: same seed → identical state.
    let again = GameState::new_game(&mugiwara_deck(), &marines_deck(), &reg, 42).unwrap();
    assert_eq!(state, again);
    let other = GameState::new_game(&mugiwara_deck(), &marines_deck(), &reg, 43).unwrap();
    assert_ne!(state.players.player1.deck, other.players.player1.deck);
}

#[test]
fn game_state_round_trips_through_serde_json() {
    let reg = fake_registry();
    let mut state = GameState::new_game(&baroque_deck(), &redhair_deck(), &reg, 7).unwrap();
    state.pending_attack = Some(PendingAttack {
        attacker_id: "a".into(),
        target_id: "b".into(),
        target_is_captain: false,
        is_special: true,
        raw_damage: 4,
        attack_power: Some(6),
        element: Some(Element::Sand),
        attack_traits: vec![AttackTrait::Zone, AttackTrait::Impact],
        has_haki: true,
        ignore_shield: None,
        cannot_be_dodged: Some(true),
        immobilize: None,
        sleep: None,
        pushback: Some(true),
        pushback_slots: Some(2),
        strip_stealth: None,
    });
    state.add_log(PlayerId::Player2, "hello");
    state.players.player2.ally_ko_ed_this_turn = Some(true);
    state.players.player2.char_ko_ed_this_game = Some(false);
    state.players.player1.board.set(Slot::V2, Some("x".into()));
    state
        .players
        .player1
        .captain
        .status_effects
        .push(StatusEffect {
            effect_type: StatusEffectType::SelfKo,
            turns_remaining: -1,
            damage_per_turn: 0,
            source: "test".into(),
        });
    state.players.player1.captain.modifiers.push(Modifier {
        id: "m1".into(),
        stat: ModifierStat::Haki,
        amount: 1,
        source: "s".into(),
        duration: ModifierDuration::NextTurn,
        turns_remaining: Some(1),
    });

    let json = serde_json::to_string(&state).unwrap();
    let back: GameState = serde_json::from_str(&json).unwrap();
    assert_eq!(state, back);

    // TS key names on the wire.
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["turnNumber"], json!(1));
    assert_eq!(v["currentPlayer"], json!("player1"));
    assert_eq!(v["firstPlayer"], json!("player1"));
    assert_eq!(v["phase"], json!("main"));
    assert_eq!(
        v["pendingAttack"]["attackTraits"],
        json!(["zone", "impact"])
    );
    assert_eq!(v["pendingAttack"]["element"], json!("sand"));
    assert!(v["pendingAttack"].get("ignoreShield").is_none());
    assert_eq!(v["players"]["player1"]["board"]["V2"], json!("x"));
    assert_eq!(v["players"]["player1"]["board"]["A3"], Value::Null);
    assert_eq!(v["players"]["player1"]["activeShip"], Value::Null);
    assert_eq!(v["players"]["player2"]["allyKOedThisTurn"], json!(true));
    assert_eq!(v["players"]["player2"]["charKOedThisGame"], json!(false));
    assert_eq!(
        serde_json::to_value(SynergyDef {
            partner_id: "MG-002".into(),
            atk_bonus: 1,
            on_partner_ko: Some(2)
        })
        .unwrap(),
        json!({"partnerId": "MG-002", "atkBonus": 1, "onPartnerKO": 2})
    );
    assert_eq!(
        serde_json::to_value(EventEffect::BuffSingle {
            stat: AtkDefStat::Def,
            amount: 1,
            duration: BuffDuration::Permanent,
            requires_own_ko: Some(true)
        })
        .unwrap(),
        json!({"type": "buffSingle", "stat": "def", "amount": 1, "duration": "permanent", "requiresOwnKO": true})
    );
    assert_eq!(
        v["players"]["player1"]["captain"]["statusEffects"][0]["type"],
        json!("selfKO")
    );
    assert_eq!(
        v["players"]["player1"]["captain"]["modifiers"][0]["duration"],
        json!("nextTurn")
    );
    assert_eq!(
        v["log"][0],
        json!({"turn": 1, "player": "player2", "message": "hello"})
    );
    let first_hand = v["players"]["player1"]["hand"][0].as_str().unwrap();
    assert_eq!(v["cards"][first_hand]["zone"], json!("hand"));
    assert_eq!(v["cards"][first_hand]["usedOnceAbilities"], json!([]));

    // A TS-shaped state (no rng / instanceCounter keys) still deserialises.
    let mut ts_shaped = v.clone();
    ts_shaped.as_object_mut().unwrap().remove("rng");
    ts_shaped.as_object_mut().unwrap().remove("instanceCounter");
    let from_ts: GameState = serde_json::from_value(ts_shaped).unwrap();
    assert_eq!(from_ts.cards, state.cards);
}

#[test]
fn game_action_wire_format_matches_ts_discriminators() {
    let cases: Vec<(GameAction, Value)> = vec![
        (
            GameAction::DeployCharacter {
                instance_id: "MG-001_1".into(),
                slot: Slot::V1,
            },
            json!({"type": "deployCharacter", "instanceId": "MG-001_1", "slot": "V1"}),
        ),
        (
            GameAction::EquipObject {
                object_instance_id: "o".into(),
                target_instance_id: "t".into(),
            },
            json!({"type": "equipObject", "objectInstanceId": "o", "targetInstanceId": "t"}),
        ),
        (
            GameAction::DeployShip {
                instance_id: "s".into(),
            },
            json!({"type": "deployShip", "instanceId": "s"}),
        ),
        (
            GameAction::BaseAttack {
                attacker_instance_id: "a".into(),
                target_instance_id: "b".into(),
                target_is_captain: Some(true),
            },
            json!({"type": "baseAttack", "attackerInstanceId": "a", "targetInstanceId": "b", "targetIsCaptain": true}),
        ),
        (
            GameAction::SpecialAttack {
                attacker_instance_id: "a".into(),
                target_instance_id: "b".into(),
                target_is_captain: None,
            },
            json!({"type": "specialAttack", "attackerInstanceId": "a", "targetInstanceId": "b"}),
        ),
        (
            GameAction::BaseSupportAction {
                instance_id: "a".into(),
                target_instance_id: None,
            },
            json!({"type": "baseSupportAction", "instanceId": "a"}),
        ),
        (
            GameAction::PlayEvent {
                instance_id: "e".into(),
                targets: Some(vec!["x".into()]),
            },
            json!({"type": "playEvent", "instanceId": "e", "targets": ["x"]}),
        ),
        (
            GameAction::PlayCounter {
                instance_id: "c".into(),
            },
            json!({"type": "playCounter", "instanceId": "c"}),
        ),
        (
            GameAction::UseShield {
                blocker_instance_id: "b".into(),
            },
            json!({"type": "useShield", "blockerInstanceId": "b"}),
        ),
        (GameAction::PassCounter, json!({"type": "passCounter"})),
        (
            GameAction::FlipCaptain { slot: Slot::A2 },
            json!({"type": "flipCaptain", "slot": "A2"}),
        ),
        (
            GameAction::CaptainAttack {
                target_instance_id: "t".into(),
                target_is_captain: Some(false),
                is_special: Some(true),
            },
            json!({"type": "captainAttack", "targetInstanceId": "t", "targetIsCaptain": false, "isSpecial": true}),
        ),
        (
            GameAction::UseHaki {
                haki_type: HakiType::Observation,
                target_instance_id: Some("t".into()),
            },
            json!({"type": "useHaki", "hakiType": "observation", "targetInstanceId": "t"}),
        ),
        (
            GameAction::MoveCharacter {
                instance_id: "m".into(),
                target_slot: Slot::A1,
            },
            json!({"type": "moveCharacter", "instanceId": "m", "targetSlot": "A1"}),
        ),
        (
            GameAction::ActivateShip {
                ship_instance_id: "s".into(),
            },
            json!({"type": "activateShip", "shipInstanceId": "s"}),
        ),
        (
            GameAction::AwakenFruit {
                fruit_instance_id: "f".into(),
            },
            json!({"type": "awakenFruit", "fruitInstanceId": "f"}),
        ),
        (
            GameAction::FruitSpecialAttack {
                attacker_instance_id: "a".into(),
                fruit_instance_id: "f".into(),
                target_instance_id: "t".into(),
                target_is_captain: None,
            },
            json!({"type": "fruitSpecialAttack", "attackerInstanceId": "a", "fruitInstanceId": "f", "targetInstanceId": "t"}),
        ),
        (GameAction::EndTurn, json!({"type": "endTurn"})),
    ];
    assert_eq!(cases.len(), 18, "all 18 TS GameAction variants covered");
    for (action, expected) in cases {
        let v = serde_json::to_value(&action).unwrap();
        assert_eq!(v, expected, "{action:?}");
        assert_eq!(v["type"], json!(action.type_name()));
        let back: GameAction = serde_json::from_value(v).unwrap();
        assert_eq!(back, action);
    }
}

#[test]
fn effect_families_serialise_with_ts_literals() {
    let reg = fake_registry();
    let def = reg.get_card_def("MG-001").unwrap();
    let v = serde_json::to_value(def).unwrap();
    assert_eq!(v["type"], json!("character"));
    assert_eq!(v["rarity"], json!("C"));
    assert_eq!(v["faction"], json!("pirate"));
    assert_eq!(v["traits"], json!(["rush"]));
    assert_eq!(v["passive"]["effects"][0]["type"], json!("buffAlly"));
    assert_eq!(v["passive"]["effects"][0]["stat"], json!("atk"));
    assert_eq!(
        v["passive"]["effects"][0]["filter"]["trait"],
        json!("shield")
    );
    assert_eq!(
        v["passive"]["effects"][1],
        json!({"type": "onAllyKO", "effect": "bonusWill", "amount": 2})
    );
    assert_eq!(
        v["passive"]["effects"][2],
        json!({"type": "threeWeaponSlots"})
    );
    assert!(
        v.get("subtype").is_none(),
        "absent optional keys are omitted"
    );
    let back: CardDef = serde_json::from_value(v).unwrap();
    assert_eq!(&back, def);

    let cap = reg.get_captain_def("CAP-LUFFY").unwrap();
    let v = serde_json::to_value(cap).unwrap();
    assert_eq!(v["flipCondition"]["freeIfAllyKO"], json!(true));
    assert!(
        v["flipCondition"].get("freeIfAllyKo").is_none(),
        "acronym casing must match TS"
    );
    assert_eq!(v["verso"]["entryEffect"]["type"], json!("multi"));
    assert_eq!(
        v["verso"]["entryEffect"]["effects"][0]["duration"],
        json!("turn")
    );
    assert_eq!(
        v["verso"]["entryEffect"]["effects"][1]["target"],
        json!("allFront")
    );
    assert_eq!(v["verso"]["naturalHaki"], json!(["king"]));
    assert_eq!(
        v["recto"]["passive"]["effects"][0],
        json!({"type": "endTurnDesiccation", "amount": 1})
    );
    let back: CaptainDef = serde_json::from_value(v).unwrap();
    assert_eq!(&back, cap);

    // Remaining discriminated unions
    let ev = EventEffect::DamageEnemies {
        amount: 2,
        target: DamageTarget::AllCursed,
        cursed_bonus: Some(1),
        sand: None,
        destroy_ships: Some(true),
    };
    assert_eq!(
        serde_json::to_value(&ev).unwrap(),
        json!({"type": "damageEnemies", "amount": 2, "target": "allCursed", "cursedBonus": 1, "destroyShips": true})
    );
    let ce = CounterEffect::Cancel {
        description: "d".into(),
        max_attacker_atk: Some(3),
        self_captain_damage: None,
        once: Some(true),
    };
    assert_eq!(
        serde_json::to_value(&ce).unwrap(),
        json!({"type": "cancel", "description": "d", "maxAttackerAtk": 3, "once": true})
    );
    // TS `(Trait | AttackTrait)[]`: "range" resolves to the Trait arm, "zone" to AttackTrait.
    let gt: Vec<GrantedTrait> = serde_json::from_value(json!(["range", "zone", "logia"])).unwrap();
    assert_eq!(
        gt,
        vec![
            GrantedTrait::Trait(Trait::Range),
            GrantedTrait::AttackTrait(AttackTrait::Zone),
            GrantedTrait::Trait(Trait::Logia)
        ]
    );
    assert_eq!(
        serde_json::to_value(&gt).unwrap(),
        json!(["range", "zone", "logia"])
    );
    // Deserialising a raw TS-style card literal with nested fruit effects.
    let fruit: CardDef = serde_json::from_value(json!({
        "id": "MG-014", "name": "Gomu Gomu", "type": "object", "cost": 2, "faction": "pirate",
        "rarity": "R", "set": "MG", "subtype": "fruit",
        "fruitEffects": {
            "base": { "grantsTraits": ["cursed"], "atkBonus": 1 },
            "awakening": {
                "porteurLegitime": "Luffy", "minTurns": 3, "volCost": 4,
                "specialAttack": { "name": "Gear 5", "cost": 5, "atkBonus": 4, "description": "x",
                                   "attackTraits": ["impact"], "pushback": true }
            }
        }
    }))
    .unwrap();
    assert_eq!(fruit.subtype, Some(ObjectSubtype::Fruit));
    let awk = fruit
        .fruit_effects
        .as_ref()
        .unwrap()
        .awakening
        .as_ref()
        .unwrap();
    assert_eq!(awk.min_turns, 3);
    assert_eq!(
        awk.special_attack.as_ref().unwrap().attack_traits,
        Some(vec![AttackTrait::Impact])
    );
}

#[test]
fn rng_shuffle_and_draw_semantics() {
    let mut rng = EngineRng::from_seed_u64(1);
    let mut deck: Vec<u32> = (0..50).collect();
    rng.shuffle(&mut deck);
    let mut sorted = deck.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, (0..50).collect::<Vec<_>>());
    assert_ne!(deck, (0..50).collect::<Vec<_>>());

    // Same seed ⇒ same permutation.
    let mut rng2 = EngineRng::from_seed_u64(1);
    let mut deck2: Vec<u32> = (0..50).collect();
    rng2.shuffle(&mut deck2);
    assert_eq!(deck, deck2);

    // Top of the deck is index 0.
    let top = deck[0];
    assert_eq!(draw_top(&mut deck), Some(top));
    let next3: Vec<u32> = deck[..3].to_vec();
    assert_eq!(draw_top_n(&mut deck, 3), next3);
    assert_eq!(deck.len(), 46);

    // RNG position survives a serde round-trip.
    let json = serde_json::to_string(&rng).unwrap();
    let mut restored: EngineRng = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, rng);
    assert_eq!(restored.random_index(1000), rng.random_index(1000));
}

#[test]
fn turn_helpers_mirror_game_state_ts() {
    let reg = fake_registry();
    let mut state = GameState::new_game(&mugiwara_deck(), &marines_deck(), &reg, 3).unwrap();

    // gainVolonte: min(turnNumber, 10), replaces previous value.
    state.turn_number = 12;
    state.player_mut(PlayerId::Player1).volonte = 99;
    state.gain_volonte();
    assert_eq!(state.player(PlayerId::Player1).volonte, VOLONTE_CAP);
    state.turn_number = 1;

    // spend / canAfford
    assert!(state.spend_volonte(PlayerId::Player1, 4).is_ok());
    assert_eq!(state.player(PlayerId::Player1).volonte, 6);
    assert!(matches!(
        state.spend_volonte(PlayerId::Player1, 7),
        Err(EngineError::CannotAfford {
            cost: 7,
            has: 6,
            ..
        })
    ));
    assert!(state.spend_volonte(PlayerId::Player1, 0).is_ok());
    assert!(state.can_afford(PlayerId::Player1, 6));
    assert!(!state.can_afford(PlayerId::Player1, 7));

    // grantKOBonus
    state.grant_ko_bonus(PlayerId::Player2);
    assert_eq!(state.player(PlayerId::Player2).volonte, ALLY_KO_BONUS_VOL);
    assert_eq!(
        state.player(PlayerId::Player2).ally_ko_ed_this_turn,
        Some(true)
    );

    // drawCard: top of deck → end of hand, zone = hand, hasDrawn.
    let top = state.player(PlayerId::Player1).deck[0].clone();
    state.draw_card(PlayerId::Player1);
    let p1 = state.player(PlayerId::Player1);
    assert_eq!(p1.hand.last(), Some(&top));
    assert_eq!(p1.deck.len(), 43);
    assert!(p1.has_drawn);
    assert_eq!(state.cards[&top].zone, Zone::Hand);

    // Empty deck ⇒ opponent wins.
    state.player_mut(PlayerId::Player2).deck.clear();
    state.draw_card(PlayerId::Player2);
    assert_eq!(state.winner, Some(PlayerId::Player1));
    assert_eq!(state.check_win_condition(), Some(PlayerId::Player1));
    state.winner = None;

    // checkWinCondition: both captains dead ⇒ active player loses.
    state.players.player1.captain.current_pv = 0;
    state.players.player2.captain.current_pv = 0;
    assert_eq!(state.check_win_condition(), Some(PlayerId::Player2));
    state.players.player1.captain.current_pv = 20;
    assert_eq!(state.check_win_condition(), Some(PlayerId::Player1));
    state.players.player2.captain.current_pv = 20;
    assert_eq!(state.check_win_condition(), None);

    // Status ticks: poison can't kill, burn expires after its turns.
    let hand_id = state.player(PlayerId::Player1).hand[0].clone();
    state
        .players
        .player1
        .board
        .set(Slot::V1, Some(hand_id.clone()));
    {
        let c = state.card_mut(&hand_id).unwrap();
        c.zone = Zone::Board;
        c.slot = Some(Slot::V1);
        c.current_pv = 2;
        c.tapped = true;
        c.status_effects.push(StatusEffect {
            effect_type: StatusEffectType::Poison,
            turns_remaining: -1,
            damage_per_turn: 5,
            source: "p".into(),
        });
        c.status_effects.push(StatusEffect {
            effect_type: StatusEffectType::Burn,
            turns_remaining: 1,
            damage_per_turn: 1,
            source: "b".into(),
        });
        c.modifiers.push(Modifier {
            id: "turn".into(),
            stat: ModifierStat::Atk,
            amount: 1,
            source: "s".into(),
            duration: ModifierDuration::Turn,
            turns_remaining: None,
        });
        c.modifiers.push(Modifier {
            id: "perm".into(),
            stat: ModifierStat::Atk,
            amount: 1,
            source: "s".into(),
            duration: ModifierDuration::Permanent,
            turns_remaining: None,
        });
    }
    state.untap_all();
    state.reset_turn_flags();
    state.process_start_of_turn_effects();
    let c = state.card(&hand_id).unwrap();
    assert!(!c.tapped);
    // poison: 2 - 5 → floored to 1 (poison can't kill); burn: 1 - 1 = 0 (effects apply in list order).
    assert_eq!(c.current_pv, 0);
    assert_eq!(c.status_effects.len(), 1);
    assert_eq!(c.status_effects[0].effect_type, StatusEffectType::Poison);
    assert_eq!(c.modifiers.len(), 1);
    assert_eq!(c.modifiers[0].id, "perm");
    assert_eq!(c.logia_used_this_turn, Some(false));
    assert_eq!(state.player(PlayerId::Player1).haki_this_turn, Some(false));

    // endTurn tail: hand limit + player switch + turn bump.
    state.current_player = PlayerId::Player2;
    // (P2's deck was emptied above — borrow ids from P1's deck; only the zone bookkeeping matters.)
    let extra: Vec<String> = state.player(PlayerId::Player1).deck[..8].to_vec();
    state.player_mut(PlayerId::Player2).hand.extend(extra);
    let hand_before = state.player(PlayerId::Player2).hand.clone();
    assert_eq!(hand_before.len(), 14);
    state.end_turn_switch();
    assert_eq!(state.phase, Phase::End);
    assert_eq!(state.current_player, PlayerId::Player1);
    assert_eq!(state.turn_number, 2);
    let p2 = state.player(PlayerId::Player2);
    assert_eq!(p2.hand.len(), HAND_LIMIT);
    assert_eq!(p2.hand, hand_before[4..].to_vec());
    assert_eq!(p2.graveyard, hand_before[..4].to_vec());
    assert_eq!(state.cards[&p2.graveyard[0]].zone, Zone::Graveyard);
    assert_eq!(
        state.log.last().unwrap().message,
        "Limite de main : 4 carte(s) défaussée(s)."
    );
}

#[test]
fn slot_constants_and_adjacency() {
    assert_eq!(Slot::ALL.len(), 6);
    assert_eq!(Slot::FRONT, [Slot::V1, Slot::V2, Slot::V3]);
    assert_eq!(Slot::BACK, [Slot::A1, Slot::A2, Slot::A3]);
    assert_eq!(Slot::V2.adjacency(), &[Slot::V1, Slot::V3, Slot::A2]);
    assert_eq!(Slot::A1.adjacency(), &[Slot::A2, Slot::V1]);
    assert!(Slot::V1.is_adjacent_to(Slot::A1));
    assert!(!Slot::V1.is_adjacent_to(Slot::A2));
    assert_eq!(Slot::A3.row(), Row::Back);
    assert_eq!(serde_json::to_value(Slot::A3).unwrap(), json!("A3"));
    assert_eq!(PlayerId::Player1.opponent(), PlayerId::Player2);
}
