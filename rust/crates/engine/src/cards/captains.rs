//! The four captain definitions.
//!
//! Port of `src/data/cards/captains.ts` — `captainLuffy` (`CAP-LUFFY`),
//! `captainAkainu` (`CAP-AKAINU`), `captainCrocodile` (`CAP-CROCODILE`) and
//! `captainShanks` (`CAP-SHANKS`), aggregated by `allCaptains` in that order.

use crate::types::{
    AllyFilter, AtkStat, AttackTrait, BaseAction, BuffStat, CaptainDef, CaptainRecto, CaptainVerso,
    Element, EntryDamageTarget, EntryEffect, Faction, FlipCondition, HakiType, PassiveDef,
    PassiveEffect, SpecialAttack, Trait,
};

/// TS `captainLuffy` (`src/data/cards/captains.ts:3`) — `CAP-LUFFY`.
pub fn captain_luffy() -> CaptainDef {
    CaptainDef {
        id: "CAP-LUFFY".to_string(),
        name: "Monkey D. Luffy".to_string(),
        faction: Faction::Pirate,
        tags: Some(vec!["mugiwara".to_string()]),
        traits: Some(vec![Trait::Conqueror]),

        recto: CaptainRecto {
            pv: 30,
            atk: 4,
            def: 2,
            passive: PassiveDef {
                name: "Pavillon au Chapeau de Paille".to_string(),
                description: "Vos personnages Pirate gagnent +1 ATK.".to_string(),
                effects: vec![PassiveEffect::BuffAlly {
                    stat: BuffStat::Atk,
                    amount: 1,
                    filter: Some(AllyFilter {
                        faction: Some(Faction::Pirate),
                        ..Default::default()
                    }),
                }],
            },
            // Le Capitaine recto ne peut pas attaquer (Rulebook v3.1 §2.1).
            attacks: vec![],
            surcharge: None,
        },

        flip_condition: FlipCondition {
            cost: Some(2),
            free_if_ally_ko: Some(true),
            ..Default::default()
        },

        verso: CaptainVerso {
            pv: 25,
            atk: 6,
            def: 2,
            passive: PassiveDef {
                name: "Esprit de Capitaine".to_string(),
                description: "Immunisé contre l'Impact. Quand un Mugiwara allié est KO : Luffy gagne +1 ATK permanent (max +3).".to_string(),
                effects: vec![
                    PassiveEffect::ImmuneImpact,
                    PassiveEffect::SelfBuffOnAllyKo {
                        stat: AtkStat::Atk,
                        amount: 1,
                        max: 3,
                        filter: Some(AllyFilter {
                            tag: Some("mugiwara".to_string()),
                            ..Default::default()
                        }),
                    },
                ],
            },
            entry_effect: EntryEffect::Multi {
                effects: vec![
                    EntryEffect::GrantSelfRush,
                    EntryEffect::DamageEnemies {
                        amount: 3,
                        target: EntryDamageTarget::Single,
                        cursed_bonus: None,
                        sand: None,
                    },
                ],
            },
            base_action: BaseAction {
                name: "Gomu Gomu no Pistol".to_string(),
                atk: 6,
                description: Some("Le poing élastique.".to_string()),
                ..Default::default()
            },
            special_attack: SpecialAttack {
                name: "Gomu Gomu no Bazooka".to_string(),
                cost: 3,
                atk_bonus: 4,
                attack_traits: Some(vec![AttackTrait::Impact]),
                pushback: Some(true),
                description: Some("Impact — repousse la cible d'un slot.".to_string()),
                ..Default::default()
            },
            surcharge: None,
            traits: Some(vec![Trait::Cursed, Trait::Conqueror]),
            natural_haki: None,
        },
    }
}

