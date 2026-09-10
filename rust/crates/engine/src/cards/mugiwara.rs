//! ST01 — Mugiwara pré-ellipse card data.
//!
//! Port of `src/data/cards/mugiwara.ts` (`mugiwaraCards`) — 28 card
//! definitions, in the exact source order (the order is load-bearing: the
//! registry is filled by iterating it, and later ids overwrite earlier ones).
//!
//! Array order is MG-001…MG-025, **MG-028**, MG-026, MG-027 — exactly like the
//! TypeScript literal, where `Coup de Burst` sits in the "EVENEMENTS" block
//! before the two counters.

use crate::types::{
    AtkDefStat, AttackTrait, BaseAction, BuffStat, CardDef, CardType, CounterEffect, Element,
    EventEffect, Faction, FruitAwakening, FruitAwakeningSpecialAttack, FruitBaseEffects,
    FruitEffects, ObjectSubtype, PassiveDef, PassiveEffect, Rarity, Row, ShipActive,
    ShipDestroyEffect, SpecialAttack, SynergyDef, Trait, Transform,
};

/// `["mugiwara", "bretteur"]` → `Some(vec!["mugiwara".into(), …])`.
fn tags(list: &[&str]) -> Option<Vec<String>> {
    Some(list.iter().map(|s| (*s).to_string()).collect())
}

