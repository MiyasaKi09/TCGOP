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
        assert_eq!(verify_deck(&deck), None);
    }
    // The eager decks.ts module-load checks print nothing.
    assert!(verify_decks().is_empty());
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
    // TS verifyDeck only console.warns — the exact text, never an error.
    assert_eq!(
        verify_deck(&bad),
        Some("Deck \"bad\" has 49 cards (expected 50)".to_string())
    );
    // …and an off-size deck is still playable (createInitialState never verifies).
    let reg = fake_registry();
    let mut ctx = EngineContext::seeded(1);
    let state = create_initial_state(&bad, &mugiwara_deck(), &reg, &mut ctx).unwrap();
    assert_eq!(state.players.player1.deck.len(), 49 - STARTING_HAND_SIZE);
    // The Rust-only strict check is the one that fails.
    assert_eq!(
        verify_deck_against(&bad, &reg),
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
    let mut ctx = EngineContext::new(42, 1_700_000_000_000);
    let state = create_initial_state(&mugiwara_deck(), &marines_deck(), &reg, &mut ctx).unwrap();

    assert_eq!(state.turn_number, 1);
    assert_eq!(state.current_player, PlayerId::Player1);
    assert_eq!(state.first_player, PlayerId::Player1);
    assert_eq!(state.phase, Phase::Main);
    assert!(state.pending_attack.is_none());
    assert!(state.winner.is_none());
    assert!(state.log.is_empty());
    assert_eq!(state.cards.len(), 100);
    assert_eq!(ctx.instance_counter, 100);

    // utils.ts generateInstanceId: `${defId}_${++instanceCounter}_${Date.now().toString(36)}`,
    // instances built in deck-list order before the shuffle.
    assert!(state.cards.contains_key("MG-001_1_loyw3v28"));
    assert!(state.cards.contains_key("MG-001_3_loyw3v28"));
    assert!(state.cards.contains_key("MG-002_4_loyw3v28"));
    assert!(state.cards.contains_key("MR-001_51_loyw3v28"));
    assert!(state.cards.contains_key("MR-026_100_loyw3v28"));
    assert_eq!(state.cards["MG-001_1_loyw3v28"].def_id, "MG-001");
    assert_eq!(state.cards["MR-001_51_loyw3v28"].owner, PlayerId::Player2);

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

    // Determinism: same context → identical state.
    let again = create_initial_state(
        &mugiwara_deck(),
        &marines_deck(),
        &reg,
        &mut EngineContext::new(42, 1_700_000_000_000),
    )
    .unwrap();
    assert_eq!(state, again);
    let other = create_initial_state(
        &mugiwara_deck(),
        &marines_deck(),
        &reg,
        &mut EngineContext::new(43, 1_700_000_000_000),
    )
    .unwrap();
    assert_ne!(state.players.player1.deck, other.players.player1.deck);

    // The counter is a process-lifetime global in TS: a second game from the
    // same context keeps numbering at 101 (createInitialState never resets it).
    let second = create_initial_state(&mugiwara_deck(), &marines_deck(), &reg, &mut ctx).unwrap();
    assert_eq!(ctx.instance_counter, 200);
    assert!(second.cards.contains_key("MG-001_101_loyw3v28"));
    assert!(second.cards.contains_key("MR-026_200_loyw3v28"));
    // utils.ts resetInstanceCounter()
    ctx.reset_instance_counter();
    assert_eq!(ctx.generate_instance_id("X"), "X_1_loyw3v28");
    ctx.set_now_ms(0);
    assert_eq!(ctx.generate_instance_id("X"), "X_2_0");
}

#[test]
fn base36_matches_js_number_to_string() {
    // Reference values from node: (n).toString(36)
    assert_eq!(to_base36(0), "0");
    assert_eq!(to_base36(35), "z");
    assert_eq!(to_base36(36), "10");
    assert_eq!(to_base36(1_700_000_000_000), "loyw3v28");
    assert_eq!(to_base36(1_757_376_000_123), "mfbsao3f");
    assert_eq!(EngineContext::new(0, 1_757_376_000_123).now_base36(), "mfbsao3f");
    let json = serde_json::to_string(&EngineContext::new(9, 5)).unwrap();
    let back: EngineContext = serde_json::from_str(&json).unwrap();
    assert_eq!(back, EngineContext::new(9, 5));
}

