import type { DeckDef } from "@/types";

export const mugiwaraDeck: DeckDef = {
  name: "Mugiwara - Pre-Ellipse",
  captainId: "CAP-LUFFY",
  cards: [
    // Personnages (21 cards)
    { cardId: "ST01-001", count: 3 }, // Zoro
    { cardId: "ST01-002", count: 3 }, // Sanji
    { cardId: "ST01-003", count: 3 }, // Nami
    { cardId: "ST01-004", count: 3 }, // Usopp
    { cardId: "ST01-005", count: 3 }, // Chopper
    { cardId: "ST01-006", count: 2 }, // Robin
    { cardId: "ST01-007", count: 2 }, // Franky
    { cardId: "ST01-008", count: 2 }, // Brook

    // Armes (5 cards)
    { cardId: "ST01-009", count: 1 }, // Wado Ichimonji
    { cardId: "ST01-010", count: 1 }, // Sandai Kitetsu
    { cardId: "ST01-011", count: 1 }, // Yubashiri
    { cardId: "ST01-012", count: 1 }, // Clima-Tact
    { cardId: "ST01-013", count: 1 }, // Kabuto

    // Fruits (3 cards)
    { cardId: "ST01-014", count: 1 }, // Gomu Gomu
    { cardId: "ST01-015", count: 1 }, // Hana Hana
    { cardId: "ST01-016", count: 1 }, // Yomi Yomi

    // Accessoires (3 cards)
    { cardId: "ST01-017", count: 1 }, // Baril d'Eau
    { cardId: "ST01-018", count: 1 }, // Dial d'Impact
    { cardId: "ST01-019", count: 1 }, // Vivre Card

    // Navires (4 cards)
    { cardId: "ST01-020", count: 2 }, // Going Merry
    { cardId: "ST01-021", count: 2 }, // Thousand Sunny

    // Evenements (10 cards)
    { cardId: "ST01-022", count: 2 }, // Volonte du D
    { cardId: "ST01-023", count: 2 }, // Nakama !
    { cardId: "ST01-024", count: 2 }, // Flashback
    { cardId: "ST01-025", count: 2 }, // Tempete
    { cardId: "ST01-028", count: 2 }, // Coup de Burst

    // Counters (4 cards)
    { cardId: "ST01-026", count: 2 }, // JE VEUX VIVRE !
    { cardId: "ST01-027", count: 2 }, // Drapeau Noir
  ],
};

export const marinesDeck: DeckDef = {
  name: "Marines - Justice Absolue",
  captainId: "CAP-AKAINU",
  cards: [
    // Personnages (21 cards)
    { cardId: "ST02-001", count: 3 }, // Coby
    { cardId: "ST02-002", count: 2 }, // Helmeppo
    { cardId: "ST02-003", count: 3 }, // Tashigi
    { cardId: "ST02-004", count: 2 }, // Smoker
    { cardId: "ST02-005", count: 2 }, // Sentomaru
    { cardId: "ST02-006", count: 2 }, // Momonga
    { cardId: "ST02-007", count: 2 }, // Garp
    { cardId: "ST02-008", count: 2 }, // Kizaru
    { cardId: "ST02-009", count: 2 }, // Aokiji
    { cardId: "ST02-010", count: 1 }, // Sengoku

    // Fruits (2 cards)
    { cardId: "ST02-011", count: 1 }, // Magu Magu
    { cardId: "ST02-012", count: 1 }, // Moku Moku

    // Armes (2 cards)
    { cardId: "ST02-013", count: 1 }, // Shigure
    { cardId: "ST02-014", count: 1 }, // Jitte

    // Accessoires (3 cards)
    { cardId: "ST02-015", count: 1 }, // Menottes
    { cardId: "ST02-016", count: 1 }, // Canon Marine
    { cardId: "ST02-017", count: 1 }, // Boulet G. Marin

    // Navires (4 cards)
    { cardId: "ST02-018", count: 2 }, // Navire de Guerre
    { cardId: "ST02-019", count: 2 }, // Navire de Justice

    // Evenements (14 cards)
    { cardId: "ST02-020", count: 2 }, // Buster Call
    { cardId: "ST02-021", count: 2 }, // Promotion
    { cardId: "ST02-022", count: 2 }, // Justice Absolue
    { cardId: "ST02-023", count: 2 }, // Renforts
    { cardId: "ST02-024", count: 2 }, // Ordre de Tir
    { cardId: "ST02-027", count: 2 }, // Embargo
    { cardId: "ST02-028", count: 2 }, // Execution Publique

    // Counters (4 cards)
    { cardId: "ST02-025", count: 2 }, // Manteau de Justice
    { cardId: "ST02-026", count: 2 }, // Mur d'Acier
  ],
};

// Verify deck sizes
function verifyDeck(deck: DeckDef): void {
  const total = deck.cards.reduce((sum, e) => sum + e.count, 0);
  if (total !== 50) {
    console.warn(`Deck "${deck.name}" has ${total} cards (expected 50)`);
  }
}

verifyDeck(mugiwaraDeck);
verifyDeck(marinesDeck);