/// TS `mugiwaraCards: CardDef[]` (`src/data/cards/mugiwara.ts`).
///
/// Returns the 28 definitions of the ST01 — Mugiwara pré-ellipse set in source order.
pub fn cards() -> Vec<CardDef> {
    vec![
        // === PERSONNAGES ===
        // mugiwara.ts:6 — MG-001 Roronoa Zoro.
        // Pas de passif — la Rivalité est une synergie.
        CardDef {
            atk: Some(7),
            def: Some(3),
            pv: Some(10),
            tags: tags(&["mugiwara", "bretteur"]),
            preferred_row: Some(Row::Front),
            synergies: Some(vec![SynergyDef {
                partner_id: "MG-002".to_string(),
                atk_bonus: 2,
                on_partner_ko: None,
            }]),
            base_action: Some(BaseAction {
                name: "Coup de Sabre".to_string(),
                atk: 7,
                description: Some("Trois sabres, une seule volonté.".to_string()),
                ..Default::default()
            }),
            special_attack: Some(SpecialAttack {
                name: "Oni Giri".to_string(),
                cost: 3,
                atk_bonus: 3,
                attack_traits: Some(vec![AttackTrait::Piercing]),
                description: Some("La coupe du démon. Perçant.".to_string()),
                ..Default::default()
            }),
            ..CardDef::new(
                "MG-001",
                "Roronoa Zoro",
                CardType::Character,
                5,
                Faction::Pirate,
                Rarity::R,
                "ST01",
            )
        },
        // mugiwara.ts:17 — MG-002 Sanji.
        CardDef {
            atk: Some(6),
            def: Some(3),
            pv: Some(7),
            tags: tags(&["mugiwara", "cuisinier"]),
            preferred_row: Some(Row::Front),
            passive: Some(PassiveDef {
                name: "Galanterie".to_string(),
                description: "NE PEUT PAS cibler les personnages féminins.".to_string(),
                effects: vec![PassiveEffect::CannotAttackFemale],
            }),
            synergies: Some(vec![SynergyDef {
                partner_id: "MG-001".to_string(),
                atk_bonus: 2,
                on_partner_ko: None,
            }]),
            base_action: Some(BaseAction {
                name: "Coup de Pied".to_string(),
                atk: 6,
                description: Some("La Jambe Noire frappe.".to_string()),
                ..Default::default()
            }),
            special_attack: Some(SpecialAttack {
                name: "Diable Jambe".to_string(),
                cost: 3,
                atk_bonus: 3,
                element: Some(Element::Fire),
                description: Some("La jambe en feu.".to_string()),
                ..Default::default()
            }),
            ..CardDef::new(
                "MG-002",
                "Sanji",
                CardType::Character,
                4,
                Faction::Pirate,
                Rarity::R,
                "ST01",
            )
        },
        // mugiwara.ts:32 — MG-003 Nami.
        CardDef {
            atk: Some(1),
            def: Some(1),
            pv: Some(4),
            traits: Some(vec![Trait::Range]),
            tags: tags(&["mugiwara", "navigateur", "female"]),
            preferred_row: Some(Row::Back),
            base_action: Some(BaseAction {
                name: "Prévisions".to_string(),
                atk: 0,
                is_support: Some(true),
                scry: Some(2),
                description: Some(
                    "Regarde les 2 cartes du dessus de ton deck, remets-les dans l'ordre voulu."
                        .to_string(),
                ),
                ..Default::default()
            }),
            special_attack: Some(SpecialAttack {
                name: "Thunder Tempo".to_string(),
                cost: 2,
                atk_bonus: 3,
                element: Some(Element::Thunder),
                description: Some("La foudre frappe.".to_string()),
                ..Default::default()
            }),
            ..CardDef::new(
                "MG-003",
                "Nami",
                CardType::Character,
                2,
                Faction::Pirate,
                Rarity::C,
                "ST01",
            )
        },
        // mugiwara.ts:41 — MG-004 Usopp.
        CardDef {
            atk: Some(1),
            def: Some(1),
            pv: Some(4),
            traits: Some(vec![Trait::Range]),
            tags: tags(&["mugiwara", "tireur"]),
            preferred_row: Some(Row::Back),
            passive: Some(PassiveDef {
                name: "Vantardise".to_string(),
                description: "Début de votre tour : un de vos alliés gagne +1 ATK ce tour."
                    .to_string(),
                effects: vec![PassiveEffect::StartTurnBuffAlly {
                    stat: AtkDefStat::Atk,
                    amount: 1,
                }],
            }),
            base_action: Some(BaseAction {
                name: "Bluff".to_string(),
                atk: 0,
                is_support: Some(true),
                bluff: Some(true),
                description: Some(
                    "Un ennemi de DEF ≤ 1 perd sa prochaine action (il a peur).".to_string(),
                ),
                ..Default::default()
            }),
            special_attack: Some(SpecialAttack {
                name: "Kayaku Boshi".to_string(),
                cost: 2,
                atk_bonus: 3,
                element: Some(Element::Fire),
                description: Some("L'étoile explosive.".to_string()),
                ..Default::default()
            }),
            ..CardDef::new(
                "MG-004",
                "Usopp",
                CardType::Character,
                2,
                Faction::Pirate,
                Rarity::C,
                "ST01",
            )
        },
        // mugiwara.ts:55 — MG-005 Tony Tony Chopper.
        CardDef {
            atk: Some(1),
            def: Some(2),
            pv: Some(6),
            tags: tags(&["mugiwara", "medecin"]),
            preferred_row: Some(Row::Back),
            passive: Some(PassiveDef {
                name: "Médecin de bord".to_string(),
                description: "Début de votre tour : soigne 1 PV à un allié adjacent.".to_string(),
                effects: vec![PassiveEffect::HealAdjacent { amount: 1 }],
            }),
            base_action: Some(BaseAction {
                name: "Point de Suture".to_string(),
                atk: 0,
                is_support: Some(true),
                heal_amount: Some(2),
                description: Some("Soigne 2 PV à un allié (n'importe quel slot).".to_string()),
                ..Default::default()
            }),
            special_attack: Some(SpecialAttack {
                name: "Monster Point".to_string(),
                cost: 3,
                atk_bonus: 0,
                once_per_game: Some(true),
                is_support: Some(true),
                transform: Some(Transform {
                    atk: 8,
                    def: 2,
                    pv: 6,
                    turns: 2,
                }),
                description: Some(
                    "ATK 8 · DEF 2 · PV 6 + Rush pendant 2 tours, puis KO automatique.".to_string(),
                ),
                ..Default::default()
            }),
            ..CardDef::new(
                "MG-005",
                "Tony Tony Chopper",
                CardType::Character,
                2,
                Faction::Pirate,
                Rarity::C,
                "ST01",
            )
        },
        // mugiwara.ts:69 — MG-006 Nico Robin.
        CardDef {
            atk: Some(3),
            def: Some(2),
            pv: Some(5),
            traits: Some(vec![Trait::Range]),
            tags: tags(&["mugiwara", "female", "archeologue"]),
            preferred_row: Some(Row::Back),
            passive: Some(PassiveDef {
                name: "Hana Hana".to_string(),
                description: "Les attaques de Robin ne peuvent pas être esquivées.".to_string(),
                effects: vec![PassiveEffect::NoDodge],
            }),
            base_action: Some(BaseAction {
                name: "Seis Fleur".to_string(),
                atk: 3,
                description: Some("Des bras surgissent de partout.".to_string()),
                ..Default::default()
            }),
            special_attack: Some(SpecialAttack {
                name: "Clutch".to_string(),
                cost: 2,
                atk_bonus: 2,
                immobilize: Some(true),
                description: Some("La cible est immobilisée 1 tour.".to_string()),
                ..Default::default()
            }),
            ..CardDef::new(
                "MG-006",
                "Nico Robin",
                CardType::Character,
                3,
                Faction::Pirate,
                Rarity::R,
                "ST01",
            )
        },
        // mugiwara.ts:83 — MG-007 Franky.
        CardDef {
            atk: Some(5),
            def: Some(4),
            pv: Some(8),
            traits: Some(vec![Trait::Shield]),
            tags: tags(&["mugiwara", "charpentier"]),
            preferred_row: Some(Row::Front),
            base_action: Some(BaseAction {
                name: "Strong Right".to_string(),
                atk: 5,
                description: Some("Le poing cyborg.".to_string()),
                ..Default::default()
            }),
            special_attack: Some(SpecialAttack {
                name: "Coup de Vent".to_string(),
                cost: 3,
                atk_bonus: 4,
                element: Some(Element::Fire),
                attack_traits: Some(vec![AttackTrait::Range]),
                description: Some("Tir d'air comprimé à distance.".to_string()),
                ..Default::default()
            }),
            ..CardDef::new(
                "MG-007",
                "Franky",
                CardType::Character,
                3,
                Faction::Pirate,
                Rarity::U,
                "ST01",
            )
        },
        // mugiwara.ts:92 — MG-008 Brook.
        CardDef {
            atk: Some(5),
            def: Some(2),
            pv: Some(6),
            tags: tags(&["mugiwara", "musicien", "bretteur"]),
            preferred_row: Some(Row::Front),
            passive: Some(PassiveDef {
                name: "Le Barde".to_string(),
                description: "Tant que Brook est en jeu, vos alliés gagnent +1 DEF.".to_string(),
                effects: vec![PassiveEffect::BuffAlly {
                    stat: BuffStat::Def,
                    amount: 1,
                    filter: None,
                }],
            }),
            base_action: Some(BaseAction {
                name: "Mélodie de l'Âme".to_string(),
                atk: 0,
                is_support: Some(true),
                buff_ally_atk: Some(2),
                description: Some("Un allié gagne +2 ATK ce tour.".to_string()),
                ..Default::default()
            }),
            special_attack: Some(SpecialAttack {
                name: "Hanauta Sancho: Yahazu Giri".to_string(),
                cost: 3,
                atk_bonus: 3,
                element: Some(Element::Ice),
                description: Some("Un coup glacial.".to_string()),
                ..Default::default()
            }),
            ..CardDef::new(
                "MG-008",
                "Brook",
                CardType::Character,
                3,
                Faction::Pirate,
                Rarity::U,
                "ST01",
            )
        },
        // === OBJETS - ARMES ===
        // mugiwara.ts:108 — MG-009 Wado Ichimonji.
        CardDef {
            subtype: Some(ObjectSubtype::Weapon),
            bonus_atk: Some(1),
            restriction: Some("bretteur".to_string()),
            equip_effect: Some("+1 ATK. Si équipée par Zoro : +1 DEF de plus.".to_string()),
            ..CardDef::new(
                "MG-009",
                "Wado Ichimonji",
                CardType::Object,
                1,
                Faction::Pirate,
                Rarity::C,
                "ST01",
            )
        },
        // mugiwara.ts:114 — MG-010 Sandai Kitetsu.
        CardDef {
            subtype: Some(ObjectSubtype::Weapon),
            bonus_atk: Some(2),
            restriction: Some("bretteur".to_string()),
            equip_effect: Some(
                "+2 ATK. Malédiction : au début de votre tour, le porteur subit 1 dégât."
                    .to_string(),
            ),
            ..CardDef::new(
                "MG-010",
                "Sandai Kitetsu",
                CardType::Object,
                1,
                Faction::Pirate,
                Rarity::C,
                "ST01",
            )
        },
        // mugiwara.ts:120 — MG-011 Yubashiri.
        CardDef {
            subtype: Some(ObjectSubtype::Weapon),
            bonus_atk: Some(1),
            restriction: Some("bretteur".to_string()),
            equip_effect: Some(
                "+1 ATK. Si détruite : le porteur gagne +1 ATK permanent.".to_string(),
            ),
            ..CardDef::new(
                "MG-011",
                "Yubashiri",
                CardType::Object,
                1,
                Faction::Pirate,
                Rarity::C,
                "ST01",
            )
        },
        // mugiwara.ts:126 — MG-012 Clima-Tact.
        CardDef {
            subtype: Some(ObjectSubtype::Weapon),
            bonus_atk: Some(1),
            restriction: Some("Nami".to_string()),
            grants_element: Some(Element::Thunder),
            equip_effect: Some(
                "+1 ATK. Attaques du porteur : élément Foudre. Combo (Usopp + Nami en jeu) : coûte 0."
                    .to_string(),
            ),
            ..CardDef::new(
                "MG-012",
                "Clima-Tact",
                CardType::Object,
                1,
                Faction::Pirate,
                Rarity::C,
                "ST01",
            )
        },
        // mugiwara.ts:132 — MG-013 Kabuto.
        CardDef {
            subtype: Some(ObjectSubtype::Weapon),
            bonus_atk: Some(1),
            restriction: Some("tireur".to_string()),
            equip_effect: Some(
                "+1 ATK. Les attaques du porteur sont inesquivables (ignorent l'esquive et l'Observation)."
                    .to_string(),
            ),
            ..CardDef::new(
                "MG-013",
                "Kabuto",
                CardType::Object,
                1,
                Faction::Pirate,
                Rarity::C,
                "ST01",
            )
        },
        // === OBJETS - FRUITS DU DÉMON ===
        // mugiwara.ts:140 — MG-014 Gomu Gomu no Mi.
        CardDef {
            subtype: Some(ObjectSubtype::Fruit),
            bonus_atk: Some(0),
            restriction: Some("Luffy".to_string()),
            equip_effect: Some("Maudit. Immunité Impact. Portée sur les attaques.".to_string()),
            fruit_effects: Some(FruitEffects {
                base: FruitBaseEffects {
                    grants_traits: Some(vec![Trait::Cursed]),
                    passive_description: Some(
                        "Immunité Impact. Gomu Gomu no Pistol : 1 Vol · ATK +3 · Impact."
                            .to_string(),
                    ),
                    atk_bonus: None,
                    def_bonus: None,
                },
                awakening: Some(FruitAwakening {
                    porteur_legitime: "Luffy".to_string(),
                    min_turns: 5,
                    vol_cost: 3,
                    grants_traits: Some(vec![Trait::Rush]),
                    atk_bonus: Some(3),
                    def_bonus: None,
                    passive_description: Some(
                        "Gear 4 Boundman — +3 ATK, Rush, Impact sur toutes les attaques."
                            .to_string(),
                    ),
                    special_attack: Some(FruitAwakeningSpecialAttack {
                        name: "Kong Gun".to_string(),
                        cost: 4,
                        atk_bonus: 6,
                        description: "Impact · Zone · ignore le Bouclier.".to_string(),
                        once_per_game: Some(true),
                        attack_traits: None,
                        element: None,
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
                "MG-014",
                "Gomu Gomu no Mi",
                CardType::Object,
                3,
                Faction::Pirate,
                Rarity::Sr,
                "ST01",
            )
        },
        // mugiwara.ts:161 — MG-015 Hana Hana no Mi.
        CardDef {
            subtype: Some(ObjectSubtype::Fruit),
            bonus_atk: Some(0),
            restriction: Some("Robin".to_string()),
            equip_effect: Some("Maudit. Le porteur gagne Portée.".to_string()),
            fruit_effects: Some(FruitEffects {
                base: FruitBaseEffects {
                    grants_traits: Some(vec![Trait::Cursed]),
                    passive_description: Some(
                        "Le porteur gagne Portée. Doce Fleur : 1 Vol · ATK +2 · repositionne la cible."
                            .to_string(),
                    ),
                    atk_bonus: None,
                    def_bonus: None,
                },
                awakening: Some(FruitAwakening {
                    porteur_legitime: "Robin".to_string(),
                    min_turns: 5,
                    vol_cost: 2,
                    grants_traits: None,
                    atk_bonus: Some(0),
                    def_bonus: None,
                    passive_description: Some(
                        "Mil Fleur — Portée ; un ennemi Avant ne peut pas utiliser Bouclier à votre tour."
                            .to_string(),
                    ),
                    special_attack: Some(FruitAwakeningSpecialAttack {
                        name: "Gigante Fleur".to_string(),
                        cost: 3,
                        atk_bonus: 0,
                        description:
                            "Immobilise toute la Ligne Avant adverse 1 tour + 2 dégâts à chacun."
                                .to_string(),
                        once_per_game: None,
                        attack_traits: None,
                        element: None,
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
                "MG-015",
                "Hana Hana no Mi",
                CardType::Object,
                2,
                Faction::Pirate,
                Rarity::R,
                "ST01",
            )
        },
        // mugiwara.ts:181 — MG-016 Yomi Yomi no Mi.
        CardDef {
            subtype: Some(ObjectSubtype::Fruit),
            bonus_atk: Some(0),
            restriction: Some("Brook".to_string()),
            equip_effect: Some("Maudit. Résurrection 1x/partie (revient 3 PV).".to_string()),
            fruit_effects: Some(FruitEffects {
                base: FruitBaseEffects {
                    grants_traits: Some(vec![Trait::Cursed]),
                    passive_description: Some(
                        "Revenant : si KO, revient (même slot) avec 3 PV — 1x/partie. Aubade Coup Droit : 1 Vol · ATK +2 · Glace."
                            .to_string(),
                    ),
                    atk_bonus: None,
                    def_bonus: None,
                },
                awakening: Some(FruitAwakening {
                    porteur_legitime: "Brook".to_string(),
                    min_turns: 5,
                    vol_cost: 2,
                    grants_traits: None,
                    atk_bonus: Some(0),
                    def_bonus: None,
                    passive_description: Some(
                        "Soul King — Revenant + les ennemis adjacents au porteur −1 ATK."
                            .to_string(),
                    ),
                    special_attack: Some(FruitAwakeningSpecialAttack {
                        name: "Nemuriuta Flanc".to_string(),
                        cost: 3,
                        atk_bonus: 0,
                        description: "Un ennemi est endormi (perd ses 2 prochaines actions)."
                            .to_string(),
                        once_per_game: None,
                        attack_traits: None,
                        element: None,
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
                "MG-016",
                "Yomi Yomi no Mi",
                CardType::Object,
                2,
                Faction::Pirate,
                Rarity::R,
                "ST01",
            )
        },
        // === OBJETS - ACCESSOIRES ===
        // mugiwara.ts:203 — MG-017 Baril d'Eau.
        CardDef {
            subtype: Some(ObjectSubtype::Accessory),
            bonus_atk: Some(0),
            grants_element: Some(Element::Water),
            equip_effect: Some(
                "Attaques du porteur : élément Eau (x2 vs Maudits ; touche les Logia).".to_string(),
            ),
            ..CardDef::new(
                "MG-017",
                "Baril d'Eau",
                CardType::Object,
                1,
                Faction::Pirate,
                Rarity::C,
                "ST01",
            )
        },
        // mugiwara.ts:209 — MG-018 Dial d'Impact.
        CardDef {
            subtype: Some(ObjectSubtype::Accessory),
            bonus_atk: Some(0),
            equip_effect: Some(
                "1x/partie : quand le porteur subit une attaque, absorbe les dégâts (→ 0). À votre prochain tour, infligez ce montant à un ennemi (Impact)."
                    .to_string(),
            ),
            ..CardDef::new(
                "MG-018",
                "Dial d'Impact",
                CardType::Object,
                2,
                Faction::Pirate,
                Rarity::U,
                "ST01",
            )
        },
        // mugiwara.ts:215 — MG-019 Vivre Card.
        CardDef {
            subtype: Some(ObjectSubtype::Accessory),
            bonus_atk: Some(0),
            equip_effect: Some(
                "Si le porteur est KO : cherchez un personnage Mugiwara de coût ≤ 3 dans votre deck et ajoutez-le à votre main."
                    .to_string(),
            ),
            ..CardDef::new(
                "MG-019",
                "Vivre Card",
                CardType::Object,
                1,
                Faction::Pirate,
                Rarity::C,
                "ST01",
            )
        },
        // === NAVIRES ===
        // mugiwara.ts:223 — MG-020 Going Merry.
        CardDef {
            ship_passive: Some("Vos Mugiwara ont +1 PV au déploiement.".to_string()),
            ship_destroy_effect: Some(ShipDestroyEffect {
                heal_all: Some(2),
                draw: Some(1),
                ..Default::default()
            }),
            ..CardDef::new(
                "MG-020",
                "Going Merry",
                CardType::Ship,
                1,
                Faction::Pirate,
                Rarity::U,
                "ST01",
            )
        },
        // mugiwara.ts:229 — MG-021 Thousand Sunny.
        CardDef {
            ship_passive: Some("Vos Mugiwara ont +1 ATK.".to_string()),
            ship_active: Some(ShipActive {
                name: "Gaon Cannon".to_string(),
                cost: 3,
                description: "5 dégâts à un ennemi (Portée).".to_string(),
                once_per_game: Some(true),
            }),
            ..CardDef::new(
                "MG-021",
                "Thousand Sunny",
                CardType::Ship,
                2,
                Faction::Pirate,
                Rarity::R,
                "ST01",
            )
        },
        // === EVENEMENTS ===
        // mugiwara.ts:237 — MG-022 Volonté du D.
        CardDef {
            event_effect: Some(EventEffect::GainWill { amount: 2 }),
            ..CardDef::new(
                "MG-022",
                "Volonté du D.",
                CardType::Event,
                1,
                Faction::Pirate,
                Rarity::C,
                "ST01",
            )
        },
        // mugiwara.ts:242 — MG-023 Nakama !
        CardDef {
            event_effect: Some(EventEffect::Rally {
                atk: 1,
                def: 1,
                heal: 1,
            }),
            ..CardDef::new(
                "MG-023",
                "Nakama !",
                CardType::Event,
                2,
                Faction::Pirate,
                Rarity::U,
                "ST01",
            )
        },
        // mugiwara.ts:247 — MG-024 Flashback : Promesse.
        CardDef {
            event_effect: Some(EventEffect::BuffSingle {
                stat: AtkDefStat::Atk,
                amount: 3,
                duration: crate::types::BuffDuration::Permanent,
                requires_own_ko: Some(true),
            }),
            ..CardDef::new(
                "MG-024",
                "Flashback : Promesse",
                CardType::Event,
                1,
                Faction::Pirate,
                Rarity::C,
                "ST01",
            )
        },
        // mugiwara.ts:252 — MG-025 Tempête.
        CardDef {
            event_effect: Some(EventEffect::DamageEnemies {
                amount: 2,
                target: crate::types::DamageTarget::All,
                cursed_bonus: Some(3),
                sand: None,
                destroy_ships: None,
            }),
            ..CardDef::new(
                "MG-025",
                "Tempête",
                CardType::Event,
                3,
                Faction::Pirate,
                Rarity::U,
                "ST01",
            )
        },
        // mugiwara.ts:257 — MG-028 Coup de Burst (listed here, BEFORE the counters).
        CardDef {
            event_effect: Some(EventEffect::RushBuff { atk: 3 }),
            ..CardDef::new(
                "MG-028",
                "Coup de Burst",
                CardType::Event,
                3,
                Faction::Pirate,
                Rarity::R,
                "ST01",
            )
        },
        // === COUNTERS ===
        // mugiwara.ts:264 — MG-026 JE VEUX VIVRE !
        CardDef {
            counter_effect: Some(CounterEffect::Survive {
                description: "Un allié Mugiwara survit avec 1 PV. Une fois par personnage."
                    .to_string(),
            }),
            ..CardDef::new(
                "MG-026",
                "JE VEUX VIVRE !",
                CardType::Counter,
                0,
                Faction::Pirate,
                Rarity::R,
                "ST01",
            )
        },
        // mugiwara.ts:269 — MG-027 Drapeau Noir.
        CardDef {
            counter_effect: Some(CounterEffect::ReduceDamage {
                amount: 4,
                captain_bonus: None,
            }),
            ..CardDef::new(
                "MG-027",
                "Drapeau Noir",
                CardType::Counter,
                1,
                Faction::Pirate,
                Rarity::C,
                "ST01",
            )
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// The TS literal has exactly 28 entries (`mugiwaraCards.length === 28`).
    #[test]
    fn card_count_matches_ts() {
        assert_eq!(cards().len(), 28);
    }

    #[test]
    fn every_id_is_unique() {
        let all = cards();
        let ids: BTreeSet<&str> = all.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids.len(), all.len(), "duplicate card id in mugiwaraCards");
    }

    /// Array order is MG-001…MG-025, then **MG-028**, then MG-026, MG-027 —
    /// the order is load-bearing for instance-id numbering.
    #[test]
    fn source_order_puts_coup_de_burst_before_the_counters() {
        let ids: Vec<String> = cards().into_iter().map(|c| c.id).collect();
        let mut expected: Vec<String> = (1..=25).map(|n| format!("MG-{n:03}")).collect();
        expected.push("MG-028".to_string());
        expected.push("MG-026".to_string());
        expected.push("MG-027".to_string());
        assert_eq!(ids, expected);
    }

    /// Every entry of `mugiwaraDeck` (decks.ts) resolves to a definition of
    /// this set.
    #[test]
    fn deck_references_resolve() {
        let all = cards();
        let deck = crate::decks::mugiwara_deck();
        for entry in &deck.cards {
            assert!(
                all.iter().any(|c| c.id == entry.card_id),
                "mugiwaraDeck references unknown card {}",
                entry.card_id
            );
        }
        let total: u32 = deck.cards.iter().map(|e| e.count).sum();
        assert_eq!(total, crate::decks::DECK_SIZE);
    }

    #[test]
    fn every_card_is_pirate_st01() {
        for c in cards() {
            assert_eq!(c.set, "ST01", "{} has the wrong set", c.id);
            assert_eq!(c.faction, Faction::Pirate, "{} is not a pirate", c.id);
            assert_eq!(c.is_token, None, "{} must not be a token", c.id);
        }
    }

    /// The trickiest transcriptions: Zoro/Sanji's mutual synergy, Chopper's
    /// `transform`, Robin's `immobilize`, and Franky's `range` attack trait.
    #[test]
    fn structured_effects_are_transcribed() {
        let all = cards();
        let by_id = |id: &str| all.iter().find(|c| c.id == id).unwrap();

        let zoro = by_id("MG-001");
        assert_eq!(
            zoro.synergies.as_ref().unwrap()[0],
            SynergyDef {
                partner_id: "MG-002".to_string(),
                atk_bonus: 2,
                on_partner_ko: None,
            }
        );
        // Zoro has NO passive — the "Rivalité" is a synergy (this is why the
        // `threeWeaponSlots` branch is unreachable).
        assert!(zoro.passive.is_none());
        assert_eq!(
            zoro.special_attack.as_ref().unwrap().attack_traits,
            Some(vec![AttackTrait::Piercing])
        );

        let sanji = by_id("MG-002");
        assert_eq!(sanji.synergies.as_ref().unwrap()[0].partner_id, "MG-001");
        assert_eq!(
            sanji.passive.as_ref().unwrap().effects,
            vec![PassiveEffect::CannotAttackFemale]
        );

        let nami = by_id("MG-003");
        let base = nami.base_action.as_ref().unwrap();
        assert_eq!(base.scry, Some(2));
        assert_eq!(base.is_support, Some(true));
        assert_eq!(base.atk, 0);

        let usopp = by_id("MG-004");
        assert_eq!(usopp.base_action.as_ref().unwrap().bluff, Some(true));
        assert_eq!(
            usopp.passive.as_ref().unwrap().effects,
            vec![PassiveEffect::StartTurnBuffAlly {
                stat: AtkDefStat::Atk,
                amount: 1
            }]
        );

        let chopper = by_id("MG-005");
        let mp = chopper.special_attack.as_ref().unwrap();
        assert_eq!(
            mp.transform,
            Some(Transform {
                atk: 8,
                def: 2,
                pv: 6,
                turns: 2
            })
        );
        assert_eq!(mp.once_per_game, Some(true));
        assert_eq!(mp.is_support, Some(true));
        assert_eq!(mp.atk_bonus, 0);

        let robin = by_id("MG-006");
        assert_eq!(
            robin.special_attack.as_ref().unwrap().immobilize,
            Some(true)
        );
        assert_eq!(
            robin.passive.as_ref().unwrap().effects,
            vec![PassiveEffect::NoDodge]
        );

        let franky = by_id("MG-007");
        assert!(franky.passive.is_none());
        assert_eq!(franky.traits, Some(vec![Trait::Shield]));
        assert_eq!(
            franky.special_attack.as_ref().unwrap().attack_traits,
            Some(vec![AttackTrait::Range])
        );

        let brook = by_id("MG-008");
        assert_eq!(
            brook.passive.as_ref().unwrap().effects,
            vec![PassiveEffect::BuffAlly {
                stat: BuffStat::Def,
                amount: 1,
                filter: None,
            }]
        );
        assert_eq!(brook.base_action.as_ref().unwrap().buff_ally_atk, Some(2));
    }

    /// The three devil fruits: all `bonusAtk: 0`, base `grantsTraits [cursed]`,
    /// `minTurns: 5`; only Gomu Gomu grants `rush` on awakening.
    #[test]
    fn fruits_are_transcribed() {
        let all = cards();
        let fruits: Vec<&CardDef> = all
            .iter()
            .filter(|c| c.subtype == Some(ObjectSubtype::Fruit))
            .collect();
        assert_eq!(fruits.len(), 3);
        for f in &fruits {
            assert_eq!(f.bonus_atk, Some(0));
            let fx = f.fruit_effects.as_ref().unwrap();
            assert_eq!(fx.base.grants_traits, Some(vec![Trait::Cursed]));
            let aw = fx.awakening.as_ref().unwrap();
            assert_eq!(aw.min_turns, 5);
            assert!(aw.special_attack.is_some());
        }

        let gomu = fruits.iter().find(|c| c.id == "MG-014").unwrap();
        let aw = gomu
            .fruit_effects
            .as_ref()
            .unwrap()
            .awakening
            .as_ref()
            .unwrap();
        assert_eq!(aw.porteur_legitime, "Luffy");
        assert_eq!(aw.vol_cost, 3);
        assert_eq!(aw.atk_bonus, Some(3));
        assert_eq!(aw.grants_traits, Some(vec![Trait::Rush]));
        let kong = aw.special_attack.as_ref().unwrap();
        assert_eq!(kong.cost, 4);
        assert_eq!(kong.atk_bonus, 6);
        assert_eq!(kong.once_per_game, Some(true));
        // "Impact · Zone · ignore le Bouclier." is TEXT-ONLY: no structured
        // attackTraits / ignoreShield in the TS literal.
        assert_eq!(kong.attack_traits, None);
        assert_eq!(kong.ignore_shield, None);

        for id in ["MG-015", "MG-016"] {
            let f = fruits.iter().find(|c| c.id == id).unwrap();
            let aw = f
                .fruit_effects
                .as_ref()
                .unwrap()
                .awakening
                .as_ref()
                .unwrap();
            assert_eq!(aw.vol_cost, 2);
            assert_eq!(aw.atk_bonus, Some(0));
            assert_eq!(aw.grants_traits, None);
            let sp = aw.special_attack.as_ref().unwrap();
            assert_eq!(sp.cost, 3);
            assert_eq!(sp.atk_bonus, 0);
            assert_eq!(sp.once_per_game, None);
            // Neither Gigante Fleur (immobilize) nor Nemuriuta Flanc (sleep)
            // carries a structured control field — text only.
            assert_eq!(sp.immobilize, None);
            assert_eq!(sp.sleep, None);
        }
    }

    /// Card ids the engine special-cases by `defId` must keep their exact ids.
    #[test]
    fn engine_hardcoded_ids_are_present() {
        let all = cards();
        for id in [
            "MG-009", // Wado Ichimonji: +1 DEF for Zoro (board.ts equipObject)
            "MG-010", // Sandai Kitetsu: 1 damage at start of turn
            "MG-012", // Clima-Tact: cost 0 with Usopp + Nami
            "MG-013", // Kabuto: attacks cannot be dodged
            "MG-019", // Vivre Card: tutor on KO
            "MG-021", // Thousand Sunny: Gaon Cannon ship active
        ] {
            assert!(all.iter().any(|c| c.id == id), "{id} disappeared");
        }
        let clima = all.iter().find(|c| c.id == "MG-012").unwrap();
        assert_eq!(clima.restriction.as_deref(), Some("Nami"));
        assert_eq!(clima.grants_element, Some(Element::Thunder));
        let baril = all.iter().find(|c| c.id == "MG-017").unwrap();
        assert_eq!(baril.grants_element, Some(Element::Water));
    }

    #[test]
    fn events_and_counters_are_transcribed() {
        let all = cards();
        let by_id = |id: &str| all.iter().find(|c| c.id == id).unwrap();

        assert_eq!(
            by_id("MG-022").event_effect,
            Some(EventEffect::GainWill { amount: 2 })
        );
        assert_eq!(
            by_id("MG-023").event_effect,
            Some(EventEffect::Rally {
                atk: 1,
                def: 1,
                heal: 1
            })
        );
        assert_eq!(
            by_id("MG-024").event_effect,
            Some(EventEffect::BuffSingle {
                stat: AtkDefStat::Atk,
                amount: 3,
                duration: crate::types::BuffDuration::Permanent,
                requires_own_ko: Some(true),
            })
        );
        assert_eq!(
            by_id("MG-025").event_effect,
            Some(EventEffect::DamageEnemies {
                amount: 2,
                target: crate::types::DamageTarget::All,
                cursed_bonus: Some(3),
                sand: None,
                destroy_ships: None,
            })
        );
        assert_eq!(
            by_id("MG-028").event_effect,
            Some(EventEffect::RushBuff { atk: 3 })
        );
        assert_eq!(
            by_id("MG-026").counter_effect,
            Some(CounterEffect::Survive {
                description: "Un allié Mugiwara survit avec 1 PV. Une fois par personnage."
                    .to_string(),
            })
        );
        assert_eq!(by_id("MG-026").cost, 0);
        assert_eq!(
            by_id("MG-027").counter_effect,
            Some(CounterEffect::ReduceDamage {
                amount: 4,
                captain_bonus: None,
            })
        );

        // Going Merry has a destroy effect and no active; Thousand Sunny the
        // other way round.
        let merry = by_id("MG-020");
        assert_eq!(
            merry.ship_destroy_effect,
            Some(ShipDestroyEffect {
                heal_all: Some(2),
                draw: Some(1),
                ..Default::default()
            })
        );
        assert!(merry.ship_active.is_none());
        let sunny = by_id("MG-021");
        assert!(sunny.ship_destroy_effect.is_none());
        assert_eq!(sunny.ship_active.as_ref().unwrap().cost, 3);
    }

    /// Round-trips through serde: the JSON shape must stay byte-compatible with
    /// the TS literal (camelCase keys, absent optionals omitted).
    #[test]
    fn serde_round_trip_matches_ts_shape() {
        let all = cards();
        let json = serde_json::to_value(&all).unwrap();
        let zoro = &json[0];
        assert_eq!(zoro["id"], "MG-001");
        assert_eq!(zoro["type"], "character");
        assert_eq!(zoro["preferredRow"], "front");
        assert_eq!(zoro["synergies"][0]["partnerId"], "MG-002");
        assert_eq!(zoro["synergies"][0]["atkBonus"], 2);
        assert!(zoro.get("passive").is_none());
        assert!(zoro.get("isToken").is_none());

        let flashback = json
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == "MG-024")
            .unwrap();
        assert_eq!(flashback["eventEffect"]["type"], "buffSingle");
        assert_eq!(flashback["eventEffect"]["requiresOwnKO"], true);

        let back: Vec<CardDef> = serde_json::from_value(json).unwrap();
        assert_eq!(back, all);
    }
}