#[test]
fn create_game_runs_start_turn_like_init_ts() {
    let reg = fake_registry();
    let mut ctx = EngineContext::seeded(42);
    let state = create_game(&mugiwara_deck(), &marines_deck(), &reg, &mut ctx).unwrap();

    // createInitialState + startTurn: P1 has gained 1 Vol., did NOT draw on T1,
    // per-turn flags reset, phase main, nothing to log with an empty board.
    assert_eq!(state.turn_number, 1);
    assert_eq!(state.current_player, PlayerId::Player1);
    assert_eq!(state.phase, Phase::Main);
    assert!(state.log.is_empty());
    assert!(state.winner.is_none());
    let p1 = state.player(PlayerId::Player1);
    assert_eq!(p1.volonte, 1);
    assert_eq!(p1.hand.len(), STARTING_HAND_SIZE);
    assert_eq!(p1.deck.len(), 50 - STARTING_HAND_SIZE);
    assert!(!p1.has_drawn);
    assert!(!p1.captain.tapped);
    assert_eq!(p1.ally_ko_ed_this_turn, Some(false));
    assert_eq!(p1.haki_this_turn, Some(false));
    // P2 is untouched by P1's startTurn.
    let p2 = state.player(PlayerId::Player2);
    assert_eq!(p2.volonte, 0);
    assert_eq!(p2.hand.len(), STARTING_HAND_SIZE);
    assert_eq!(p2.ally_ko_ed_this_turn, None);

    // GameState::new_game is the same entry point from a bare seed.
    let via_seed = GameState::new_game(&mugiwara_deck(), &marines_deck(), &reg, 42).unwrap();
    assert_eq!(via_seed, state);

    // endTurn then startTurn for P2: P2 draws (7 cards), gains min(turn, 10) = 1 Vol.
    let mut state = state;
    state.end_turn(&reg).unwrap();
    assert_eq!(state.current_player, PlayerId::Player2);
    assert_eq!(state.turn_number, 1);
    assert_eq!(state.phase, Phase::End);
    state.start_turn(&reg, &ctx).unwrap();
    assert_eq!(state.phase, Phase::Main);
    let p2 = state.player(PlayerId::Player2);
    assert_eq!(p2.hand.len(), STARTING_HAND_SIZE + 1);
    assert!(p2.has_drawn);
    assert_eq!(p2.volonte, 1);
    // …and back to P1 on turn 2: draws, 2 Vol.
    state.end_turn(&reg).unwrap();
    assert_eq!(state.turn_number, 2);
    state.start_turn(&reg, &ctx).unwrap();
    let p1 = state.player(PlayerId::Player1);
    assert_eq!(p1.hand.len(), STARTING_HAND_SIZE + 1);
    assert_eq!(p1.volonte, 2);
}

/// Put `hand[idx]` of `player` onto the board in `slot` (bypassing deploy,
/// which is not ported yet) and return its instance id.
fn place_from_hand(state: &mut GameState, player: PlayerId, idx: usize, slot: Slot) -> String {
    let id = state.player_mut(player).hand.remove(idx);
    state.player_mut(player).board.set(slot, Some(id.clone()));
    let c = state.card_mut(&id).unwrap();
    c.zone = Zone::Board;
    c.slot = Some(slot);
    c.deployed_turn = Some(0);
    id
}

