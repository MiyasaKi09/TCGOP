import type { CardDef, CaptainDef } from "@/types";
import { mugiwaraCards } from "./mugiwara";
import { marinesCards } from "./marines";
import { baroqueCards } from "./baroque";
import { redhairCards } from "./redhair";
import { tokenCards } from "./tokens";
import { allCaptains } from "./captains";

export const allCards: CardDef[] = [
  ...mugiwaraCards,
  ...marinesCards,
  ...baroqueCards,
  ...redhairCards,
  ...tokenCards,
];

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
export { baroqueCards } from "./baroque";
export { redhairCards } from "./redhair";
export { tokenCards } from "./tokens";
