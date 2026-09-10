//! ST04 — équipage de Shanks le Roux card data.
//!
//! Port of `src/data/cards/redhair.ts` (`redhairCards`) — 27 card
//! definitions, in the exact source order (the order is load-bearing: the
//! registry is filled by iterating it, and later ids overwrite earlier ones).

use crate::types::{
    AttackTrait, BaseAction, BuffDuration, CardDef, CardType, CounterEffect, EventEffect, Faction,
    GrantedTrait, HakiType, ObjectSubtype, PassiveDef, PassiveEffect, Rarity, Row, ShipActive,
    ShipDestroyEffect, SpecialAttack, SynergyDef, Trait,
};

/// TS `redhairCards: CardDef[]` (`src/data/cards/redhair.ts`).
///
/// Returns the 27 definitions of the ST04 — Red Hair Pirates set in source order.
pub fn cards() -> Vec<CardDef> {
    fn base(id: &str, name: &str, card_type: CardType, cost: i32, rarity: Rarity) -> CardDef {
        // Every ST04 card is `faction: "pirate", set: "ST04"`.
        CardDef::new(id, name, card_type, cost, Faction::Pirate, rarity, "ST04")
    }

    fn tags(list: &[&str]) -> Option<Vec<String>> {
        Some(list.iter().map(|s| (*s).to_string()).collect())
    }

    vec![
        // === PERSONNAGES ===
        CardDef {
            atk: Some(6),
            def: Some(4),
            pv: Some(8),
            tags: tags(&["redhair"]),
            preferred_row: Some(Row::Front),
            passive: Some(PassiveDef {
                name: "Observation de Maître".to_string(),
                description:
                    "Vos personnages bénéficient de l'esquive Observation (1x/tour), même avant le tour 5."
                        .to_string(),
                effects: vec![PassiveEffect::GrantObservationAll],
            }),
            base_action: Some(BaseAction {
                name: "Tir de Précision".to_string(),
                atk: 6,
                description: Some("Le bras droit de l'Empereur.".to_string()),
                ..BaseAction::default()
            }),
            special_attack: Some(SpecialAttack {
                name: "Tir de Maître".to_string(),
                cost: 3,
                atk_bonus: 3,
                attack_traits: Some(vec![AttackTrait::Range]),
                immobilize: Some(true),
                description: Some("Portée · la cible perd sa prochaine action.".to_string()),
                ..SpecialAttack::default()
            }),
            ..base("RH-001", "Ben Beckman", CardType::Character, 5, Rarity::Sr)
        },
        CardDef {
            atk: Some(6),
            def: Some(3),
            pv: Some(8),
            tags: tags(&["redhair"]),
            preferred_row: Some(Row::Front),
            passive: Some(PassiveDef {
                name: "Quick Draw".to_string(),
                description: "Les attaques de Lucky Roux ne peuvent pas être esquivées."
                    .to_string(),
                effects: vec![PassiveEffect::NoDodge],
            }),
            base_action: Some(BaseAction {
                name: "Tir".to_string(),
                atk: 6,
                description: Some("Tir rapide.".to_string()),
                ..BaseAction::default()
            }),
            special_attack: Some(SpecialAttack {
                name: "Tir à Bout Portant".to_string(),
                cost: 2,
                atk_bonus: 3,
                attack_traits: Some(vec![AttackTrait::Range]),
                description: Some("Portée.".to_string()),
                ..SpecialAttack::default()
            }),
            ..base("RH-002", "Lucky Roux", CardType::Character, 4, Rarity::R)
        },
        CardDef {
            atk: Some(5),
            def: Some(2),
            pv: Some(6),
            traits: Some(vec![Trait::Range]),
            tags: tags(&["redhair", "tireur"]),
            preferred_row: Some(Row::Back),
            passive: Some(PassiveDef {
                name: "Précision Absolue".to_string(),
                description: "Les attaques de Yasopp ne peuvent être ni esquivées ni bloquées."
                    .to_string(),
                effects: vec![PassiveEffect::NoDodge, PassiveEffect::AttacksIgnoreShield],
            }),
            base_action: Some(BaseAction {
                name: "Tir Précis".to_string(),
                atk: 5,
                description: Some("Il ne rate jamais.".to_string()),
                ..BaseAction::default()
            }),
            special_attack: Some(SpecialAttack {
                name: "Tir Mortel".to_string(),
                cost: 3,
                atk_bonus: 3,
                attack_traits: Some(vec![AttackTrait::Piercing]),
                description: Some("Perçant.".to_string()),
                ..SpecialAttack::default()
            }),
            ..base("RH-003", "Yasopp", CardType::Character, 5, Rarity::R)
        },
        CardDef {
            atk: Some(3),
            def: Some(2),
            pv: Some(4),
            tags: tags(&["redhair"]),
            preferred_row: Some(Row::Front),
            synergies: Some(vec![SynergyDef {
                partner_id: "CAP-SHANKS".to_string(),
                atk_bonus: 1,
                on_partner_ko: None,
            }]),
            base_action: Some(BaseAction {
                name: "Coup Insolent".to_string(),
                atk: 3,
                description: Some("Il défie l'adversaire.".to_string()),
                ..BaseAction::default()
            }),
            special_attack: Some(SpecialAttack {
                name: "Provocation".to_string(),
                cost: 1,
                atk_bonus: 0,
                is_support: Some(true),
                description: Some("Un ennemi doit cibler Rockstar à son prochain tour.".to_string()),
                ..SpecialAttack::default()
            }),
            ..base("RH-004", "Rockstar", CardType::Character, 2, Rarity::C)
        },
        CardDef {
            atk: Some(5),
            def: Some(3),
            pv: Some(6),
            tags: tags(&["redhair"]),
            preferred_row: Some(Row::Front),
            natural_haki: Some(vec![HakiType::Armament]),
            passive: Some(PassiveDef {
                name: "Haki d'Équipage".to_string(),
                description: "Haki Armement naturel (attaques touchent les Logia).".to_string(),
                effects: vec![PassiveEffect::NaturalHaki {
                    haki_type: HakiType::Armament,
                }],
            }),
            base_action: Some(BaseAction {
                name: "Frappe".to_string(),
                atk: 5,
                description: Some("Coup d'équipage.".to_string()),
                ..BaseAction::default()
            }),
            special_attack: Some(SpecialAttack {
                name: "Frappe Aguerrie".to_string(),
                cost: 2,
                atk_bonus: 3,
                attack_traits: Some(vec![AttackTrait::Piercing]),
                description: Some("Perçant.".to_string()),
                ..SpecialAttack::default()
            }),
            ..base("RH-005", "Limejuice", CardType::Character, 3, Rarity::U)
        },
        CardDef {
            atk: Some(4),
            def: Some(4),
            pv: Some(7),
            traits: Some(vec![Trait::Shield]),
            tags: tags(&["redhair"]),
            preferred_row: Some(Row::Front),
            base_action: Some(BaseAction {
                name: "Coup de Poing".to_string(),
                atk: 4,
                description: Some("Le tank.".to_string()),
                ..BaseAction::default()
            }),
            special_attack: Some(SpecialAttack {
                name: "Charge".to_string(),
                cost: 2,
                atk_bonus: 3,
                attack_traits: Some(vec![AttackTrait::Impact]),
                description: Some("Impact.".to_string()),
                ..SpecialAttack::default()
            }),
            ..base("RH-006", "Bonk Punch", CardType::Character, 3, Rarity::U)
        },
        CardDef {
            atk: Some(3),
            def: Some(2),
            pv: Some(5),
            tags: tags(&["redhair"]),
            preferred_row: Some(Row::Front),
            passive: Some(PassiveDef {
                name: "Hurlement".to_string(),
                description: "Un ennemi a -1 ATK tant que Howling Gab est en jeu.".to_string(),
                effects: vec![PassiveEffect::DebuffOneEnemy { amount: 1 }],
            }),
            base_action: Some(BaseAction {
                name: "Coup Sonore".to_string(),
                atk: 3,
                description: Some("Le cri.".to_string()),
                ..BaseAction::default()
            }),
            special_attack: Some(SpecialAttack {
                name: "Onde de Choc".to_string(),
                cost: 2,
                atk_bonus: 2,
                attack_traits: Some(vec![AttackTrait::Impact]),
                description: Some("Impact.".to_string()),
                ..SpecialAttack::default()
            }),
            ..base("RH-007", "Howling Gab", CardType::Character, 3, Rarity::C)
        },
        CardDef {
            atk: Some(5),
            def: Some(3),
            pv: Some(6),
            tags: tags(&["redhair", "bretteur"]),
            preferred_row: Some(Row::Front),
            base_action: Some(BaseAction {
                name: "Coup de Canne".to_string(),
                atk: 5,
                description: Some("La canne-épée.".to_string()),
                ..BaseAction::default()
            }),
            special_attack: Some(SpecialAttack {
                name: "Coup Fourbe".to_string(),
                cost: 2,
                atk_bonus: 3,
                attack_traits: Some(vec![AttackTrait::Piercing]),
                ignore_shield: Some(true),
                description: Some("Perçant · ignore le Bouclier.".to_string()),
                ..SpecialAttack::default()
            }),
            ..base("RH-008", "Building Snake", CardType::Character, 3, Rarity::U)
        },
        CardDef {
            atk: Some(1),
            def: Some(2),
            pv: Some(4),
            tags: tags(&["redhair", "medecin"]),
            preferred_row: Some(Row::Back),
            passive: Some(PassiveDef {
                name: "Médecin de l'Équipage".to_string(),
                description: "Début de votre tour : soignez 1 PV à un allié adjacent.".to_string(),
                effects: vec![PassiveEffect::HealAdjacent { amount: 1 }],
            }),
            base_action: Some(BaseAction {
                name: "Soins".to_string(),
                atk: 0,
                is_support: Some(true),
                heal_amount: Some(2),
                description: Some("Soigne 2 PV à un allié.".to_string()),
                ..BaseAction::default()
            }),
            special_attack: Some(SpecialAttack {
                name: "Stimulant".to_string(),
                cost: 2,
                atk_bonus: 0,
                is_support: Some(true),
                description: Some("Un allié gagne +2 ATK et perd gelé/immobilisé.".to_string()),
                ..SpecialAttack::default()
            }),
            ..base("RH-009", "Hongo", CardType::Character, 2, Rarity::C)
        },
        // === OBJETS - ARMES ===
        CardDef {
            subtype: Some(ObjectSubtype::Weapon),
            bonus_atk: Some(2),
            restriction: Some("bretteur".to_string()),
            equip_effect: Some(
                "+2 ATK. Attaques : Haki Armement (touchent les Logia) et ignorent le Bouclier. Si équipée par Shanks : +1 ATK."
                    .to_string(),
            ),
            ..base("RH-010", "Gryphon", CardType::Object, 2, Rarity::R)
        },
        CardDef {
            subtype: Some(ObjectSubtype::Weapon),
            bonus_atk: Some(1),
            restriction: Some("tireur".to_string()),
            grants_traits: Some(vec![GrantedTrait::Trait(Trait::Range)]),
            equip_effect: Some(
                "+1 ATK. Attaques : Portée. Si équipée par Beckman : +1 ATK.".to_string(),
            ),
            ..base("RH-011", "Fusil de Beckman", CardType::Object, 1, Rarity::C)
        },
        CardDef {
            subtype: Some(ObjectSubtype::Weapon),
            bonus_atk: Some(1),
            restriction: Some("tireur".to_string()),
            grants_traits: Some(vec![GrantedTrait::Trait(Trait::Range)]),
            equip_effect: Some(
                "+1 ATK. Attaques : Portée. Si équipée par Lucky Roux : attaques inesquivables."
                    .to_string(),
            ),
            ..base(
                "RH-012",
                "Pistolet de Lucky Roux",
                CardType::Object,
                1,
                Rarity::C,
            )
        },
        CardDef {
            subtype: Some(ObjectSubtype::Weapon),
            bonus_atk: Some(1),
            restriction: Some("tireur".to_string()),
            grants_traits: Some(vec![GrantedTrait::Trait(Trait::Range)]),
            equip_effect: Some(
                "+1 ATK. Attaques : Portée. Si équipée par Yasopp : +1 ATK et Perçant.".to_string(),
            ),
            ..base("RH-013", "Fusil de Yasopp", CardType::Object, 1, Rarity::C)
        },
        // === OBJETS - ACCESSOIRES ===
        CardDef {
            subtype: Some(ObjectSubtype::Accessory),
            bonus_atk: Some(1),
            equip_effect: Some(
                "+1 ATK. La première fois que le porteur serait KO, il survit avec 1 PV."
                    .to_string(),
            ),
            ..base("RH-014", "Chapeau de Paille", CardType::Object, 1, Rarity::R)
        },
        CardDef {
            subtype: Some(ObjectSubtype::Accessory),
            bonus_atk: Some(0),
            equip_effect: Some(
                "1x/partie : tous vos alliés sont soignés de 2 PV et gagnent +1 ATK ce tour."
                    .to_string(),
            ),
            ..base("RH-015", "Sake de la Fête", CardType::Object, 1, Rarity::U)
        },
        CardDef {
            subtype: Some(ObjectSubtype::Accessory),
            bonus_atk: Some(0),
            bonus_def: Some(2),
            equip_effect: Some(
                "+2 DEF. Les ennemis adjacents au porteur ont -1 ATK.".to_string(),
            ),
            ..base(
                "RH-016",
                "Cape de l'Empereur",
                CardType::Object,
                2,
                Rarity::R,
            )
        },
        // === NAVIRES ===
        CardDef {
            ship_passive: Some("Vos personnages gagnent +1 ATK.".to_string()),
            ship_active: Some(ShipActive {
                name: "Volonté d'Acier".to_string(),
                cost: 3,
                description: "Ce tour, vos personnages gagnent le Haki Armement et +1 ATK."
                    .to_string(),
                once_per_game: Some(true),
            }),
            ..base("RH-017", "Red Force", CardType::Ship, 2, Rarity::R)
        },
        CardDef {
            ship_passive: Some("Vos personnages gagnent +1 PV.".to_string()),
            ship_destroy_effect: Some(ShipDestroyEffect {
                heal_all: None,
                draw: Some(1),
                buff_atk: None,
                buff_def: None,
                deploy_token: None,
            }),
            ..base(
                "RH-018",
                "Navire du Nouveau Monde",
                CardType::Ship,
                1,
                Rarity::C,
            )
        },
        // === EVENEMENTS ===
        CardDef {
            event_effect: Some(EventEffect::DebuffAllEnemies {
                atk: 2,
                immobilize_max_def: Some(3),
            }),
            ..base("RH-019", "Haki du Roi", CardType::Event, 3, Rarity::R)
        },
        CardDef {
            event_effect: Some(EventEffect::HealAllBuff { heal: 3, atk: 1 }),
            ..base("RH-020", "Festin", CardType::Event, 2, Rarity::U)
        },
        CardDef {
            event_effect: Some(EventEffect::DebuffAllEnemies {
                atk: 2,
                immobilize_max_def: None,
            }),
            ..base("RH-021", "Intimidation", CardType::Event, 2, Rarity::U)
        },
        CardDef {
            event_effect: Some(EventEffect::BuffSingle {
                stat: crate::types::AtkDefStat::Atk,
                amount: 2,
                duration: BuffDuration::Permanent,
                requires_own_ko: None,
            }),
            ..base("RH-022", "Promesse", CardType::Event, 1, Rarity::C)
        },
        CardDef {
            event_effect: Some(EventEffect::GrantHakiAll { atk: None }),
            ..base("RH-023", "L'Ère des Rêves", CardType::Event, 1, Rarity::C)
        },
        CardDef {
            event_effect: Some(EventEffect::Tutor {
                filter_tag: None,
                max_cost: None,
            }),
            ..base("RH-026", "Alliance", CardType::Event, 2, Rarity::U)
        },
        // === COUNTERS ===
        CardDef {
            counter_effect: Some(CounterEffect::ReduceDamage {
                amount: 3,
                captain_bonus: None,
            }),
            ..base(
                "RH-024",
                "Courant du Nouveau Monde",
                CardType::Counter,
                0,
                Rarity::C,
            )
        },
        CardDef {
            counter_effect: Some(CounterEffect::ReduceDamage {
                amount: 3,
                captain_bonus: None,
            }),
            ..base(
                "RH-025",
                "Imposer le Respect",
                CardType::Counter,
                1,
                Rarity::U,
            )
        },
        CardDef {
            counter_effect: Some(CounterEffect::Cancel {
                description: "Annulez l'attaque ; votre Capitaine subit 5 dégâts.".to_string(),
                max_attacker_atk: None,
                self_captain_damage: Some(5),
                once: None,
            }),
            ..base(
                "RH-027",
                "Sacrifice du Bras",
                CardType::Counter,
                3,
                Rarity::R,
            )
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::BTreeSet;

    #[test]
    fn card_count_matches_ts() {
        // `redhairCards` in src/data/cards/redhair.ts holds 27 entries.
        assert_eq!(cards().len(), 27);
    }

    #[test]
    fn ids_are_unique_and_in_source_order() {
        let cards = cards();
        let ids: Vec<&str> = cards.iter().map(|c| c.id.as_str()).collect();
        let unique: BTreeSet<&str> = ids.iter().copied().collect();
        assert_eq!(unique.len(), ids.len(), "duplicate id in redhairCards");
        // Source order: events RH-019..RH-023 then RH-026, then counters
        // RH-024, RH-025, RH-027 (the TS file groups by section, not by id).
        assert_eq!(
            ids,
            vec![
                "RH-001", "RH-002", "RH-003", "RH-004", "RH-005", "RH-006", "RH-007", "RH-008",
                "RH-009", "RH-010", "RH-011", "RH-012", "RH-013", "RH-014", "RH-015", "RH-016",
                "RH-017", "RH-018", "RH-019", "RH-020", "RH-021", "RH-022", "RH-023", "RH-026",
                "RH-024", "RH-025", "RH-027",
            ]
        );
    }

    #[test]
    fn every_card_is_pirate_st04() {
        for c in cards() {
            assert_eq!(c.faction, Faction::Pirate, "{}", c.id);
            assert_eq!(c.set, "ST04", "{}", c.id);
        }
    }

    #[test]
    fn deck_references_are_resolvable() {
        let cards = cards();
        let ids: BTreeSet<&str> = cards.iter().map(|c| c.id.as_str()).collect();
        let deck = crate::decks::redhair_deck();
        assert_eq!(deck.captain_id, "CAP-SHANKS");
        for entry in &deck.cards {
            assert!(
                ids.contains(entry.card_id.as_str()),
                "deck references unknown card {}",
                entry.card_id
            );
        }
        // Every ST04 card appears in the starter deck.
        assert_eq!(deck.cards.len(), cards.len());
    }

    #[test]
    fn tricky_literals_are_transcribed() {
        let cards = cards();
        let by_id = |id: &str| cards.iter().find(|c| c.id == id).unwrap().clone();

        // RH-001: passive grantObservationAll + immobilizing Range special.
        let beckman = by_id("RH-001");
        assert_eq!(
            beckman.passive.as_ref().unwrap().effects,
            vec![PassiveEffect::GrantObservationAll]
        );
        let sp = beckman.special_attack.as_ref().unwrap();
        assert_eq!(sp.immobilize, Some(true));
        assert_eq!(sp.attack_traits, Some(vec![AttackTrait::Range]));

        // RH-003: two passive effects, in TS order.
        assert_eq!(
            by_id("RH-003").passive.unwrap().effects,
            vec![PassiveEffect::NoDodge, PassiveEffect::AttacksIgnoreShield]
        );

        // RH-004: the only synergy in the set (no onPartnerKO).
        assert_eq!(
            by_id("RH-004").synergies.unwrap(),
            vec![SynergyDef {
                partner_id: "CAP-SHANKS".to_string(),
                atk_bonus: 1,
                on_partner_ko: None,
            }]
        );

        // RH-005: naturalHaki appears both as a card field and as a passive effect.
        let limejuice = by_id("RH-005");
        assert_eq!(limejuice.natural_haki, Some(vec![HakiType::Armament]));
        assert_eq!(
            limejuice.passive.unwrap().effects,
            vec![PassiveEffect::NaturalHaki {
                haki_type: HakiType::Armament
            }]
        );

        // RH-015 / RH-016 carry an explicit `bonusAtk: 0` (present, not absent).
        assert_eq!(by_id("RH-015").bonus_atk, Some(0));
        assert_eq!(by_id("RH-016").bonus_atk, Some(0));
        assert_eq!(by_id("RH-016").bonus_def, Some(2));
        // …while RH-014 has bonusAtk 1 and no bonusDef.
        assert_eq!(by_id("RH-014").bonus_atk, Some(1));
        assert_eq!(by_id("RH-014").bonus_def, None);

        // RH-019 immobilizes DEF <= 3, RH-021 is the same effect without it.
        assert_eq!(
            by_id("RH-019").event_effect.unwrap(),
            EventEffect::DebuffAllEnemies {
                atk: 2,
                immobilize_max_def: Some(3)
            }
        );
        assert_eq!(
            by_id("RH-021").event_effect.unwrap(),
            EventEffect::DebuffAllEnemies {
                atk: 2,
                immobilize_max_def: None
            }
        );

        // RH-027: cancel with 5 self-captain damage, accented "dégâts".
        assert_eq!(
            by_id("RH-027").counter_effect.unwrap(),
            CounterEffect::Cancel {
                description: "Annulez l'attaque ; votre Capitaine subit 5 dégâts.".to_string(),
                max_attacker_atk: None,
                self_captain_damage: Some(5),
                once: None,
            }
        );

        // RH-009: support base action (atk 0, heal 2) and support special.
        let hongo = by_id("RH-009");
        let ba = hongo.base_action.as_ref().unwrap();
        assert_eq!(
            (ba.atk, ba.is_support, ba.heal_amount),
            (0, Some(true), Some(2))
        );
        assert_eq!(
            hongo.special_attack.as_ref().unwrap().is_support,
            Some(true)
        );

        // RH-018: shipDestroyEffect draws exactly 1 and does nothing else.
        assert_eq!(
            by_id("RH-018").ship_destroy_effect.unwrap(),
            ShipDestroyEffect {
                heal_all: None,
                draw: Some(1),
                buff_atk: None,
                buff_def: None,
                deploy_token: None,
            }
        );
    }
}