#[test]
fn start_turn_ko_check_and_self_ko_timers_mirror_game_state_ts() {
    let reg = fake_registry();
    let mut ctx = EngineContext::new(5, 1234);
    let mut state = create_game(&mugiwara_deck(), &marines_deck(), &reg, &mut ctx).unwrap();
    state.end_turn(&reg).unwrap();
    state.start_turn(&reg, &ctx).unwrap();
    state.end_turn(&reg).unwrap();
    // Now P1's turn 2 is about to start.
    assert_eq!(state.current_player, PlayerId::Player1);
    assert_eq!(state.turn_number, 2);

    // V1: burning to death (1 PV, burn 1). V2: Sandai Kitetsu bearer (3 PV → 2).
    // A1: selfKO timer at 1 → KO without KO bonus. A2: selfKO timer at 3 → 2.
    let burning = place_from_hand(&mut state, PlayerId::Player1, 0, Slot::V1);
    let bearer = place_from_hand(&mut state, PlayerId::Player1, 0, Slot::V2);
    let falling = place_from_hand(&mut state, PlayerId::Player1, 0, Slot::A1);
    let ticking = place_from_hand(&mut state, PlayerId::Player1, 0, Slot::A2);
    // Sandai Kitetsu: any P1 instance re-labelled as MG-010 and attached to the bearer.
    let sandai = state.player(PlayerId::Player1).deck[0].clone();
    state.card_mut(&sandai).unwrap().def_id = "MG-010".into();
    state.card_mut(&sandai).unwrap().zone = Zone::Board;
    state
        .card_mut(&bearer)
        .unwrap()
        .attached_objects
        .push(sandai.clone());
    {
        let c = state.card_mut(&burning).unwrap();
        c.current_pv = 1;
        c.status_effects.push(StatusEffect {
            effect_type: StatusEffectType::Burn,
            turns_remaining: 2,
            damage_per_turn: 1,
            source: "b".into(),
        });
    }
    for (id, turns) in [(&falling, 1), (&ticking, 3)] {
        state.card_mut(id).unwrap().status_effects.push(StatusEffect {
            effect_type: StatusEffectType::SelfKo,
            turns_remaining: turns,
            damage_per_turn: 0,
            source: "monster".into(),
        });
    }
    let vol_before_turn = state.player(PlayerId::Player1).volonte; // 1 (from T1)
    assert_eq!(vol_before_turn, 1);
    let log_start = state.log.len();

    state.start_turn(&reg, &ctx).unwrap();

    let p1 = state.player(PlayerId::Player1);
    // gainVolonte → 2, +2 KO bonus for the burn KO (the fake captain has no onAllyKO passive).
    assert_eq!(p1.volonte, 2 + ALLY_KO_BONUS_VOL);
    assert_eq!(p1.ally_ko_ed_this_turn, Some(true));
    assert_eq!(p1.char_ko_ed_this_game, Some(true));
    // Burn KO: removed from board, character in graveyard.
    assert_eq!(p1.board.get(Slot::V1), None);
    assert_eq!(state.cards[&burning].zone, Zone::Graveyard);
    assert_eq!(state.cards[&burning].slot, None);
    // Sandai curse: 3 → 2, still on board.
    assert_eq!(state.cards[&bearer].current_pv, 2);
    assert_eq!(p1.board.get(Slot::V2), Some(&bearer));
    // selfKO at 1 → KO (no KO bonus: the +2s above are fully accounted for).
    assert_eq!(p1.board.get(Slot::A1), None);
    assert_eq!(state.cards[&falling].zone, Zone::Graveyard);
    // selfKO at 3 → 2, untouched by processStartOfTurnEffects.
    assert_eq!(
        state.cards[&ticking].status(StatusEffectType::SelfKo).unwrap().turns_remaining,
        2
    );
    assert_eq!(p1.graveyard, vec![falling.clone(), burning.clone()]);

    // Log order = startTurn step order: 5b curse, 6a selfKO, 6b burn KO
    // (the fake registry names every card "Card <defId>").
    let msgs: Vec<&str> = state.log[log_start..]
        .iter()
        .map(|l| l.message.as_str())
        .collect();
    assert_eq!(
        msgs,
        vec![
            "Malédiction du Sandai Kitetsu : 1 dégât.".to_string(),
            format!(
                "Card {} retombe (fin de transformation) — KO.",
                state.cards[&falling].def_id
            ),
            format!(
                "Card {} est KO (brulure/effet) !",
                state.cards[&burning].def_id
            ),
        ]
    );
    assert!(state.log[log_start..].iter().all(|l| l.turn == 2 && l.player == PlayerId::Player1));
}

