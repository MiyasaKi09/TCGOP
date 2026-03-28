import type { CardDef, CaptainDef } from "@/types";

/** Global card definition registry */
const cardRegistry: Record<string, CardDef> = {};
const captainRegistry: Record<string, CaptainDef> = {};

export function registerCards(cards: CardDef[]): void {
  for (const card of cards) {
    cardRegistry[card.id] = card;
  }
}

export function registerCaptains(captains: CaptainDef[]): void {
  for (const cap of captains) {
    captainRegistry[cap.id] = cap;
  }
}

export function getCardDef(id: string): CardDef {
  const def = cardRegistry[id];
  if (!def) throw new Error(`Card not found: ${id}`);
  return def;
}

export function getCaptainDef(id: string): CaptainDef {
  const def = captainRegistry[id];
  if (!def) throw new Error(`Captain not found: ${id}`);
  return def;
}

export function getAllCardDefs(): Record<string, CardDef> {
  return { ...cardRegistry };
}

export function getAllCaptainDefs(): Record<string, CaptainDef> {
  return { ...captainRegistry };
}