/// TS `captainAkainu` (`src/data/cards/captains.ts:55`) — `CAP-AKAINU`.
pub fn captain_akainu() -> CaptainDef {
    CaptainDef {
        id: "CAP-AKAINU".to_string(),
        name: "Akainu (Sakazuki)".to_string(),
        faction: Faction::Marine,
        tags: Some(vec!["marine".to_string(), "amiral".to_string()]),
        traits: Some(vec![]),

        recto: CaptainRecto {
            pv: 35,
            atk: 5,
            def: 4,
            passive: PassiveDef {
                name: "Ordre de Marche".to_string(),
                description: "Vos personnages Marine gagnent +1 ATK.".to_string(),
                effects: vec![PassiveEffect::BuffAlly {
                    stat: BuffStat::Atk,
                    amount: 1,
                    filter: Some(AllyFilter {
                        faction: Some(Faction::Marine),
                        ..Default::default()
                    }),
                }],
            },
            attacks: vec![],
            surcharge: None,
        },

        flip_condition: FlipCondition {
            cost: Some(3),
            free_if_enemy_cursed: Some(true),
            ..Default::default()
        },

        verso: CaptainVerso {
            pv: 28,
            atk: 8,
            def: 3,
            passive: PassiveDef {
                name: "Justice Implacable".to_string(),
                description:
                    "Intangibilité Logia. Un ennemi KO par Akainu est banni (retiré du jeu)."
                        .to_string(),
                effects: vec![PassiveEffect::LogiaIntangibility, PassiveEffect::BanishOnKo],
            },
            entry_effect: EntryEffect::DamageEnemies {
                amount: 4,
                target: EntryDamageTarget::Single,
                cursed_bonus: Some(6),
                sand: None,
            },
            base_action: BaseAction {
                name: "Coup de Magma".to_string(),
                atk: 8,
                element: Some(Element::Fire),
                description: Some("Le poing de magma.".to_string()),
                ..Default::default()
            },
            special_attack: SpecialAttack {
                name: "Ryusei Kazan".to_string(),
                cost: 4,
                atk_bonus: 4,
                element: Some(Element::Fire),
                attack_traits: Some(vec![AttackTrait::Zone]),
                description: Some("Les météores de magma. Zone · Feu.".to_string()),
                ..Default::default()
            },
            surcharge: None,
            traits: Some(vec![Trait::Cursed, Trait::Logia]),
            natural_haki: None,
        },
    }
}

/// TS `captainCrocodile` (`src/data/cards/captains.ts:93`) — `CAP-CROCODILE`.
pub fn captain_crocodile() -> CaptainDef {
    CaptainDef {
        id: "CAP-CROCODILE".to_string(),
        name: "Crocodile (Mr. 0)".to_string(),
        faction: Faction::Pirate,
        tags: Some(vec!["baroque".to_string()]),
        traits: Some(vec![]),

        recto: CaptainRecto {
            pv: 30,
            atk: 5,
            def: 3,
            passive: PassiveDef {
                name: "Maître de Baroque Works".to_string(),
                description: "Vos personnages Baroque Works gagnent +1 ATK.".to_string(),
                effects: vec![PassiveEffect::BuffAlly {
                    stat: BuffStat::Atk,
                    amount: 1,
                    filter: Some(AllyFilter {
                        tag: Some("baroque".to_string()),
                        ..Default::default()
                    }),
                }],
            },
            attacks: vec![],
            surcharge: None,
        },

        flip_condition: FlipCondition {
            cost: Some(3),
            free_if_allies_gte: Some(3),
            ..Default::default()
        },

        verso: CaptainVerso {
            pv: 25,
            atk: 7,
            def: 2,
            passive: PassiveDef {
                name: "Déshydratation".to_string(),
                description: "Intangibilité Logia. Fin de votre tour : un ennemi blessé perd 1 PV permanent (Sable).".to_string(),
                effects: vec![
                    PassiveEffect::LogiaIntangibility,
                    PassiveEffect::EndTurnDesiccation { amount: 1 },
                ],
            },
            entry_effect: EntryEffect::DamageEnemies {
                amount: 4,
                target: EntryDamageTarget::Single,
                cursed_bonus: None,
                sand: Some(true),
            },
            base_action: BaseAction {
                name: "Sables".to_string(),
                atk: 7,
                description: Some("La main de sable.".to_string()),
                ..Default::default()
            },
            special_attack: SpecialAttack {
                name: "Desert Girasol".to_string(),
                cost: 4,
                atk_bonus: 4,
                element: Some(Element::Sand),
                permanent_pv_loss: Some(2),
                description: Some("La cible perd 2 PV permanent (Sable).".to_string()),
                ..Default::default()
            },
            surcharge: None,
            traits: Some(vec![Trait::Cursed, Trait::Logia]),
            natural_haki: None,
        },
    }
}

