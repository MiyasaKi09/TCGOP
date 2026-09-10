//! Token bodies deployed by effects (never part of a deck).
//!
//! Port of `src/data/cards/tokens.ts` (`tokenCards`) — 3 card
//! definitions, in the exact source order (the order is load-bearing: the
//! registry is filled by iterating it, and later ids overwrite earlier ones).

use crate::types::{BaseAction, CardDef, CardType, Faction, Rarity, Row};

/// TS `tokenCards: CardDef[]` (`src/data/cards/tokens.ts`).
///
/// Returns the 3 definitions of the TOKEN set in source order.
pub fn cards() -> Vec<CardDef> {
    vec![
        // TOK-MARINE — tokens.ts:5
        {
            let mut c = CardDef::new(
                "TOK-MARINE",
                "Jeton Marine",
                CardType::Character,
                0,
                Faction::Marine,
                Rarity::C,
                "TOKEN",
            );
            c.is_token = Some(true);
            c.atk = Some(2);
            c.def = Some(1);
            c.pv = Some(3);
            c.tags = Some(vec!["marine".to_string(), "soldat".to_string()]);
            c.preferred_row = Some(Row::Front);
            c.base_action = Some(BaseAction {
                name: "Tir".to_string(),
                atk: 2,
                description: Some("Un simple soldat.".to_string()),
                ..Default::default()
            });
            c
        },
        // TOK-AGENT — tokens.ts:11
        {
            let mut c = CardDef::new(
                "TOK-AGENT",
                "Agent Baroque Works",
                CardType::Character,
                0,
                Faction::Pirate,
                Rarity::C,
                "TOKEN",
            );
            c.is_token = Some(true);
            c.atk = Some(3);
            c.def = Some(1);
            c.pv = Some(3);
            c.tags = Some(vec!["baroque".to_string()]);
            c.preferred_row = Some(Row::Front);
            c.base_action = Some(BaseAction {
                name: "Frappe".to_string(),
                atk: 3,
                description: Some("Un agent anonyme.".to_string()),
                ..Default::default()
            });
            c
        },
        // TOK-BANANAWANI — tokens.ts:17
        {
            let mut c = CardDef::new(
                "TOK-BANANAWANI",
                "Bananawani",
                CardType::Character,
                0,
                Faction::Pirate,
                Rarity::C,
                "TOKEN",
            );
            c.is_token = Some(true);
            c.atk = Some(4);
            c.def = Some(1);
            c.pv = Some(4);
            c.tags = Some(vec!["baroque".to_string()]);
            c.preferred_row = Some(Row::Front);
            c.base_action = Some(BaseAction {
                name: "Morsure".to_string(),
                atk: 4,
                description: Some("Un crocodile-banane affamé.".to_string()),
                ..Default::default()
            });
            c
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// `tokenCards` has exactly 3 entries (TS `tokens.ts`), matching the
    /// 114-card total asserted by `cards::all_cards`.
    #[test]
    fn token_card_count_matches_ts() {
        assert_eq!(cards().len(), 3);
    }

    /// Every id is unique — a duplicate would silently overwrite the earlier
    /// definition when the registry is folded.
    #[test]
    fn token_ids_are_unique_and_in_source_order() {
        let ids: Vec<String> = cards().into_iter().map(|c| c.id).collect();
        assert_eq!(ids, vec!["TOK-MARINE", "TOK-AGENT", "TOK-BANANAWANI"]);
        let unique: BTreeSet<&String> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len());
    }

    /// Tokens are never part of a deck: all three carry `isToken: true`,
    /// `cost: 0`, `set: "TOKEN"`, rarity `C` and `preferredRow: "front"`.
    #[test]
    fn tokens_share_the_common_shape() {
        for c in cards() {
            assert_eq!(c.is_token, Some(true), "{}", c.id);
            assert_eq!(c.cost, 0, "{}", c.id);
            assert_eq!(c.set, "TOKEN", "{}", c.id);
            assert_eq!(c.card_type, CardType::Character, "{}", c.id);
            assert_eq!(c.rarity, Rarity::C, "{}", c.id);
            assert_eq!(c.preferred_row, Some(Row::Front), "{}", c.id);
            // No token has traits, a special attack, a passive or synergies.
            assert!(c.traits.is_none(), "{}", c.id);
            assert!(c.special_attack.is_none(), "{}", c.id);
            assert!(c.passive.is_none(), "{}", c.id);
            assert!(c.synergies.is_none(), "{}", c.id);
        }
    }

    /// Statline + baseAction transcription, literal from the TS.
    #[test]
    fn token_statlines_match_ts() {
        let cs = cards();

        let marine = &cs[0];
        assert_eq!(marine.name, "Jeton Marine");
        assert_eq!(marine.faction, Faction::Marine);
        assert_eq!(
            (marine.atk, marine.def, marine.pv),
            (Some(2), Some(1), Some(3))
        );
        assert_eq!(
            marine.tags.as_deref(),
            Some(&["marine".to_string(), "soldat".to_string()][..])
        );
        let a = marine.base_action.as_ref().unwrap();
        assert_eq!(a.name, "Tir");
        assert_eq!(a.atk, 2);
        assert_eq!(a.description.as_deref(), Some("Un simple soldat."));

        let agent = &cs[1];
        assert_eq!(agent.name, "Agent Baroque Works");
        assert_eq!(agent.faction, Faction::Pirate);
        assert_eq!(
            (agent.atk, agent.def, agent.pv),
            (Some(3), Some(1), Some(3))
        );
        assert_eq!(agent.tags.as_deref(), Some(&["baroque".to_string()][..]));
        let a = agent.base_action.as_ref().unwrap();
        assert_eq!(a.name, "Frappe");
        assert_eq!(a.atk, 3);
        assert_eq!(a.description.as_deref(), Some("Un agent anonyme."));

        let bana = &cs[2];
        assert_eq!(bana.name, "Bananawani");
        assert_eq!(bana.faction, Faction::Pirate);
        assert_eq!((bana.atk, bana.def, bana.pv), (Some(4), Some(1), Some(4)));
        assert_eq!(bana.tags.as_deref(), Some(&["baroque".to_string()][..]));
        let a = bana.base_action.as_ref().unwrap();
        assert_eq!(a.name, "Morsure");
        assert_eq!(a.atk, 4);
        // Accents are byte-exact with the TS source.
        assert_eq!(
            a.description.as_deref(),
            Some("Un crocodile-banane affamé.")
        );
    }

    /// The baseAction carries only `name` / `atk` / `description`; every other
    /// optional flag is absent (no element, no traits, not a support action).
    #[test]
    fn token_base_actions_have_no_extra_flags() {
        for c in cards() {
            let a = c.base_action.as_ref().unwrap();
            assert!(a.attack_traits.is_none(), "{}", c.id);
            assert!(a.element.is_none(), "{}", c.id);
            assert!(a.is_support.is_none(), "{}", c.id);
            assert!(a.heal_amount.is_none(), "{}", c.id);
            assert!(a.immobilize.is_none(), "{}", c.id);
            assert!(a.cannot_be_dodged.is_none(), "{}", c.id);
            assert!(a.strip_stealth.is_none(), "{}", c.id);
            // The baseAction ATK mirrors the card's own ATK for every token.
            assert_eq!(Some(a.atk), c.atk, "{}", c.id);
        }
    }
}