#[test]
fn end_turn_applies_crocodile_desiccation() {
    let reg = fake_registry();
    let mut ctx = EngineContext::seeded(8);
    let mut state = create_game(&baroque_deck(), &redhair_deck(), &reg, &mut ctx).unwrap();

    // Two enemy characters: V1 healthy (3/3), A1 injured (2/3) — first injured in slot order.
    let healthy = place_from_hand(&mut state, PlayerId::Player2, 0, Slot::V1);
    let injured = place_from_hand(&mut state, PlayerId::Player2, 0, Slot::A1);
    state.card_mut(&injured).unwrap().current_pv = 2;

    // Recto captain: the passive carries endTurnDesiccation but only the flipped side applies.
    state.end_turn(&reg).unwrap();
    assert_eq!(state.cards[&injured].current_pv, 2);
    assert!(state.log.is_empty());
    assert_eq!(state.current_player, PlayerId::Player2);
    // Back to P1 and flip.
    state.end_turn_switch();
    state.players.player1.captain.flipped = true;

    state.end_turn(&reg).unwrap();
    assert_eq!(state.cards[&injured].current_pv, 1);
    assert_eq!(state.cards[&healthy].current_pv, 3);
    assert_eq!(
        state.log.last().unwrap().message,
        format!(
            "Déshydratation : Card {} perd 1 PV permanent.",
            state.cards[&injured].def_id
        )
    );
    assert_eq!(state.log.last().unwrap().player, PlayerId::Player1);
    assert_eq!(state.phase, Phase::End);
    assert_eq!(state.current_player, PlayerId::Player2);

    // Second application drops it to 0 → removeFromBoard.
    state.end_turn_switch();
    state.end_turn(&reg).unwrap();
    assert_eq!(state.cards[&injured].current_pv, 0);
    assert_eq!(state.cards[&injured].zone, Zone::Graveyard);
    assert_eq!(state.player(PlayerId::Player2).board.get(Slot::A1), None);
    assert_eq!(state.player(PlayerId::Player2).graveyard, vec![injured.clone()]);
    assert_eq!(state.player(PlayerId::Player2).board.get(Slot::V1), Some(&healthy));
}

/// A registry exercising every passives.ts branch: A buffs tag-`x` allies and
/// heals adjacents; B (tag x, mugiwara) synergises with A; D buffs the best
/// ally at start of turn and explodes on KO; a ship with a parsed passive;
/// a captain whose verso side debuffs, banishes and self-buffs on ally KO.
fn passives_registry() -> CardRegistry {
    let mut reg = CardRegistry::new();
    let mk = |id: &str, atk: i32, pv: i32| {
        let mut d = CardDef::new(
            id,
            format!("N-{id}"),
            CardType::Character,
            2,
            Faction::Pirate,
            Rarity::C,
            "T",
        );
        d.atk = Some(atk);
        d.def = Some(1);
        d.pv = Some(pv);
        d
    };
    let passive = |effects: Vec<PassiveEffect>| {
        Some(PassiveDef {
            name: "p".into(),
            description: "d".into(),
            effects,
        })
    };
    let mut a = mk("A", 2, 3);
    a.passive = passive(vec![
        PassiveEffect::BuffAlly {
            stat: BuffStat::Atk,
            amount: 1,
            filter: Some(AllyFilter {
                faction: None,
                tag: Some("x".into()),
                trait_: None,
            }),
        },
        PassiveEffect::HealAdjacent { amount: 2 },
    ]);
    let mut b = mk("B", 4, 5);
    b.tags = Some(vec!["x".into(), "mugiwara".into()]);
    b.synergies = Some(vec![SynergyDef {
        partner_id: "A".into(),
        atk_bonus: 2,
        on_partner_ko: Some(3),
    }]);
    let mut d = mk("D", 1, 3);
    d.passive = passive(vec![
        PassiveEffect::StartTurnBuffAlly {
            stat: AtkDefStat::Atk,
            amount: 1,
        },
        PassiveEffect::ExplodeOnKo { amount: 9 },
    ]);
    let mut ship = CardDef::new("SHIP", "Ship", CardType::Ship, 1, Faction::Pirate, Rarity::C, "T");
    ship.ship_passive = Some("Vos Mugiwara gagnent +1 ATK et +2  DEF".into());
    reg.register_set(vec![a, b, d, ship]);

    let mut cap = fake_captain("CAP", "Cap");
    cap.recto.passive.effects = vec![PassiveEffect::BuffAlly {
        stat: BuffStat::Def,
        amount: 1,
        filter: None,
    }];
    cap.verso.passive.effects = vec![
        PassiveEffect::DebuffAdjacentEnemies { amount: 2 },
        PassiveEffect::DebuffOneEnemy { amount: 1 },
        PassiveEffect::BanishOnKo,
        PassiveEffect::SelfBuffOnAllyKo {
            stat: AtkStat::Atk,
            amount: 1,
            max: 2,
            filter: Some(AllyFilter {
                faction: None,
                tag: Some("x".into()),
                trait_: None,
            }),
        },
    ];
    reg.register_captain(cap);
    reg
}

