import type { GameState, DeckDef } from "@/types";
import { registerCards, registerCaptains } from "./cardRegistry";
import { allCards, allCaptains } from "@/data/cards";
import { createInitialState, startTurn } from "./gameState";

let initialized = false;

/** Initialize the card registry (call once before creating games) */
export function initializeRegistry(): void {
  if (initialized) return;
  registerCards(allCards);
  registerCaptains(allCaptains);
  initialized = true;
}

/** Create a new game from two deck definitions */
export function createGame(
  p1Deck: DeckDef,
  p2Deck: DeckDef
): GameState {
  initializeRegistry();
  const state = createInitialState(p1Deck, p2Deck);
  // Start the first turn (untap, draw skipped for P1 T1, gain 1 Vol.)
  return startTurn(state);
}
