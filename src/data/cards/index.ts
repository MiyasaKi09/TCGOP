import type { CardDef, CaptainDef } from "@/types";
import { mugiwaraCards } from "./mugiwara";
import { marinesCards } from "./marines";
import { allCaptains } from "./captains";

export const allCards: CardDef[] = [...mugiwaraCards, ...marinesCards];

export const cardRegistry: Record<string, CardDef> = {};
for (const card of allCards) {
  cardRegistry[card.id] = card;
}

export const captainRegistry: Record<string, CaptainDef> = {};
for (const cap of allCaptains) {
  captainRegistry[cap.id] = cap;
}

export { allCaptains } from "./captains";
export { mugiwaraCards } from "./mugiwara";
export { marinesCards } from "./marines";