/// Move the first hand card with `def_id` onto the board in `slot`.
fn place_def(state: &mut GameState, player: PlayerId, def_id: &str, slot: Slot) -> String {
    let idx = state
        .player(player)
        .hand
        .iter()
        .position(|id| state.cards[id].def_id == def_id)
        .unwrap();
    place_from_hand(state, player, idx, slot)
}

fn mods<'a>(state: &'a GameState, id: &str) -> Vec<(&'a str, ModifierStat, i32, &'a str)> {
    state.cards[id]
        .modifiers
        .iter()
        .map(|m| (m.id.as_str(), m.stat, m.amount, m.source.as_str()))
        .collect()
}

#[test]
fn passives_mirror_passives_ts() {
    let reg = passives_registry();
    let deck = |cards: &[&str]| DeckDef {
        name: "t".into(),
        captain_id: "CAP".into(),
        cards: cards
            .iter()
            .map(|c| DeckEntry {
                card_id: (*c).into(),
                count: 1,
            })
            .collect(),
    };
    let mut ctx = EngineContext::new(1, 777);
    let mut state = create_initial_state(
        &deck(&["A", "B", "D", "SHIP"]),
        &deck(&["A", "B"]),
        &reg,
        &mut ctx,
    )
    .unwrap();
    let a = place_def(&mut state, PlayerId::Player1, "A", Slot::V1);
    let b = place_def(&mut state, PlayerId::Player1, "B", Slot::V2);
    let d = place_def(&mut state, PlayerId::Player1, "D", Slot::A1);
    let ship = {
        let idx = state.players.player1.hand.iter().position(|id| state.cards[id].def_id == "SHIP").unwrap();
        let id = state.players.player1.hand.remove(idx);
        state.cards.get_mut(&id).unwrap().zone = Zone::Board;
        state.players.player1.active_ship = Some(id.clone());
        id
    };
    let a2 = place_def(&mut state, PlayerId::Player2, "A", Slot::V1);
    let b2 = place_def(&mut state, PlayerId::Player2, "B", Slot::A2);
    assert_eq!(state.cards[&ship].def_id, "SHIP");

    // --- recalculatePassiveBuffs: captain buffAlly, character buffAlly, ship, synergy ---
    recalculate_passive_buffs(&mut state, &reg, PlayerId::Player1).unwrap();
    // Idempotent: the passive_/captain_/synergy_ modifiers are stripped and rebuilt.
    let once = state.clone();
    recalculate_passive_buffs(&mut state, &reg, PlayerId::Player1).unwrap();
    assert_eq!(state, once);
    assert_eq!(
        mods(&state, &a),
        vec![(format!("captain_def_{a}").as_str(), ModifierStat::Def, 1, "captain_CAP")]
    );
    assert_eq!(
        mods(&state, &b),
        vec![
            (format!("captain_def_{b}").as_str(), ModifierStat::Def, 1, "captain_CAP"),
            (format!("passive_{a}_atk_{b}").as_str(), ModifierStat::Atk, 1, format!("passive_{a}").as_str()),
            (format!("ship_passive_atk_{b}").as_str(), ModifierStat::Atk, 1, "passive_ship_SHIP"),
            (format!("ship_passive_def_{b}").as_str(), ModifierStat::Def, 2, "passive_ship_SHIP"),
            (format!("synergy_{b}_A").as_str(), ModifierStat::Atk, 2, "synergy_A"),
        ]
    );
    assert_eq!(mods(&state, &d).len(), 1); // captain def only (no tag x / mugiwara)
    assert_eq!(get_effective_atk(&state, &reg, &b).unwrap(), 4 + 1 + 1 + 2);
    assert_eq!(get_effective_def(&state, &reg, &b).unwrap(), 1 + 1 + 2);
    assert_eq!(get_effective_atk(&state, &reg, "nope").unwrap(), 0);

    // --- applyEnemyDebuffAuras: P1 verso captain → enemy front row -2, strongest enemy -1 ---
    state.players.player1.captain.flipped = true;
    apply_enemy_debuff_auras(&mut state, &reg).unwrap();
    let once = state.clone();
    apply_enemy_debuff_auras(&mut state, &reg).unwrap();
    assert_eq!(state, once);
    assert_eq!(
        mods(&state, &a2),
        vec![(format!("debuffAura_adj_{a2}").as_str(), ModifierStat::Atk, -2, "debuffAura")]
    );
    assert_eq!(
        mods(&state, &b2),
        vec![(format!("debuffAura_one_{b2}").as_str(), ModifierStat::Atk, -1, "debuffAura")]
    );
    assert_eq!(get_effective_atk(&state, &reg, &a2).unwrap(), 0); // 2 - 2
    assert!(mods(&state, &a).iter().all(|m| m.3 != "debuffAura"));

    // --- applyStartOfTurnPassives: healAdjacent (A → V2, A1) then startTurnBuffAlly (D → B) ---
    state.cards.get_mut(&b).unwrap().current_pv = 2;
    state.cards.get_mut(&d).unwrap().current_pv = 1;
    state.cards.get_mut(&a).unwrap().current_pv = 1; // not adjacent to itself: unchanged
    apply_start_of_turn_passives(&mut state, &reg, &ctx, PlayerId::Player1).unwrap();
    assert_eq!(state.cards[&b].current_pv, 4);
    assert_eq!(state.cards[&d].current_pv, 3); // capped at def.pv
    assert_eq!(state.cards[&a].current_pv, 1);
    let vant = state.cards[&b].modifiers.last().unwrap();
    assert_eq!(vant.id, format!("vantardise_{b}_777"));
    assert_eq!(vant.source, format!("passive_{d}"));
    assert_eq!(vant.duration, ModifierDuration::Turn);
    assert_eq!(vant.amount, 1);
    let msgs: Vec<&str> = state.log.iter().map(|l| l.message.as_str()).collect();
    assert_eq!(
        msgs,
        vec![
            "N-A soigne 2 PV aux adjacents",
            "N-D : Vantardise — un allié gagne +1 ATK ce tour.",
        ]
    );
    state.log.clear();

    // --- applyOnKOEffects: A KO'd by P2 (flipped, banishOnKO) → rage on B, banish, recalc ---
    state.players.player2.captain.flipped = true;
    remove_from_board(&mut state, &reg, &a).unwrap();
    assert_eq!(state.players.player1.graveyard, vec![a.clone()]);
    apply_on_ko_effects(&mut state, &reg, &ctx, PlayerId::Player1, PlayerId::Player2, "A").unwrap();
    assert_eq!(state.players.player1.char_ko_ed_this_game, Some(true));
    assert_eq!(state.cards[&a].zone, Zone::Banished);
    assert!(state.players.player1.graveyard.is_empty());
    // TS quirk kept on purpose: the rage modifier (source `synergy_rage_A`) is
    // pushed and logged, then immediately stripped by the trailing
    // recalculatePassiveBuffs (`source.startsWith("synergy_")`).
    assert!(!state.cards[&b].modifiers.iter().any(|m| m.source == "synergy_rage_A"));
    // recalculated: A's buff and the A synergy are gone, the rest stays
    assert!(!state.cards[&b].modifiers.iter().any(|m| m.source == format!("passive_{a}")));
    assert!(!state.cards[&b].modifiers.iter().any(|m| m.source == "synergy_A"));
    assert!(state.cards[&b].modifiers.iter().any(|m| m.source == "passive_ship_SHIP"));
    // A has no tag x → the captain self-buff did not trigger
    assert!(state.players.player1.captain.modifiers.is_empty());
    let msgs: Vec<&str> = state.log.iter().map(|l| l.message.as_str()).collect();
    assert_eq!(
        msgs,
        vec![
            "N-A est banni (Justice Implacable) !",
            "N-B : rage ! +3 ATK (N-A KO)",
        ]
    );
    assert_eq!(state.log[0].player, PlayerId::Player2);
    assert_eq!(state.log[1].player, PlayerId::Player1);
    state.log.clear();

    // selfBuffOnAllyKO (tag x, max 2): three KOs of "B" → two +1 modifiers, then capped.
    for _ in 0..3 {
        apply_on_ko_effects(&mut state, &reg, &ctx, PlayerId::Player1, PlayerId::Player2, "B").unwrap();
    }
    let cap_mods = &state.players.player1.captain.modifiers;
    assert_eq!(cap_mods.len(), 2);
    assert!(cap_mods.iter().all(|m| m.id == "captainSelfKO_777"
        && m.source == "captainSelfKO"
        && m.amount == 1
        && m.stat == ModifierStat::Atk
        && m.duration == ModifierDuration::Permanent));
    let self_buff_logs = state
        .log
        .iter()
        .filter(|l| l.message == "Cap : +1 ATK permanent (Mugiwara KO).")
        .count();
    assert_eq!(self_buff_logs, 2);
    state.log.clear();

    // explodeOnKO: D KO'd → 9 damage to the lowest-PV enemy (A2, 3 PV) → KO'd and removed.
    remove_from_board(&mut state, &reg, &d).unwrap();
    apply_on_ko_effects(&mut state, &reg, &ctx, PlayerId::Player1, PlayerId::Player2, "D").unwrap();
    assert_eq!(state.cards[&a2].current_pv, 3 - 9);
    assert_eq!(state.cards[&a2].zone, Zone::Graveyard);
    assert_eq!(state.players.player2.board.get(Slot::V1), None);
    assert_eq!(state.players.player2.graveyard, vec![a2.clone()]);
    assert_eq!(state.cards[&b2].current_pv, 5);
    // Order: banish (P2's flipped captain) → explosion → the victim's KO.
    assert_eq!(state.cards[&d].zone, Zone::Banished);
    let msgs: Vec<&str> = state.log.iter().map(|l| l.message.as_str()).collect();
    assert_eq!(
        msgs,
        vec![
            "N-D est banni (Justice Implacable) !",
            "Corps Explosif : 9 dégâts à N-A !",
            "N-A est KO !",
        ]
    );
    assert_eq!(state.log[0].player, PlayerId::Player2);
    assert_eq!(state.log[1].player, PlayerId::Player1);
    assert_eq!(state.log[2].player, PlayerId::Player2);
}