/// TS `captainShanks` (`src/data/cards/captains.ts:129`) — `CAP-SHANKS`.
pub fn captain_shanks() -> CaptainDef {
    CaptainDef {
        id: "CAP-SHANKS".to_string(),
        name: "Shanks (Akagami)".to_string(),
        faction: Faction::Pirate,
        tags: Some(vec!["redhair".to_string()]),
        traits: Some(vec![]),

        recto: CaptainRecto {
            pv: 35,
            atk: 6,
            def: 3,
            passive: PassiveDef {
                name: "Volonté de l'Empereur".to_string(),
                description: "Vos personnages gagnent +1 ATK.".to_string(),
                effects: vec![PassiveEffect::BuffAlly {
                    stat: BuffStat::Atk,
                    amount: 1,
                    filter: None,
                }],
            },
            attacks: vec![],
            surcharge: None,
        },

        flip_condition: FlipCondition {
            cost: Some(4),
            free_if_turn_gte: Some(7),
            ..Default::default()
        },

        verso: CaptainVerso {
            pv: 30,
            atk: 8,
            def: 4,
            passive: PassiveDef {
                name: "Présence de l'Empereur".to_string(),
                description: "Les ennemis adjacents à Shanks ont -2 ATK.".to_string(),
                effects: vec![PassiveEffect::DebuffAdjacentEnemies { amount: 2 }],
            },
            entry_effect: EntryEffect::Haoshoku {
                immobilize_max_def: 2,
                debuff_atk: 2,
            },
            base_action: BaseAction {
                name: "Coup de Sabre".to_string(),
                atk: 8,
                description: Some("Le sabre de l'Empereur (touche les Logia).".to_string()),
                ..Default::default()
            },
            special_attack: SpecialAttack {
                name: "Divin Départ".to_string(),
                cost: 4,
                atk_bonus: 4,
                ignore_def: Some(99),
                ignore_shield: Some(true),
                description: Some("Ignore la DEF et le Bouclier.".to_string()),
                ..Default::default()
            },
            surcharge: None,
            traits: Some(vec![Trait::Conqueror]),
            natural_haki: Some(vec![HakiType::Armament]),
        },
    }
}

