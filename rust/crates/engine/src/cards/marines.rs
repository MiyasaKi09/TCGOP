//! ST02 — Marines card data.
//!
//! Port of `src/data/cards/marines.ts` (`marinesCards`) — 28 card
//! definitions, in the exact source order (the order is load-bearing: the
//! registry is filled by iterating it, and later ids overwrite earlier ones).

use crate::types::{
    AllyFilter, AtkDefStat, AttackTrait, BaseAction, BuffDuration, CardDef, CardType,
    CounterEffect, DamageTarget, Element, EventEffect, Faction, FruitAwakening,
    FruitAwakeningSpecialAttack, FruitBaseEffects, FruitEffects, HakiType, ObjectSubtype,
    PassiveDef, PassiveEffect, Rarity, Row, ShipActive, ShipDestroyEffect, SpecialAttack,
    SynergyDef, Trait,
};

fn s(v: &str) -> String {
    v.to_string()
}

fn tags(v: &[&str]) -> Option<Vec<String>> {
    Some(v.iter().map(|t| t.to_string()).collect())
}

/// TS `marinesCards: CardDef[]` (`src/data/cards/marines.ts`).
///
/// Returns the 28 definitions of the ST02 — Marines set in source order.
pub fn cards() -> Vec<CardDef> {
    vec![
        // === PERSONNAGES ===
        // MR-001 Coby (marines.ts:6)
        CardDef {
            atk: Some(2),
            def: Some(1),
            pv: Some(4),
            tags: tags(&["marine", "soldat"]),
            preferred_row: Some(Row::Front),
            synergies: Some(vec![SynergyDef {
                partner_id: s("MR-002"),
                atk_bonus: 1,
                on_partner_ko: None,
            }]),
            base_action: Some(BaseAction {
                name: s("Coup de Poing"),
                atk: 2,
                description: Some(s("Le futur amiral s'entraîne.")),
                ..Default::default()
            }),
            special_attack: Some(SpecialAttack {
                name: s("Poing de la Justice"),
                cost: 2,
                atk_bonus: 3,
                attack_traits: Some(vec![AttackTrait::Piercing]),
                description: Some(s("Perçant.")),
                ..Default::default()
            }),
            ..CardDef::new(
                "MR-001",
                "Coby",
                CardType::Character,
                2,
                Faction::Marine,
                Rarity::C,
                "ST02",
            )
        },
        // MR-002 Helmeppo (marines.ts:14)
        CardDef {
            atk: Some(3),
            def: Some(2),
            pv: Some(4),
            tags: tags(&["marine", "soldat", "bretteur"]),
            preferred_row: Some(Row::Front),
            synergies: Some(vec![SynergyDef {
                partner_id: s("MR-001"),
                atk_bonus: 1,
                on_partner_ko: None,
            }]),
            base_action: Some(BaseAction {
                name: s("Coup de Lame"),
                atk: 3,
                description: Some(s("Le fils prodigue.")),
                ..Default::default()
            }),
            special_attack: Some(SpecialAttack {
                name: s("Frappe Jumelle"),
                cost: 2,
                atk_bonus: 2,
                two_targets: Some(true),
                description: Some(s("Touche 2 cibles.")),
                ..Default::default()
            }),
            ..CardDef::new(
                "MR-002",
                "Helmeppo",
                CardType::Character,
                2,
                Faction::Marine,
                Rarity::C,
                "ST02",
            )
        },
        // MR-003 Tashigi (marines.ts:22)
        CardDef {
            atk: Some(4),
            def: Some(2),
            pv: Some(5),
            tags: tags(&["marine", "capitaine_marine", "bretteur"]),
            preferred_row: Some(Row::Front),
            passive: Some(PassiveDef {
                name: s("Chasseuse de Meito"),
                description: s("Peut équiper 2 Armes au lieu d'une."),
                effects: vec![PassiveEffect::TwoWeaponSlots],
            }),
            base_action: Some(BaseAction {
                name: s("Coup de Sabre"),
                atk: 4,
                description: Some(s("La lame précise.")),
                ..Default::default()
            }),
            special_attack: Some(SpecialAttack {
                name: s("Tenpô"),
                cost: 2,
                atk_bonus: 3,
                attack_traits: Some(vec![AttackTrait::Piercing]),
                description: Some(s("Perçant.")),
                ..Default::default()
            }),
            ..CardDef::new(
                "MR-003",
                "Tashigi",
                CardType::Character,
                3,
                Faction::Marine,
                Rarity::U,
                "ST02",
            )
        },
        // MR-004 Smoker (marines.ts:30)
        CardDef {
            atk: Some(5),
            def: Some(3),
            pv: Some(7),
            tags: tags(&["marine", "capitaine_marine"]),
            preferred_row: Some(Row::Front),
            passive: Some(PassiveDef {
                name: s("White Snake"),
                description: s("Les attaques de Smoker ignorent le Furtif et l'Inciblable."),
                effects: vec![PassiveEffect::StripStealthOnAttack],
            }),
            base_action: Some(BaseAction {
                name: s("Jitte"),
                atk: 5,
                description: Some(s("La matraque en granit marin.")),
                ..Default::default()
            }),
            special_attack: Some(SpecialAttack {
                name: s("White Blow"),
                cost: 2,
                atk_bonus: 3,
                immobilize: Some(true),
                description: Some(s("La cible perd sa prochaine action.")),
                ..Default::default()
            }),
            ..CardDef::new(
                "MR-004",
                "Smoker",
                CardType::Character,
                4,
                Faction::Marine,
                Rarity::R,
                "ST02",
            )
        },
        // MR-005 Sentomaru (marines.ts:38)
        CardDef {
            atk: Some(4),
            def: Some(4),
            pv: Some(7),
            tags: tags(&["marine", "garde"]),
            preferred_row: Some(Row::Front),
            natural_haki: Some(vec![HakiType::Armament]),
            passive: Some(PassiveDef {
                name: s("Garde du Corps"),
                description: s("Haki Armement naturel (touche les Logia)."),
                effects: vec![PassiveEffect::NaturalHaki {
                    haki_type: HakiType::Armament,
                }],
            }),
            base_action: Some(BaseAction {
                name: s("Coup de Hache"),
                atk: 4,
                description: Some(s("La hache de la garde.")),
                ..Default::default()
            }),
            special_attack: Some(SpecialAttack {
                name: s("Ashigara Dokkoi"),
                cost: 3,
                atk_bonus: 3,
                ignore_shield: Some(true),
                description: Some(s("Ignore le Bouclier.")),
                ..Default::default()
            }),
            ..CardDef::new(
                "MR-005",
                "Sentomaru",
                CardType::Character,
                3,
                Faction::Marine,
                Rarity::U,
                "ST02",
            )
        },
        // MR-006 Momonga (marines.ts:47)
        CardDef {
            atk: Some(5),
            def: Some(3),
            pv: Some(6),
            tags: tags(&["marine", "vice_amiral", "bretteur"]),
            preferred_row: Some(Row::Front),
            passive: Some(PassiveDef {
                name: s("Volonté de Fer"),
                description: s("Immunisé contre tous les effets de contrôle."),
                effects: vec![PassiveEffect::ImmuneControl],
            }),
            base_action: Some(BaseAction {
                name: s("Coup de Sabre"),
                atk: 5,
                description: Some(s("La volonté triomphe de la chair.")),
                ..Default::default()
            }),
            special_attack: Some(SpecialAttack {
                name: s("Frappe Disciplinée"),
                cost: 2,
                atk_bonus: 3,
                attack_traits: Some(vec![AttackTrait::Piercing]),
                description: Some(s("Perçant.")),
                ..Default::default()
            }),
            ..CardDef::new(
                "MR-006",
                "Momonga",
                CardType::Character,
                3,
                Faction::Marine,
                Rarity::U,
                "ST02",
            )
        },
        // MR-007 Garp (marines.ts:55)
        CardDef {
            atk: Some(7),
            def: Some(3),
            pv: Some(10),
            tags: tags(&["marine", "vice_amiral"]),
            preferred_row: Some(Row::Front),
            natural_haki: Some(vec![HakiType::Armament]),
            passive: Some(PassiveDef {
                name: s("Poing de Garp"),
                description: s("Haki Armement naturel (toutes ses attaques touchent les Logia)."),
                effects: vec![PassiveEffect::NaturalHaki {
                    haki_type: HakiType::Armament,
                }],
            }),
            base_action: Some(BaseAction {
                name: s("Poing de l'Amour"),
                atk: 7,
                attack_traits: Some(vec![AttackTrait::Impact]),
                description: Some(s("Le poing d'amour de grand-père.")),
                ..Default::default()
            }),
            special_attack: Some(SpecialAttack {
                name: s("Boulet de Canon"),
                cost: 3,
                atk_bonus: 3,
                attack_traits: Some(vec![AttackTrait::Range, AttackTrait::Impact]),
                pushback: Some(true),
                description: Some(s("Portée · Impact.")),
                ..Default::default()
            }),
            ..CardDef::new(
                "MR-007",
                "Garp",
                CardType::Character,
                5,
                Faction::Marine,
                Rarity::Sr,
                "ST02",
            )
        },
        // MR-008 Kizaru (Borsalino) (marines.ts:64)
        CardDef {
            atk: Some(6),
            def: Some(2),
            pv: Some(8),
            traits: Some(vec![Trait::Cursed, Trait::Logia]),
            tags: tags(&["marine", "amiral"]),
            preferred_row: Some(Row::Front),
            passive: Some(PassiveDef {
                name: s("Vitesse de la Lumière"),
                description: s("Les attaques de Kizaru ne peuvent pas être esquivées."),
                effects: vec![PassiveEffect::LogiaIntangibility, PassiveEffect::NoDodge],
            }),
            base_action: Some(BaseAction {
                name: s("Coup de Pied Lumineux"),
                atk: 6,
                description: Some(s("La vitesse de la lumière.")),
                ..Default::default()
            }),
            special_attack: Some(SpecialAttack {
                name: s("Yasakani no Magatama"),
                cost: 4,
                atk_bonus: 4,
                attack_traits: Some(vec![AttackTrait::Zone, AttackTrait::Range]),
                description: Some(s("La pluie de lasers.")),
                ..Default::default()
            }),
            ..CardDef::new(
                "MR-008",
                "Kizaru (Borsalino)",
                CardType::Character,
                5,
                Faction::Marine,
                Rarity::Sr,
                "ST02",
            )
        },
        // MR-009 Aokiji (Kuzan) (marines.ts:72)
        CardDef {
            atk: Some(6),
            def: Some(3),
            pv: Some(9),
            traits: Some(vec![Trait::Cursed, Trait::Logia]),
            tags: tags(&["marine", "amiral"]),
            preferred_row: Some(Row::Front),
            passive: Some(PassiveDef {
                name: s("Souffle Glacial"),
                description: s("Intangibilité Logia. Toutes ses attaques ont l'élément Glace."),
                effects: vec![PassiveEffect::LogiaIntangibility],
            }),
            base_action: Some(BaseAction {
                name: s("Ice Time"),
                atk: 6,
                element: Some(Element::Ice),
                description: Some(s("La cible perd sa prochaine action.")),
                ..Default::default()
            }),
            special_attack: Some(SpecialAttack {
                name: s("Ice Age"),
                cost: 4,
                atk_bonus: 3,
                element: Some(Element::Ice),
                attack_traits: Some(vec![AttackTrait::Zone]),
                description: Some(s("Le gel se propage.")),
                ..Default::default()
            }),
            ..CardDef::new(
                "MR-009",
                "Aokiji (Kuzan)",
                CardType::Character,
                5,
                Faction::Marine,
                Rarity::Sr,
                "ST02",
            )
        },
        // MR-010 Sengoku (marines.ts:80)
        CardDef {
            atk: Some(6),
            def: Some(4),
            pv: Some(9),
            tags: tags(&["marine", "amiral_en_chef"]),
            preferred_row: Some(Row::Front),
            passive: Some(PassiveDef {
                name: s("Stratège Suprême"),
                description: s("Vos personnages Marine gagnent +1 DEF."),
                effects: vec![PassiveEffect::BuffAlly {
                    stat: crate::types::BuffStat::Def,
                    amount: 1,
                    filter: Some(AllyFilter {
                        faction: Some(Faction::Marine),
                        tag: None,
                        trait_: None,
                    }),
                }],
            }),
            base_action: Some(BaseAction {
                name: s("Onde de Choc"),
                atk: 6,
                description: Some(s("Le Bouddha frappe.")),
                ..Default::default()
            }),
            special_attack: Some(SpecialAttack {
                name: s("Daibutsu : Paume de l'Illumination"),
                cost: 4,
                atk_bonus: 4,
                attack_traits: Some(vec![AttackTrait::Zone, AttackTrait::Impact]),
                description: Some(s("Zone · Impact.")),
                ..Default::default()
            }),
            ..CardDef::new(
                "MR-010",
                "Sengoku",
                CardType::Character,
                5,
                Faction::Marine,
                Rarity::Sr,
                "ST02",
            )
        },
        // === OBJETS - FRUITS DU DÉMON ===
        // MR-011 Magu Magu no Mi (marines.ts:90)
        CardDef {
            subtype: Some(ObjectSubtype::Fruit),
            bonus_atk: Some(0),
            restriction: Some(s("Akainu")),
            equip_effect: Some(s("Applique Logia + Maudit. Attaques gagnent Feu.")),
            fruit_effects: Some(FruitEffects {
                base: FruitBaseEffects {
                    grants_traits: Some(vec![Trait::Cursed, Trait::Logia]),
                    passive_description: Some(s("Logia. Meigo : 2 Vol · ATK +3 · Feu.")),
                    atk_bonus: None,
                    def_bonus: None,
                },
                awakening: Some(FruitAwakening {
                    porteur_legitime: s("Akainu"),
                    min_turns: 5,
                    vol_cost: 3,
                    grants_traits: None,
                    atk_bonus: Some(4),
                    def_bonus: None,
                    passive_description: Some(s(
                        "Terre Brûlée — les ennemis adjacents subissent 1 dégât/tour (Feu).",
                    )),
                    special_attack: Some(FruitAwakeningSpecialAttack {
                        name: s("Inugami Guren"),
                        cost: 4,
                        atk_bonus: 6,
                        description: s("Zone · Perçant · Feu."),
                        once_per_game: Some(true),
                        attack_traits: Some(vec![AttackTrait::Zone, AttackTrait::Piercing]),
                        element: Some(Element::Fire),
                        ignore_def: None,
                        immobilize: None,
                        sleep: None,
                        pushback: None,
                        ignore_shield: None,
                        strip_stealth: None,
                        permanent_pv_loss: None,
                        no_heal: None,
                    }),
                }),
            }),
            ..CardDef::new(
                "MR-011",
                "Magu Magu no Mi",
                CardType::Object,
                3,
                Faction::Marine,
                Rarity::Sr,
                "ST02",
            )
        },
        // MR-012 Moku Moku no Mi (marines.ts:103)
        CardDef {
            subtype: Some(ObjectSubtype::Fruit),
            bonus_atk: Some(0),
            restriction: Some(s("Smoker")),
            equip_effect: Some(s("Applique Logia + Maudit.")),
            fruit_effects: Some(FruitEffects {
                base: FruitBaseEffects {
                    grants_traits: Some(vec![Trait::Cursed, Trait::Logia]),
                    passive_description: Some(s(
                        "Logia. White Out : 1 Vol · ATK +2 · la cible ne peut pas se déplacer.",
                    )),
                    atk_bonus: None,
                    def_bonus: None,
                },
                awakening: Some(FruitAwakening {
                    porteur_legitime: s("Smoker"),
                    min_turns: 5,
                    vol_cost: 2,
                    grants_traits: None,
                    atk_bonus: Some(3),
                    def_bonus: None,
                    passive_description: Some(s(
                        "White Monster — les ennemis ne peuvent pas quitter un slot adjacent au porteur.",
                    )),
                    special_attack: Some(FruitAwakeningSpecialAttack {
                        name: s("White Launcher"),
                        cost: 3,
                        atk_bonus: 3,
                        description: s("Portée · immobilise 1 tour."),
                        once_per_game: None,
                        attack_traits: Some(vec![AttackTrait::Range]),
                        element: None,
                        ignore_def: None,
                        immobilize: Some(true),
                        sleep: None,
                        pushback: None,
                        ignore_shield: None,
                        strip_stealth: None,
                        permanent_pv_loss: None,
                        no_heal: None,
                    }),
                }),
            }),
            ..CardDef::new(
                "MR-012",
                "Moku Moku no Mi",
                CardType::Object,
                2,
                Faction::Marine,
                Rarity::R,
                "ST02",
            )
        },
        // === OBJETS - ARMES ===
        // MR-013 Shigure (marines.ts:118)
        CardDef {
            subtype: Some(ObjectSubtype::Weapon),
            bonus_atk: Some(1),
            restriction: Some(s("bretteur")),
            equip_effect: Some(s("+1 ATK. Si équipée par Tashigi : +1 ATK de plus.")),
            ..CardDef::new(
                "MR-013",
                "Shigure",
                CardType::Object,
                1,
                Faction::Marine,
                Rarity::C,
                "ST02",
            )
        },
        // MR-014 Jitte Granit Marin (marines.ts:123)
        CardDef {
            subtype: Some(ObjectSubtype::Weapon),
            bonus_atk: Some(1),
            grants_element: Some(Element::Water),
            equip_effect: Some(s(
                "+1 ATK. Granit Marin : x2 dégâts aux Maudits et touche les Logia.",
            )),
            ..CardDef::new(
                "MR-014",
                "Jitte Granit Marin",
                CardType::Object,
                1,
                Faction::Marine,
                Rarity::U,
                "ST02",
            )
        },
        // === OBJETS - ACCESSOIRES ===
        // MR-015 Menottes Granit Marin (marines.ts:130)
        CardDef {
            subtype: Some(ObjectSubtype::Accessory),
            bonus_atk: Some(0),
            equip_effect: Some(s(
                "Actif (1x/partie) : un ennemi Maudit perd ses traits et son action à son prochain tour.",
            )),
            ..CardDef::new(
                "MR-015",
                "Menottes Granit Marin",
                CardType::Object,
                2,
                Faction::Marine,
                Rarity::R,
                "ST02",
            )
        },
        // MR-016 Canon Marine (marines.ts:135)
        CardDef {
            subtype: Some(ObjectSubtype::Accessory),
            bonus_atk: Some(0),
            equip_effect: Some(s(
                "Le porteur gagne une attaque : Tir de Canon — 1 Vol · 3 dégâts (Portée).",
            )),
            ..CardDef::new(
                "MR-016",
                "Canon Marine",
                CardType::Object,
                1,
                Faction::Marine,
                Rarity::C,
                "ST02",
            )
        },
        // MR-017 Boulet Granit Marin (marines.ts:140)
        CardDef {
            subtype: Some(ObjectSubtype::Accessory),
            bonus_atk: Some(0),
            equip_effect: Some(s(
                "1x/partie : 2 dégâts à un ennemi ; s'il est Maudit, 4 dégâts et il perd ses traits ce tour.",
            )),
            ..CardDef::new(
                "MR-017",
                "Boulet Granit Marin",
                CardType::Object,
                1,
                Faction::Marine,
                Rarity::C,
                "ST02",
            )
        },
        // === NAVIRES ===
        // MR-018 Navire de Guerre (marines.ts:147)
        CardDef {
            ship_passive: Some(s("Vos Marine gagnent +1 ATK.")),
            ship_active: Some(ShipActive {
                name: s("Salve de Canons"),
                cost: 2,
                description: s("3 dégâts à un ennemi (Portée)."),
                once_per_game: Some(true),
            }),
            ..CardDef::new(
                "MR-018",
                "Navire de Guerre",
                CardType::Ship,
                2,
                Faction::Marine,
                Rarity::U,
                "ST02",
            )
        },
        // MR-019 Navire de Justice (marines.ts:153)
        CardDef {
            ship_passive: Some(s("Vos Marine gagnent +1 PV.")),
            ship_destroy_effect: Some(ShipDestroyEffect {
                deploy_token: Some(s("TOK-MARINE")),
                ..Default::default()
            }),
            ..CardDef::new(
                "MR-019",
                "Navire de Justice",
                CardType::Ship,
                1,
                Faction::Marine,
                Rarity::C,
                "ST02",
            )
        },
        // === EVENEMENTS ===
        // MR-020 Buster Call (marines.ts:161)
        CardDef {
            event_effect: Some(EventEffect::DamageEnemies {
                amount: 4,
                target: DamageTarget::All,
                cursed_bonus: None,
                sand: None,
                destroy_ships: Some(true),
            }),
            ..CardDef::new(
                "MR-020",
                "Buster Call",
                CardType::Event,
                5,
                Faction::Marine,
                Rarity::Sr,
                "ST02",
            )
        },
        // MR-021 Promotion (marines.ts:166)
        CardDef {
            event_effect: Some(EventEffect::BuffSingle {
                stat: AtkDefStat::Atk,
                amount: 1,
                duration: BuffDuration::Permanent,
                requires_own_ko: None,
            }),
            ..CardDef::new(
                "MR-021",
                "Promotion",
                CardType::Event,
                1,
                Faction::Marine,
                Rarity::C,
                "ST02",
            )
        },
        // MR-022 Justice Absolue (marines.ts:171)
        CardDef {
            event_effect: Some(EventEffect::BuffAllies {
                stat: AtkDefStat::Atk,
                amount: 2,
                filter: Some(AllyFilter {
                    faction: Some(Faction::Marine),
                    tag: None,
                    trait_: None,
                }),
                duration: BuffDuration::Turn,
            }),
            ..CardDef::new(
                "MR-022",
                "Justice Absolue",
                CardType::Event,
                2,
                Faction::Marine,
                Rarity::U,
                "ST02",
            )
        },
        // MR-023 Renforts (marines.ts:176)
        CardDef {
            event_effect: Some(EventEffect::DeployTokens {
                token_id: s("TOK-MARINE"),
                count: 2,
            }),
            ..CardDef::new(
                "MR-023",
                "Renforts",
                CardType::Event,
                1,
                Faction::Marine,
                Rarity::C,
                "ST02",
            )
        },
        // MR-024 Ordre de Tir (marines.ts:181)
        CardDef {
            event_effect: Some(EventEffect::Custom {
                id: s("coordinatedFire"),
                description: s("Chaque Marine inflige 1 dégât à un ennemi."),
            }),
            ..CardDef::new(
                "MR-024",
                "Ordre de Tir",
                CardType::Event,
                2,
                Faction::Marine,
                Rarity::U,
                "ST02",
            )
        },
        // MR-027 Embargo (marines.ts:186)
        CardDef {
            event_effect: Some(EventEffect::Custom {
                id: s("embargo"),
                description: s(
                    "L'adversaire ne peut ni équiper ni jouer de Navire à son prochain tour.",
                ),
            }),
            ..CardDef::new(
                "MR-027",
                "Embargo",
                CardType::Event,
                2,
                Faction::Marine,
                Rarity::U,
                "ST02",
            )
        },
        // MR-028 Exécution Publique (marines.ts:191)
        CardDef {
            event_effect: Some(EventEffect::Custom {
                id: s("execute4"),
                description: s("Détruisez un ennemi de PV actuels ≤ 4."),
            }),
            ..CardDef::new(
                "MR-028",
                "Exécution Publique",
                CardType::Event,
                3,
                Faction::Marine,
                Rarity::R,
                "ST02",
            )
        },
        // === COUNTERS ===
        // MR-025 Manteau de Justice (marines.ts:198)
        CardDef {
            counter_effect: Some(CounterEffect::Cancel {
                description: s("Annule l'attaque. 1x par partie."),
                max_attacker_atk: None,
                self_captain_damage: None,
                once: Some(true),
            }),
            ..CardDef::new(
                "MR-025",
                "Manteau de Justice",
                CardType::Counter,
                0,
                Faction::Marine,
                Rarity::R,
                "ST02",
            )
        },
        // MR-026 Mur d'Acier (marines.ts:203)
        CardDef {
            counter_effect: Some(CounterEffect::ReduceDamage {
                amount: 5,
                captain_bonus: None,
            }),
            ..CardDef::new(
                "MR-026",
                "Mur d'Acier",
                CardType::Counter,
                1,
                Faction::Marine,
                Rarity::C,
                "ST02",
            )
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// `marinesCards.length === 28`.
    #[test]
    fn card_count_matches_ts() {
        assert_eq!(cards().len(), 28);
    }

    /// Every id is unique (the registry would silently overwrite duplicates).
    #[test]
    fn ids_are_unique() {
        let all = cards();
        let set: BTreeSet<&str> = all.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(set.len(), all.len(), "duplicate id in marinesCards");
    }

    /// Source order is load-bearing: MR-001..MR-024, then MR-027, MR-028,
    /// then the two counters MR-025, MR-026.
    #[test]
    fn source_order_matches_ts() {
        let ids: Vec<String> = cards().into_iter().map(|c| c.id).collect();
        let mut expected: Vec<String> = (1..=24).map(|n| format!("MR-{n:03}")).collect();
        expected.push("MR-027".to_string());
        expected.push("MR-028".to_string());
        expected.push("MR-025".to_string());
        expected.push("MR-026".to_string());
        assert_eq!(ids, expected);
    }

    /// Every card id referenced by `marinesDeck` resolves in this set.
    #[test]
    fn deck_references_resolve() {
        let all = cards();
        let ids: BTreeSet<&str> = all.iter().map(|c| c.id.as_str()).collect();
        let deck = crate::decks::marines_deck();
        for entry in &deck.cards {
            assert!(
                ids.contains(entry.card_id.as_str()),
                "marinesDeck references unknown card {}",
                entry.card_id
            );
        }
    }

    /// Type/set/faction invariants of ST02 (every card is Marine / ST02, and
    /// the split is 10 characters, 7 objects, 2 ships, 7 events, 2 counters).
    #[test]
    fn type_breakdown_matches_ts() {
        let all = cards();
        assert!(all.iter().all(|c| c.faction == Faction::Marine));
        assert!(all.iter().all(|c| c.set == "ST02"));
        let count = |t: CardType| all.iter().filter(|c| c.card_type == t).count();
        assert_eq!(count(CardType::Character), 10);
        assert_eq!(count(CardType::Object), 7);
        assert_eq!(count(CardType::Ship), 2);
        assert_eq!(count(CardType::Event), 7);
        assert_eq!(count(CardType::Counter), 2);
    }

    /// Objects carry a subtype; the two fruits carry the full `fruitEffects`
    /// payload (base traits + awakening special).
    #[test]
    fn fruits_are_fully_transcribed() {
        let all = cards();
        let magu = all.iter().find(|c| c.id == "MR-011").unwrap();
        let fx = magu.fruit_effects.as_ref().unwrap();
        assert_eq!(
            fx.base.grants_traits.as_deref(),
            Some(&[Trait::Cursed, Trait::Logia][..])
        );
        let aw = fx.awakening.as_ref().unwrap();
        assert_eq!(aw.porteur_legitime, "Akainu");
        assert_eq!(aw.min_turns, 5);
        assert_eq!(aw.vol_cost, 3);
        assert_eq!(aw.atk_bonus, Some(4));
        let sp = aw.special_attack.as_ref().unwrap();
        assert_eq!(sp.name, "Inugami Guren");
        assert_eq!(sp.cost, 4);
        assert_eq!(sp.atk_bonus, 6);
        assert_eq!(sp.once_per_game, Some(true));
        assert_eq!(sp.element, Some(Element::Fire));

        let moku = all.iter().find(|c| c.id == "MR-012").unwrap();
        let aw = moku
            .fruit_effects
            .as_ref()
            .unwrap()
            .awakening
            .as_ref()
            .unwrap();
        assert_eq!(aw.porteur_legitime, "Smoker");
        assert_eq!(aw.vol_cost, 2);
        let sp = aw.special_attack.as_ref().unwrap();
        assert_eq!(sp.immobilize, Some(true));
        assert_eq!(sp.once_per_game, None);
    }

    /// The French strings are byte-exact (accents included).
    #[test]
    fn french_strings_are_byte_exact() {
        let all = cards();
        let coby = all.iter().find(|c| c.id == "MR-001").unwrap();
        assert_eq!(
            coby.base_action.as_ref().unwrap().description.as_deref(),
            Some("Le futur amiral s'entraîne.")
        );
        assert_eq!(
            coby.special_attack.as_ref().unwrap().description.as_deref(),
            Some("Perçant.")
        );
        let sengoku = all.iter().find(|c| c.id == "MR-010").unwrap();
        assert_eq!(sengoku.passive.as_ref().unwrap().name, "Stratège Suprême");
        assert_eq!(
            sengoku.special_attack.as_ref().unwrap().name,
            "Daibutsu : Paume de l'Illumination"
        );
        let exec = all.iter().find(|c| c.id == "MR-028").unwrap();
        assert_eq!(
            exec.event_effect,
            Some(EventEffect::Custom {
                id: "execute4".to_string(),
                description: "Détruisez un ennemi de PV actuels ≤ 4.".to_string(),
            })
        );
    }

    /// Synergy pair Coby <-> Helmeppo, both +1 ATK, no `onPartnerKO`.
    #[test]
    fn coby_helmeppo_synergy() {
        let all = cards();
        let coby = all.iter().find(|c| c.id == "MR-001").unwrap();
        let helm = all.iter().find(|c| c.id == "MR-002").unwrap();
        let cs = coby.synergies.as_ref().unwrap();
        let hs = helm.synergies.as_ref().unwrap();
        assert_eq!(cs.len(), 1);
        assert_eq!(hs.len(), 1);
        assert_eq!(cs[0].partner_id, "MR-002");
        assert_eq!(hs[0].partner_id, "MR-001");
        assert_eq!(cs[0].atk_bonus, 1);
        assert_eq!(hs[0].atk_bonus, 1);
        assert_eq!(cs[0].on_partner_ko, None);
        assert_eq!(hs[0].on_partner_ko, None);
    }

    /// Kizaru's passive holds BOTH effects, in TS order.
    #[test]
    fn kizaru_passive_effect_order() {
        let all = cards();
        let kizaru = all.iter().find(|c| c.id == "MR-008").unwrap();
        assert_eq!(
            kizaru.passive.as_ref().unwrap().effects,
            vec![PassiveEffect::LogiaIntangibility, PassiveEffect::NoDodge]
        );
        assert_eq!(
            kizaru.traits.as_deref(),
            Some(&[Trait::Cursed, Trait::Logia][..])
        );
    }

    /// `naturalHaki` is set on Sentomaru and Garp only.
    #[test]
    fn natural_haki_carriers() {
        let all = cards();
        let carriers: Vec<&str> = all
            .iter()
            .filter(|c| c.natural_haki.is_some())
            .map(|c| c.id.as_str())
            .collect();
        assert_eq!(carriers, vec!["MR-005", "MR-007"]);
        for id in carriers {
            let c = all.iter().find(|c| c.id == id).unwrap();
            assert_eq!(c.natural_haki.as_deref(), Some(&[HakiType::Armament][..]));
        }
    }
}