#[test]
fn spend_volonte_error_text_matches_volonte_ts() {
    let err = EngineError::NotEnoughVolonte { has: 1, needs: 3 };
    assert_eq!(err.to_string(), "Not enough Volonte: has 1, needs 3");
    let err = EngineError::CannotAfford {
        what: "Zoro".into(),
        cost: 3,
        has: 1,
    };
    assert_eq!(err.to_string(), "Cannot afford Zoro (cost 3)");
}

#[test]
fn game_state_round_trips_through_serde_json() {
    let reg = fake_registry();
    let mut ctx = EngineContext::seeded(7);
    let mut state = create_initial_state(&baroque_deck(), &redhair_deck(), &reg, &mut ctx).unwrap();
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

    // Exactly the nine keys of the TS `GameState` interface, nothing extra.
    let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    let mut expected = vec![
        "cards",
        "players",
        "turnNumber",
        "currentPlayer",
        "phase",
        "pendingAttack",
        "log",
        "winner",
        "firstPlayer",
    ];
    let mut got = keys.clone();
    expected.sort_unstable();
    got.sort_unstable();
    assert_eq!(got, expected);
    assert!(v.get("rng").is_none() && v.get("instanceCounter").is_none());
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
    let mut ctx = EngineContext::seeded(3);
    let mut state = create_initial_state(&mugiwara_deck(), &marines_deck(), &reg, &mut ctx).unwrap();

    // gainVolonte: min(turnNumber, 10), replaces previous value.
    state.turn_number = 12;
    state.player_mut(PlayerId::Player1).volonte = 99;
    state.gain_volonte();
    assert_eq!(state.player(PlayerId::Player1).volonte, VOLONTE_CAP);
    state.turn_number = 1;

    // spend / canAfford
    assert!(state.spend_volonte(PlayerId::Player1, 4).is_ok());
    assert_eq!(state.player(PlayerId::Player1).volonte, 6);
    assert_eq!(
        state.spend_volonte(PlayerId::Player1, 7),
        Err(EngineError::NotEnoughVolonte { has: 6, needs: 7 })
    );
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
