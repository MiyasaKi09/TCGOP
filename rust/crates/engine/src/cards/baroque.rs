//! ST03 — Baroque Works card data.
//!
//! Port of `src/data/cards/baroque.ts` (`baroqueCards`) — 28 card
//! definitions, in the exact source order (the order is load-bearing: the
//! registry is filled by iterating it, and later ids overwrite earlier ones).

use crate::types::{
    AttackTrait, BaseAction, CardDef, CardType, CounterEffect, DamageTarget, Element, EventEffect,
    Faction, FruitAwakening, FruitAwakeningSpecialAttack, FruitBaseEffects, FruitEffects,
    ObjectSubtype, PassiveDef, PassiveEffect, Rarity, Row, ShipActive, SpecialAttack, Trait,
};

fn s(v: &str) -> String {
    v.to_string()
}

fn tags(v: &[&str]) -> Option<Vec<String>> {
    Some(v.iter().map(|t| t.to_string()).collect())
}

/// TS `baroqueCards: CardDef[]` (`src/data/cards/baroque.ts`).
///
/// Returns the 28 definitions of the ST03 — Baroque Works set in source order.
pub fn cards() -> Vec<CardDef> {
    vec![
        // === PERSONNAGES ===
        // BW-001 Mr. 1 (Daz Bonez) (baroque.ts:6)
        CardDef {
            atk: Some(6),
            def: Some(4),
            pv: Some(7),
            traits: Some(vec![Trait::Piercing]),
            tags: tags(&["baroque"]),
            preferred_row: Some(Row::Front),
            base_action: Some(BaseAction {
                name: s("Lame du Corps"),
                atk: 6,
                description: Some(s("Le corps de lames.")),
                ..Default::default()
            }),
            special_attack: Some(SpecialAttack {
                name: s("Atomic Spurt"),
                cost: 3,
                atk_bonus: 3,
                ignore_shield: Some(true),
                description: Some(s("Ignore le Bouclier.")),
                ..Default::default()
            }),
            ..CardDef::new(
                "BW-001",
                "Mr. 1 (Daz Bonez)",
                CardType::Character,
                4,
                Faction::Pirate,
                Rarity::R,
                "ST03",
            )
        },
        // BW-002 Miss Doublefinger (baroque.ts:13)
        CardDef {
            atk: Some(4),
            def: Some(2),
            pv: Some(5),
            tags: tags(&["baroque", "female"]),
            preferred_row: Some(Row::Front),
            passive: Some(PassiveDef {
                name: s("Épines"),
                description: s("Quand elle est attaquée en mêlée, l'attaquant subit 2 dégâts."),
                effects: vec![PassiveEffect::MeleeRecoil { amount: 2 }],
            }),
            base_action: Some(BaseAction {
                name: s("Stinger"),
                atk: 4,
                description: Some(s("Les épines jaillissent.")),
                ..Default::default()
            }),
            special_attack: Some(SpecialAttack {
                name: s("Tsubaki"),
                cost: 2,
                atk_bonus: 3,
                attack_traits: Some(vec![AttackTrait::Piercing]),
                description: Some(s("Perçant.")),
                ..Default::default()
            }),
            ..CardDef::new(
                "BW-002",
                "Miss Doublefinger",
                CardType::Character,
                3,
                Faction::Pirate,
                Rarity::U,
                "ST03",
            )
        },
        // BW-003 Mr. 2 (Bon Clay) (baroque.ts:21)
        CardDef {
            atk: Some(4),
            def: Some(2),
            pv: Some(6),
            tags: tags(&["baroque"]),
            preferred_row: Some(Row::Front),
            passive: Some(PassiveDef {
                name: s("Mane Mane"),
                description: s("Au déploiement, copie l'ATK d'un de vos autres personnages."),
                effects: vec![PassiveEffect::CopyAtkOnDeploy],
            }),
            base_action: Some(BaseAction {
                name: s("Coup de Ballet"),
                atk: 4,
                description: Some(s("La danse okama.")),
                ..Default::default()
            }),
            special_attack: Some(SpecialAttack {
                name: s("Okama Kenpo"),
                cost: 2,
                atk_bonus: 2,
                two_targets: Some(true),
                description: Some(s("Touche 2 cibles.")),
                ..Default::default()
            }),
            ..CardDef::new(
                "BW-003",
                "Mr. 2 (Bon Clay)",
                CardType::Character,
                3,
                Faction::Pirate,
                Rarity::U,
                "ST03",
            )
        },
        // BW-004 Mr. 3 (Galdino) (baroque.ts:29)
        CardDef {
            atk: Some(2),
            def: Some(3),
            pv: Some(5),
            traits: Some(vec![Trait::Shield]),
            tags: tags(&["baroque"]),
            preferred_row: Some(Row::Front),
            base_action: Some(BaseAction {
                name: s("Coup de Cire"),
                atk: 2,
                description: Some(s("Le mur de cire.")),
                ..Default::default()
            }),
            special_attack: Some(SpecialAttack {
                name: s("Candle Lock"),
                cost: 2,
                atk_bonus: 2,
                immobilize: Some(true),
                description: Some(s("La cible est immobilisée 1 tour.")),
                ..Default::default()
            }),
            ..CardDef::new(
                "BW-004",
                "Mr. 3 (Galdino)",
                CardType::Character,
                2,
                Faction::Pirate,
                Rarity::C,
                "ST03",
            )
        },
        // BW-005 Miss Goldenweek (baroque.ts:36)
        CardDef {
            atk: Some(1),
            def: Some(1),
            pv: Some(4),
            tags: tags(&["baroque", "female"]),
            preferred_row: Some(Row::Back),
            passive: Some(PassiveDef {
                name: s("Colors Trap"),
                description: s("Un ennemi a -2 ATK tant que Miss Goldenweek est en jeu."),
                effects: vec![PassiveEffect::DebuffOneEnemy { amount: 2 }],
            }),
            base_action: Some(BaseAction {
                name: s("Peinture du Rire"),
                atk: 0,
                is_support: Some(true),
                immobilize: Some(true),
                description: Some(s("Un ennemi perd sa prochaine action (figé de rire).")),
                ..Default::default()
            }),
            special_attack: Some(SpecialAttack {
                name: s("Peinture de la Colère"),
                cost: 1,
                atk_bonus: 0,
                is_support: Some(true),
                // Decision §8.38/§8.54: the printed text becomes a structured
                // field, in lockstep with `src/data/cards/baroque.ts`.
                taunt: Some(true),
                description: Some(s(
                    "Un ennemi doit cibler Miss Goldenweek à son prochain tour.",
                )),
                ..Default::default()
            }),
            ..CardDef::new(
                "BW-005",
                "Miss Goldenweek",
                CardType::Character,
                2,
                Faction::Pirate,
                Rarity::U,
                "ST03",
            )
        },
        // BW-006 Mr. 5 (baroque.ts:44)
        CardDef {
            atk: Some(3),
            def: Some(1),
            pv: Some(4),
            tags: tags(&["baroque"]),
            preferred_row: Some(Row::Front),
            passive: Some(PassiveDef {
                name: s("Corps Explosif"),
                description: s("Quand Mr. 5 est KO, inflige 2 dégâts à un ennemi adjacent."),
                effects: vec![PassiveEffect::ExplodeOnKo { amount: 2 }],
            }),
            base_action: Some(BaseAction {
                name: s("Coup Explosif"),
                atk: 3,
                description: Some(s("Boom.")),
                ..Default::default()
            }),
            special_attack: Some(SpecialAttack {
                name: s("Nose Fancy Cannon"),
                cost: 2,
                atk_bonus: 2,
                attack_traits: Some(vec![AttackTrait::Zone]),
                description: Some(s("Zone.")),
                ..Default::default()
            }),
            ..CardDef::new(
                "BW-006",
                "Mr. 5",
                CardType::Character,
                2,
                Faction::Pirate,
                Rarity::C,
                "ST03",
            )
        },
        // BW-007 Miss Valentine (baroque.ts:52)
        CardDef {
            atk: Some(3),
            def: Some(1),
            pv: Some(4),
            traits: Some(vec![Trait::Rush]),
            tags: tags(&["baroque", "female"]),
            preferred_row: Some(Row::Front),
            base_action: Some(BaseAction {
                name: s("Coup Léger"),
                atk: 3,
                description: Some(s("Elle plane.")),
                ..Default::default()
            }),
            special_attack: Some(SpecialAttack {
                name: s("Tonne Drop"),
                cost: 2,
                atk_bonus: 3,
                attack_traits: Some(vec![AttackTrait::Impact]),
                pushback: Some(true),
                description: Some(s("Impact.")),
                ..Default::default()
            }),
            ..CardDef::new(
                "BW-007",
                "Miss Valentine",
                CardType::Character,
                2,
                Faction::Pirate,
                Rarity::C,
                "ST03",
            )
        },
        // BW-008 Mr. 4 (baroque.ts:59)
        CardDef {
            atk: Some(5),
            def: Some(3),
            pv: Some(7),
            tags: tags(&["baroque"]),
            preferred_row: Some(Row::Front),
            passive: Some(PassiveDef {
                name: s("Quatre Tonnes"),
                description: s("Ne peut pas être repoussé ni déplacé de force."),
                effects: vec![PassiveEffect::ImmuneImpact],
            }),
            base_action: Some(BaseAction {
                name: s("Coup de Batte"),
                atk: 5,
                description: Some(s("Le batteur.")),
                ..Default::default()
            }),
            special_attack: Some(SpecialAttack {
                name: s("Home Run"),
                cost: 3,
                atk_bonus: 4,
                attack_traits: Some(vec![AttackTrait::Impact]),
                pushback: Some(true),
                description: Some(s("Impact · repousse.")),
                ..Default::default()
            }),
            ..CardDef::new(
                "BW-008",
                "Mr. 4",
                CardType::Character,
                3,
                Faction::Pirate,
                Rarity::U,
                "ST03",
            )
        },
        // BW-009 Miss Merry Christmas (baroque.ts:67)
        CardDef {
            atk: Some(3),
            def: Some(2),
            pv: Some(5),
            tags: tags(&["baroque", "female"]),
            preferred_row: Some(Row::Front),
            base_action: Some(BaseAction {
                name: s("Coup de Griffe"),
                atk: 3,
                description: Some(s("Elle creuse.")),
                ..Default::default()
            }),
            special_attack: Some(SpecialAttack {
                name: s("Mole Attack"),
                cost: 2,
                atk_bonus: 2,
                attack_traits: Some(vec![AttackTrait::Range]),
                description: Some(s("Elle surgit du sol (Portée).")),
                ..Default::default()
            }),
            ..CardDef::new(
                "BW-009",
                "Miss Merry Christmas",
                CardType::Character,
                2,
                Faction::Pirate,
                Rarity::C,
                "ST03",
            )
        },
        // BW-010 Miss All Sunday (Robin) (baroque.ts:74)
        CardDef {
            atk: Some(3),
            def: Some(2),
            pv: Some(5),
            traits: Some(vec![Trait::Range]),
            tags: tags(&["baroque", "female"]),
            preferred_row: Some(Row::Back),
            passive: Some(PassiveDef {
                name: s("Vice-Présidente"),
                description: s("À l'entrée, l'adversaire défausse une carte au hasard."),
                effects: vec![PassiveEffect::EntryDiscardRandom],
            }),
            base_action: Some(BaseAction {
                name: s("Seis Fleur"),
                atk: 3,
                description: Some(s("Des bras surgissent.")),
                ..Default::default()
            }),
            special_attack: Some(SpecialAttack {
                name: s("Spider Net"),
                cost: 2,
                atk_bonus: 2,
                immobilize: Some(true),
                description: Some(s("La cible est immobilisée 1 tour.")),
                ..Default::default()
            }),
            ..CardDef::new(
                "BW-010",
                "Miss All Sunday (Robin)",
                CardType::Character,
                3,
                Faction::Pirate,
                Rarity::R,
                "ST03",
            )
        },
        // === OBJETS - ARMES ===
        // BW-013 Crochet Empoisonné (baroque.ts:84)
        CardDef {
            subtype: Some(ObjectSubtype::Weapon),
            bonus_atk: Some(1),
            grants_element: Some(Element::Poison),
            equip_effect: Some(s(
                "+1 ATK. Attaques du porteur : élément Poison (1 dég./tour permanent).",
            )),
            ..CardDef::new(
                "BW-013",
                "Crochet Empoisonné",
                CardType::Object,
                1,
                Faction::Pirate,
                Rarity::U,
                "ST03",
            )
        },
        // BW-014 Lassoo (baroque.ts:89)
        CardDef {
            subtype: Some(ObjectSubtype::Weapon),
            bonus_atk: Some(1),
            restriction: Some(s("Mr. 4")),
            grants_element: Some(Element::Fire),
            equip_effect: Some(s(
                "+1 ATK. Attaques : Feu. Si détruite : déployez un jeton.",
            )),
            ..CardDef::new(
                "BW-014",
                "Lassoo",
                CardType::Object,
                1,
                Faction::Pirate,
                Rarity::C,
                "ST03",
            )
        },
        // === OBJETS - FRUITS DU DÉMON ===
        // BW-011 Suna Suna no Mi (baroque.ts:96)
        CardDef {
            subtype: Some(ObjectSubtype::Fruit),
            bonus_atk: Some(0),
            restriction: Some(s("Crocodile")),
            equip_effect: Some(s("Applique Logia + Maudit.")),
            fruit_effects: Some(FruitEffects {
                base: FruitBaseEffects {
                    grants_traits: Some(vec![Trait::Cursed, Trait::Logia]),
                    passive_description: Some(s(
                        "Logia. Sables : 2 Vol · ATK +3 · Sable (-1 PV permanent).",
                    )),
                    atk_bonus: None,
                    def_bonus: None,
                },
                awakening: Some(FruitAwakening {
                    porteur_legitime: s("Crocodile"),
                    min_turns: 5,
                    vol_cost: 3,
                    grants_traits: None,
                    atk_bonus: Some(5),
                    def_bonus: None,
                    passive_description: Some(s(
                        "Fin de votre tour : tous les ennemis blessés perdent 1 PV permanent.",
                    )),
                    special_attack: Some(FruitAwakeningSpecialAttack {
                        name: s("Ground Death"),
                        cost: 4,
                        atk_bonus: 5,
                        description: s(
                            "La cible perd 3 PV permanent et ne peut plus être soignée.",
                        ),
                        once_per_game: Some(true),
                        attack_traits: None,
                        element: Some(Element::Sand),
                        ignore_def: None,
                        immobilize: None,
                        sleep: None,
                        pushback: None,
                        ignore_shield: None,
                        strip_stealth: None,
                    }),
                }),
            }),
            ..CardDef::new(
                "BW-011",
                "Suna Suna no Mi",
                CardType::Object,
                3,
                Faction::Pirate,
                Rarity::Sr,
                "ST03",
            )
        },
        // BW-012 Supa Supa no Mi (baroque.ts:109)
        CardDef {
            subtype: Some(ObjectSubtype::Fruit),
            bonus_atk: Some(0),
            restriction: Some(s("Mr. 1")),
            equip_effect: Some(s("Applique Perçant + Maudit.")),
            fruit_effects: Some(FruitEffects {
                base: FruitBaseEffects {
                    grants_traits: Some(vec![Trait::Cursed, Trait::Piercing]),
                    passive_description: Some(s("Perçant. Spartan : 1 Vol · ATK +2 · Perçant.")),
                    atk_bonus: None,
                    def_bonus: None,
                },
                awakening: Some(FruitAwakening {
                    porteur_legitime: s("Mr. 1"),
                    min_turns: 5,
                    vol_cost: 2,
                    grants_traits: None,
                    atk_bonus: Some(0),
                    def_bonus: None,
                    passive_description: Some(s("Les attaques ignorent le Bouclier et la DEF.")),
                    special_attack: Some(FruitAwakeningSpecialAttack {
                        name: s("Atomic Spurt"),
                        cost: 3,
                        atk_bonus: 3,
                        description: s("Total · ignore Bouclier et DEF."),
                        once_per_game: None,
                        attack_traits: Some(vec![AttackTrait::Total]),
                        element: None,
                        ignore_def: Some(99),
                        immobilize: None,
                        sleep: None,
                        pushback: None,
                        ignore_shield: Some(true),
                        strip_stealth: None,
                    }),
                }),
            }),
            ..CardDef::new(
                "BW-012",
                "Supa Supa no Mi",
                CardType::Object,
                2,
                Faction::Pirate,
                Rarity::R,
                "ST03",
            )
        },
        // === OBJETS - ACCESSOIRES ===
        // BW-015 Den Den Mushi Secret (baroque.ts:124)
        CardDef {
            subtype: Some(ObjectSubtype::Accessory),
            bonus_atk: Some(0),
            equip_effect: Some(s(
                "À l'entrée, regardez la main adverse. Les attaques du porteur ignorent le Furtif.",
            )),
            ..CardDef::new(
                "BW-015",
                "Den Den Mushi Secret",
                CardType::Object,
                1,
                Faction::Pirate,
                Rarity::C,
                "ST03",
            )
        },
        // BW-016 Bananawani (baroque.ts:129)
        CardDef {
            subtype: Some(ObjectSubtype::Accessory),
            bonus_atk: Some(0),
            equip_effect: Some(s(
                "À l'entrée du porteur, déployez un jeton Bananawani (ATK 4 / DEF 1 / PV 4) adjacent.",
            )),
            ..CardDef::new(
                "BW-016",
                "Bananawani",
                CardType::Object,
                2,
                Faction::Pirate,
                Rarity::U,
                "ST03",
            )
        },
        // BW-017 Poudre Explosive (baroque.ts:134)
        CardDef {
            subtype: Some(ObjectSubtype::Accessory),
            bonus_atk: Some(0),
            equip_effect: Some(s("1x/partie : 3 dégâts à un ennemi (Zone).")),
            ..CardDef::new(
                "BW-017",
                "Poudre Explosive",
                CardType::Object,
                1,
                Faction::Pirate,
                Rarity::C,
                "ST03",
            )
        },
        // === NAVIRES ===
        // BW-018 Rain Dinners (baroque.ts:141)
        CardDef {
            ship_passive: Some(s("Vos Baroque Works gagnent +1 ATK.")),
            ship_active: Some(ShipActive {
                name: s("Casino"),
                cost: 1,
                description: s("Piochez 1 carte puis défaussez 1 carte."),
                once_per_game: None,
            }),
            ..CardDef::new(
                "BW-018",
                "Rain Dinners",
                CardType::Ship,
                1,
                Faction::Pirate,
                Rarity::U,
                "ST03",
            )
        },
        // BW-019 Navire Baroque Works (baroque.ts:147)
        CardDef {
            ship_passive: Some(s("Vos Baroque Works gagnent +1 PV.")),
            ship_active: Some(ShipActive {
                name: s("Agents"),
                cost: 2,
                description: s("Déployez deux jetons agents."),
                once_per_game: Some(true),
            }),
            ..CardDef::new(
                "BW-019",
                "Navire Baroque Works",
                CardType::Ship,
                2,
                Faction::Pirate,
                Rarity::U,
                "ST03",
            )
        },
        // === EVENEMENTS ===
        // BW-020 Operation Utopia (baroque.ts:155)
        CardDef {
            event_effect: Some(EventEffect::DamageEnemies {
                amount: 3,
                target: DamageTarget::All,
                cursed_bonus: None,
                sand: None,
                destroy_ships: None,
            }),
            ..CardDef::new(
                "BW-020",
                "Operation Utopia",
                CardType::Event,
                3,
                Faction::Pirate,
                Rarity::R,
                "ST03",
            )
        },
        // BW-021 Embuscade (baroque.ts:160)
        CardDef {
            event_effect: Some(EventEffect::RushBuff { atk: 2 }),
            ..CardDef::new(
                "BW-021",
                "Embuscade",
                CardType::Event,
                2,
                Faction::Pirate,
                Rarity::U,
                "ST03",
            )
        },
        // BW-022 Contrat d'Assassinat (baroque.ts:165)
        CardDef {
            event_effect: Some(EventEffect::Custom {
                id: s("execute3"),
                description: s("Détruisez un ennemi de PV actuels ≤ 3."),
            }),
            ..CardDef::new(
                "BW-022",
                "Contrat d'Assassinat",
                CardType::Event,
                1,
                Faction::Pirate,
                Rarity::C,
                "ST03",
            )
        },
        // BW-023 Tempête de Sable (baroque.ts:170)
        CardDef {
            event_effect: Some(EventEffect::DamageEnemies {
                amount: 2,
                target: DamageTarget::All,
                cursed_bonus: None,
                sand: Some(true),
                destroy_ships: None,
            }),
            ..CardDef::new(
                "BW-023",
                "Tempête de Sable",
                CardType::Event,
                3,
                Faction::Pirate,
                Rarity::U,
                "ST03",
            )
        },
        // BW-024 Trahison (baroque.ts:175)
        CardDef {
            event_effect: Some(EventEffect::Custom {
                id: s("betrayal"),
                description: s("Prenez le contrôle d'un ennemi de coût ≤ 2 ce tour."),
            }),
            ..CardDef::new(
                "BW-024",
                "Trahison",
                CardType::Event,
                2,
                Faction::Pirate,
                Rarity::U,
                "ST03",
            )
        },
        // BW-027 Infiltration (baroque.ts:180)
        CardDef {
            event_effect: Some(EventEffect::Tutor {
                filter_tag: Some(s("baroque")),
                max_cost: Some(3),
            }),
            ..CardDef::new(
                "BW-027",
                "Infiltration",
                CardType::Event,
                1,
                Faction::Pirate,
                Rarity::C,
                "ST03",
            )
        },
        // BW-028 Pluie Artificielle (baroque.ts:185)
        CardDef {
            event_effect: Some(EventEffect::Custom {
                id: s("noHeal2"),
                description: s("Persistant (2 tours) : les ennemis ne peuvent pas être soignés."),
            }),
            ..CardDef::new(
                "BW-028",
                "Pluie Artificielle",
                CardType::Event,
                2,
                Faction::Pirate,
                Rarity::U,
                "ST03",
            )
        },
        // === COUNTERS ===
        // BW-025 « Faible » (baroque.ts:192)
        CardDef {
            counter_effect: Some(CounterEffect::Cancel {
                description: s("Annulez une attaque d'un ennemi d'ATK ≤ 4."),
                max_attacker_atk: Some(4),
                self_captain_damage: None,
                once: None,
            }),
            ..CardDef::new(
                "BW-025",
                "« Faible »",
                CardType::Counter,
                0,
                Faction::Pirate,
                Rarity::C,
                "ST03",
            )
        },
        // BW-026 Mirage du Désert (baroque.ts:197)
        CardDef {
            counter_effect: Some(CounterEffect::Untargetable {
                description: s("La cible devient Inciblable jusqu'à la fin du tour."),
            }),
            ..CardDef::new(
                "BW-026",
                "Mirage du Désert",
                CardType::Counter,
                1,
                Faction::Pirate,
                Rarity::C,
                "ST03",
            )
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// The exact id sequence of `baroqueCards`, in TS source order. The order is
    /// load-bearing (registry insertion order) and BW-013/BW-014 deliberately
    /// come *before* BW-011/BW-012 in the TS file.
    const TS_ORDER: [&str; 28] = [
        "BW-001", "BW-002", "BW-003", "BW-004", "BW-005", "BW-006", "BW-007", "BW-008", "BW-009",
        "BW-010", "BW-013", "BW-014", "BW-011", "BW-012", "BW-015", "BW-016", "BW-017", "BW-018",
        "BW-019", "BW-020", "BW-021", "BW-022", "BW-023", "BW-024", "BW-027", "BW-028", "BW-025",
        "BW-026",
    ];

    #[test]
    fn card_count_and_order_match_ts() {
        let all = cards();
        assert_eq!(all.len(), 28, "baroqueCards has 28 entries");
        let ids: Vec<&str> = all.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, TS_ORDER.to_vec());
    }

    #[test]
    fn every_id_is_unique() {
        let all = cards();
        let uniq: BTreeSet<&str> = all.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(uniq.len(), all.len());
    }

    /// Every card is ST03 / pirate, and the type split is 10 characters,
    /// 7 objects, 2 ships, 7 events, 2 counters.
    #[test]
    fn set_faction_and_type_distribution() {
        let all = cards();
        for c in &all {
            assert_eq!(c.set, "ST03", "{} set", c.id);
            assert_eq!(c.faction, Faction::Pirate, "{} faction", c.id);
            assert_eq!(c.is_token, None, "{} is not a token", c.id);
        }
        let count = |t: CardType| all.iter().filter(|c| c.card_type == t).count();
        assert_eq!(count(CardType::Character), 10);
        assert_eq!(count(CardType::Object), 7);
        assert_eq!(count(CardType::Ship), 2);
        assert_eq!(count(CardType::Event), 7);
        assert_eq!(count(CardType::Counter), 2);
    }

    /// `baroqueDeck` (decks.ts) only references ids defined in this set.
    #[test]
    fn baroque_deck_references_resolve() {
        let all = cards();
        let ids: BTreeSet<&str> = all.iter().map(|c| c.id.as_str()).collect();
        let deck = crate::decks::baroque_deck();
        for entry in &deck.cards {
            assert!(
                ids.contains(entry.card_id.as_str()),
                "deck references unknown card {}",
                entry.card_id
            );
        }
        // Every non-token card of the set is actually played by the deck.
        let referenced: BTreeSet<&str> = deck.cards.iter().map(|e| e.card_id.as_str()).collect();
        assert_eq!(referenced, ids);
    }

    /// Suna Suna no Mi (BW-011) — the deepest nesting in the file: Logia+Cursed
    /// base, Crocodile awakening (5 turns / 3 Vol / +5 ATK) and a once-per-game
    /// Sand special with no `attackTraits`.
    #[test]
    fn suna_suna_awakening_is_literal() {
        let all = cards();
        let f = all.iter().find(|c| c.id == "BW-011").unwrap();
        assert_eq!(f.subtype, Some(ObjectSubtype::Fruit));
        assert_eq!(f.bonus_atk, Some(0));
        assert_eq!(f.restriction.as_deref(), Some("Crocodile"));
        assert_eq!(f.rarity, Rarity::Sr);
        let fe = f.fruit_effects.as_ref().unwrap();
        assert_eq!(
            fe.base.grants_traits.as_deref(),
            Some(&[Trait::Cursed, Trait::Logia][..])
        );
        assert_eq!(fe.base.atk_bonus, None);
        assert_eq!(fe.base.def_bonus, None);
        let aw = fe.awakening.as_ref().unwrap();
        assert_eq!(aw.porteur_legitime, "Crocodile");
        assert_eq!(aw.min_turns, 5);
        assert_eq!(aw.vol_cost, 3);
        assert_eq!(aw.atk_bonus, Some(5));
        assert_eq!(aw.grants_traits, None);
        let sa = aw.special_attack.as_ref().unwrap();
        assert_eq!(sa.name, "Ground Death");
        assert_eq!((sa.cost, sa.atk_bonus), (4, 5));
        assert_eq!(sa.once_per_game, Some(true));
        assert_eq!(sa.element, Some(Element::Sand));
        assert_eq!(sa.attack_traits, None);
        assert_eq!(sa.ignore_def, None);
        assert_eq!(
            sa.description,
            "La cible perd 3 PV permanent et ne peut plus être soignée."
        );
    }

    /// Supa Supa no Mi (BW-012) — `atkBonus: 0` is *present* on the awakening
    /// (so `Some(0)`, not `None`), and the special is Total + ignoreShield +
    /// ignoreDef 99 with no `oncePerGame`.
    #[test]
    fn supa_supa_awakening_is_literal() {
        let all = cards();
        let f = all.iter().find(|c| c.id == "BW-012").unwrap();
        assert_eq!(f.restriction.as_deref(), Some("Mr. 1"));
        let fe = f.fruit_effects.as_ref().unwrap();
        assert_eq!(
            fe.base.grants_traits.as_deref(),
            Some(&[Trait::Cursed, Trait::Piercing][..])
        );
        let aw = fe.awakening.as_ref().unwrap();
        assert_eq!(aw.porteur_legitime, "Mr. 1");
        assert_eq!((aw.min_turns, aw.vol_cost), (5, 2));
        assert_eq!(aw.atk_bonus, Some(0));
        let sa = aw.special_attack.as_ref().unwrap();
        assert_eq!((sa.cost, sa.atk_bonus), (3, 3));
        assert_eq!(sa.once_per_game, None);
        assert_eq!(sa.attack_traits.as_deref(), Some(&[AttackTrait::Total][..]));
        assert_eq!(sa.ignore_shield, Some(true));
        assert_eq!(sa.ignore_def, Some(99));
        assert_eq!(sa.element, None);
    }

    /// Miss Goldenweek (BW-005) is the only card whose *base* action is a
    /// support with `immobilize`, and whose special is a 1-Vol `isSupport`
    /// with `atkBonus: 0` (so it is never an attack).
    #[test]
    fn goldenweek_support_actions() {
        let all = cards();
        let g = all.iter().find(|c| c.id == "BW-005").unwrap();
        assert_eq!(g.preferred_row, Some(Row::Back));
        assert_eq!(
            g.passive.as_ref().unwrap().effects,
            vec![PassiveEffect::DebuffOneEnemy { amount: 2 }]
        );
        let ba = g.base_action.as_ref().unwrap();
        assert_eq!(ba.atk, 0);
        assert_eq!(ba.is_support, Some(true));
        assert_eq!(ba.immobilize, Some(true));
        assert_eq!(
            ba.description.as_deref(),
            Some("Un ennemi perd sa prochaine action (figé de rire).")
        );
        let sp = g.special_attack.as_ref().unwrap();
        assert_eq!((sp.cost, sp.atk_bonus), (1, 0));
        assert_eq!(sp.is_support, Some(true));
        assert_eq!(sp.immobilize, None);
    }

    /// The passive roster, in card order — six cards carry exactly one effect.
    #[test]
    fn passive_effects_roster() {
        let all = cards();
        let got: Vec<(&str, &PassiveEffect)> = all
            .iter()
            .filter_map(|c| c.passive.as_ref().map(|p| (c.id.as_str(), &p.effects[0])))
            .collect();
        assert_eq!(
            got,
            vec![
                ("BW-002", &PassiveEffect::MeleeRecoil { amount: 2 }),
                ("BW-003", &PassiveEffect::CopyAtkOnDeploy),
                ("BW-005", &PassiveEffect::DebuffOneEnemy { amount: 2 }),
                ("BW-006", &PassiveEffect::ExplodeOnKo { amount: 2 }),
                ("BW-008", &PassiveEffect::ImmuneImpact),
                ("BW-010", &PassiveEffect::EntryDiscardRandom),
            ]
        );
        for c in all.iter().filter(|c| c.passive.is_some()) {
            assert_eq!(c.passive.as_ref().unwrap().effects.len(), 1, "{}", c.id);
        }
    }

    /// Every event effect, in card order — the two `damageEnemies` differ only
    /// by `sand`, and the three `custom` ids are `execute3` / `betrayal` /
    /// `noHeal2`.
    #[test]
    fn event_effects_are_literal() {
        let all = cards();
        let got: Vec<(&str, &EventEffect)> = all
            .iter()
            .filter_map(|c| c.event_effect.as_ref().map(|e| (c.id.as_str(), e)))
            .collect();
        assert_eq!(
            got,
            vec![
                (
                    "BW-020",
                    &EventEffect::DamageEnemies {
                        amount: 3,
                        target: DamageTarget::All,
                        cursed_bonus: None,
                        sand: None,
                        destroy_ships: None,
                    }
                ),
                ("BW-021", &EventEffect::RushBuff { atk: 2 }),
                (
                    "BW-022",
                    &EventEffect::Custom {
                        id: s("execute3"),
                        description: s("Détruisez un ennemi de PV actuels ≤ 3."),
                    }
                ),
                (
                    "BW-023",
                    &EventEffect::DamageEnemies {
                        amount: 2,
                        target: DamageTarget::All,
                        cursed_bonus: None,
                        sand: Some(true),
                        destroy_ships: None,
                    }
                ),
                (
                    "BW-024",
                    &EventEffect::Custom {
                        id: s("betrayal"),
                        description: s("Prenez le contrôle d'un ennemi de coût ≤ 2 ce tour."),
                    }
                ),
                (
                    "BW-027",
                    &EventEffect::Tutor {
                        filter_tag: Some(s("baroque")),
                        max_cost: Some(3),
                    }
                ),
                (
                    "BW-028",
                    &EventEffect::Custom {
                        id: s("noHeal2"),
                        description: s(
                            "Persistant (2 tours) : les ennemis ne peuvent pas être soignés.",
                        ),
                    }
                ),
            ]
        );
    }

    /// Both counters, including the guillemet-wrapped name `« Faible »`
    /// (ASCII spaces inside the guillemets) and its `maxAttackerAtk: 4` gate.
    #[test]
    fn counters_are_literal() {
        let all = cards();
        let faible = all.iter().find(|c| c.id == "BW-025").unwrap();
        assert_eq!(faible.name, "\u{ab} Faible \u{bb}");
        assert_eq!(faible.cost, 0);
        assert_eq!(
            faible.counter_effect,
            Some(CounterEffect::Cancel {
                description: s("Annulez une attaque d'un ennemi d'ATK ≤ 4."),
                max_attacker_atk: Some(4),
                self_captain_damage: None,
                once: None,
            })
        );
        let mirage = all.iter().find(|c| c.id == "BW-026").unwrap();
        assert_eq!(
            mirage.counter_effect,
            Some(CounterEffect::Untargetable {
                description: s("La cible devient Inciblable jusqu'à la fin du tour."),
            })
        );
    }

    /// Both ships: only BW-019's active is once-per-game.
    #[test]
    fn ships_are_literal() {
        let all = cards();
        let rain = all.iter().find(|c| c.id == "BW-018").unwrap();
        assert_eq!(
            rain.ship_passive.as_deref(),
            Some("Vos Baroque Works gagnent +1 ATK.")
        );
        assert_eq!(
            rain.ship_active,
            Some(ShipActive {
                name: s("Casino"),
                cost: 1,
                description: s("Piochez 1 carte puis défaussez 1 carte."),
                once_per_game: None,
            })
        );
        assert_eq!(rain.ship_destroy_effect, None);
        let bw = all.iter().find(|c| c.id == "BW-019").unwrap();
        assert_eq!(
            bw.ship_passive.as_deref(),
            Some("Vos Baroque Works gagnent +1 PV.")
        );
        assert_eq!(
            bw.ship_active,
            Some(ShipActive {
                name: s("Agents"),
                cost: 2,
                description: s("Déployez deux jetons agents."),
                once_per_game: Some(true),
            })
        );
    }

    /// The two weapons carry `grantsElement` (Poison / Fire); Lassoo is the
    /// only weapon with a `restriction`. Accessories are pure text effects.
    #[test]
    fn objects_weapons_and_accessories() {
        let all = cards();
        let hook = all.iter().find(|c| c.id == "BW-013").unwrap();
        assert_eq!(hook.subtype, Some(ObjectSubtype::Weapon));
        assert_eq!(hook.bonus_atk, Some(1));
        assert_eq!(hook.grants_element, Some(Element::Poison));
        assert_eq!(hook.restriction, None);
        assert_eq!(hook.grants_traits, None);
        let lassoo = all.iter().find(|c| c.id == "BW-014").unwrap();
        assert_eq!(lassoo.subtype, Some(ObjectSubtype::Weapon));
        assert_eq!(lassoo.bonus_atk, Some(1));
        assert_eq!(lassoo.grants_element, Some(Element::Fire));
        assert_eq!(lassoo.restriction.as_deref(), Some("Mr. 4"));
        for id in ["BW-015", "BW-016", "BW-017"] {
            let a = all.iter().find(|c| c.id == id).unwrap();
            assert_eq!(a.subtype, Some(ObjectSubtype::Accessory), "{id}");
            assert_eq!(a.bonus_atk, Some(0), "{id}");
            assert_eq!(a.grants_element, None, "{id}");
            assert!(a.equip_effect.is_some(), "{id}");
            assert_eq!(a.fruit_effects, None, "{id}");
        }
    }

    /// Character stat block, in card order: (id, cost, atk, def, pv).
    #[test]
    fn character_stat_blocks() {
        let all = cards();
        let got: Vec<(&str, i32, i32, i32, i32)> = all
            .iter()
            .filter(|c| c.card_type == CardType::Character)
            .map(|c| {
                (
                    c.id.as_str(),
                    c.cost,
                    c.atk.unwrap(),
                    c.def.unwrap(),
                    c.pv.unwrap(),
                )
            })
            .collect();
        assert_eq!(
            got,
            vec![
                ("BW-001", 4, 6, 4, 7),
                ("BW-002", 3, 4, 2, 5),
                ("BW-003", 3, 4, 2, 6),
                ("BW-004", 2, 2, 3, 5),
                ("BW-005", 2, 1, 1, 4),
                ("BW-006", 2, 3, 1, 4),
                ("BW-007", 2, 3, 1, 4),
                ("BW-008", 3, 5, 3, 7),
                ("BW-009", 2, 3, 2, 5),
                ("BW-010", 3, 3, 2, 5),
            ]
        );
        // Every character is tagged "baroque"; the five women also carry "female".
        let female: Vec<&str> = all
            .iter()
            .filter(|c| c.has_tag("female"))
            .map(|c| c.id.as_str())
            .collect();
        assert_eq!(
            female,
            vec!["BW-002", "BW-005", "BW-007", "BW-009", "BW-010"]
        );
        for c in all.iter().filter(|c| c.card_type == CardType::Character) {
            assert!(c.has_tag("baroque"), "{}", c.id);
            assert!(c.base_action.is_some(), "{}", c.id);
            assert!(c.special_attack.is_some(), "{}", c.id);
            assert_eq!(c.natural_haki, None, "{}", c.id);
            assert_eq!(c.synergies, None, "{}", c.id);
        }
    }

    /// Innate `traits` are only on four characters, and `preferredRow` is
    /// `back` only for Goldenweek and Robin.
    #[test]
    fn traits_and_rows() {
        let all = cards();
        let with_traits: Vec<(&str, &[Trait])> = all
            .iter()
            .filter_map(|c| c.traits.as_deref().map(|t| (c.id.as_str(), t)))
            .collect();
        assert_eq!(
            with_traits,
            vec![
                ("BW-001", &[Trait::Piercing][..]),
                ("BW-004", &[Trait::Shield][..]),
                ("BW-007", &[Trait::Rush][..]),
                ("BW-010", &[Trait::Range][..]),
            ]
        );
        let back: Vec<&str> = all
            .iter()
            .filter(|c| c.preferred_row == Some(Row::Back))
            .map(|c| c.id.as_str())
            .collect();
        assert_eq!(back, vec!["BW-005", "BW-010"]);
    }
}