/// TS `allCaptains: CaptainDef[] = [captainLuffy, captainAkainu,
/// captainCrocodile, captainShanks]` (`src/data/cards/captains.ts:166`).
///
/// The order is load-bearing — `CardRegistry::register_captains` iterates it.
pub fn captains() -> Vec<CaptainDef> {
    vec![
        captain_luffy(),
        captain_akainu(),
        captain_crocodile(),
        captain_shanks(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn captain_count_and_order_match_ts() {
        let caps = captains();
        assert_eq!(caps.len(), 4);
        let ids: Vec<&str> = caps.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["CAP-LUFFY", "CAP-AKAINU", "CAP-CROCODILE", "CAP-SHANKS"]
        );
    }

    #[test]
    fn captain_ids_are_unique() {
        let caps = captains();
        let unique: BTreeSet<&str> = caps.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(unique.len(), caps.len());
    }

    #[test]
    fn recto_never_attacks_and_has_no_surcharge() {
        // Rulebook v3.1 §2.1 — every recto has an empty `attacks` array and no
        // surcharge in the TS data.
        for c in captains() {
            assert!(c.recto.attacks.is_empty(), "{} recto attacks", c.id);
            assert!(c.recto.surcharge.is_none(), "{} recto surcharge", c.id);
            assert!(c.verso.surcharge.is_none(), "{} verso surcharge", c.id);
            assert_eq!(c.recto.passive.effects.len(), 1, "{} recto passive", c.id);
        }
    }

    #[test]
    fn flip_conditions_match_ts() {
        let caps = captains();
        assert_eq!(
            caps[0].flip_condition,
            FlipCondition {
                cost: Some(2),
                free_if_ally_ko: Some(true),
                ..Default::default()
            }
        );
        assert_eq!(
            caps[1].flip_condition,
            FlipCondition {
                cost: Some(3),
                free_if_enemy_cursed: Some(true),
                ..Default::default()
            }
        );
        assert_eq!(
            caps[2].flip_condition,
            FlipCondition {
                cost: Some(3),
                free_if_allies_gte: Some(3),
                ..Default::default()
            }
        );
        assert_eq!(
            caps[3].flip_condition,
            FlipCondition {
                cost: Some(4),
                free_if_turn_gte: Some(7),
                ..Default::default()
            }
        );
    }

    #[test]
    fn stat_lines_match_ts() {
        let caps = captains();
        let recto: Vec<(i32, i32, i32)> = caps
            .iter()
            .map(|c| (c.recto.pv, c.recto.atk, c.recto.def))
            .collect();
        assert_eq!(recto, vec![(30, 4, 2), (35, 5, 4), (30, 5, 3), (35, 6, 3)]);
        let verso: Vec<(i32, i32, i32)> = caps
            .iter()
            .map(|c| (c.verso.pv, c.verso.atk, c.verso.def))
            .collect();
        assert_eq!(verso, vec![(25, 6, 2), (28, 8, 3), (25, 7, 2), (30, 8, 4)]);
    }

    #[test]
    fn base_action_atk_matches_verso_atk() {
        // Every captain's verso baseAction.atk equals verso.atk in the TS data.
        for c in captains() {
            assert_eq!(c.verso.base_action.atk, c.verso.atk, "{}", c.id);
            assert!(c.verso.base_action.is_support.is_none(), "{}", c.id);
        }
    }

    #[test]
    fn luffy_entry_effect_is_multi_rush_then_damage() {
        let luffy = captain_luffy();
        match &luffy.verso.entry_effect {
            EntryEffect::Multi { effects } => {
                assert_eq!(effects.len(), 2);
                assert_eq!(effects[0], EntryEffect::GrantSelfRush);
                assert_eq!(
                    effects[1],
                    EntryEffect::DamageEnemies {
                        amount: 3,
                        target: EntryDamageTarget::Single,
                        cursed_bonus: None,
                        sand: None,
                    }
                );
            }
            other => panic!("expected multi, got {other:?}"),
        }
        assert_eq!(luffy.verso.special_attack.pushback, Some(true));
        assert_eq!(
            luffy.verso.special_attack.attack_traits,
            Some(vec![AttackTrait::Impact])
        );
        assert_eq!(luffy.verso.special_attack.cost, 3);
    }

    #[test]
    fn akainu_entry_has_cursed_bonus_and_croc_has_sand() {
        assert_eq!(
            captain_akainu().verso.entry_effect,
            EntryEffect::DamageEnemies {
                amount: 4,
                target: EntryDamageTarget::Single,
                cursed_bonus: Some(6),
                sand: None,
            }
        );
        assert_eq!(
            captain_crocodile().verso.entry_effect,
            EntryEffect::DamageEnemies {
                amount: 4,
                target: EntryDamageTarget::Single,
                cursed_bonus: None,
                sand: Some(true),
            }
        );
        assert_eq!(
            captain_crocodile().verso.special_attack.permanent_pv_loss,
            Some(2)
        );
        assert_eq!(
            captain_crocodile().verso.special_attack.element,
            Some(Element::Sand)
        );
    }

    #[test]
    fn shanks_haoshoku_and_natural_haki() {
        let shanks = captain_shanks();
        assert_eq!(
            shanks.verso.entry_effect,
            EntryEffect::Haoshoku {
                immobilize_max_def: 2,
                debuff_atk: 2,
            }
        );
        assert_eq!(shanks.verso.natural_haki, Some(vec![HakiType::Armament]));
        assert_eq!(shanks.verso.special_attack.ignore_def, Some(99));
        assert_eq!(shanks.verso.special_attack.ignore_shield, Some(true));
        // Recto passive has no filter — buffs every ally.
        assert_eq!(
            shanks.recto.passive.effects[0],
            PassiveEffect::BuffAlly {
                stat: BuffStat::Atk,
                amount: 1,
                filter: None,
            }
        );
    }

    #[test]
    fn only_shanks_declares_natural_haki() {
        for c in captains() {
            if c.id == "CAP-SHANKS" {
                assert!(c.verso.natural_haki.is_some());
            } else {
                assert!(c.verso.natural_haki.is_none(), "{}", c.id);
            }
        }
    }

    #[test]
    fn verso_traits_match_ts() {
        let caps = captains();
        assert_eq!(
            caps[0].verso.traits,
            Some(vec![Trait::Cursed, Trait::Conqueror])
        );
        assert_eq!(
            caps[1].verso.traits,
            Some(vec![Trait::Cursed, Trait::Logia])
        );
        assert_eq!(
            caps[2].verso.traits,
            Some(vec![Trait::Cursed, Trait::Logia])
        );
        assert_eq!(caps[3].verso.traits, Some(vec![Trait::Conqueror]));
        // Top-level traits: only Luffy carries `conqueror`, the rest are empty.
        assert_eq!(caps[0].traits, Some(vec![Trait::Conqueror]));
        assert_eq!(caps[1].traits, Some(vec![]));
        assert_eq!(caps[2].traits, Some(vec![]));
        assert_eq!(caps[3].traits, Some(vec![]));
    }

    #[test]
    fn french_strings_are_byte_exact() {
        let caps = captains();
        assert_eq!(caps[0].recto.passive.name, "Pavillon au Chapeau de Paille");
        assert_eq!(
            caps[0].verso.passive.description,
            "Immunisé contre l'Impact. Quand un Mugiwara allié est KO : Luffy gagne +1 ATK permanent (max +3)."
        );
        assert_eq!(
            caps[0].verso.special_attack.description.as_deref(),
            Some("Impact — repousse la cible d'un slot.")
        );
        assert_eq!(
            caps[1].verso.special_attack.description.as_deref(),
            Some("Les météores de magma. Zone · Feu.")
        );
        assert_eq!(caps[2].recto.passive.name, "Maître de Baroque Works");
        assert_eq!(caps[2].verso.passive.name, "Déshydratation");
        assert_eq!(caps[3].recto.passive.name, "Volonté de l'Empereur");
        assert_eq!(caps[3].verso.special_attack.name, "Divin Départ");
    }
}
