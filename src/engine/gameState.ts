import { produce } from "immer";
import type {
  GameState,
  PlayerState,
  CardInstance,
  CaptainInstance,
  DeckDef,
  PlayerId,
  Slot,
  LogEntry,
} from "@/types";
import { getCardDef, getCaptainDef } from "./cardRegistry";
import { shuffle, generateInstanceId, STARTING_HAND_SIZE } from "./utils";
import { gainVolonte } from "./volonte";

// ============================================================
// Create initial game state from two deck definitions
// ============================================================

function createPlayerState(
  playerId: PlayerId,
  deckDef: DeckDef,
  allCards: Record<string, CardInstance>
): PlayerState {
  const captainDef = getCaptainDef(deckDef.captainId);

  // Build deck instances
  const deckInstanceIds: string[] = [];
  for (const entry of deckDef.cards) {
    for (let i = 0; i < entry.count; i++) {
      const instanceId = generateInstanceId(entry.cardId);
      const cardDef = getCardDef(entry.cardId);
      const instance: CardInstance = {
        instanceId,
        defId: entry.cardId,
        owner: playerId,
        zone: "deck",
        tapped: false,
        currentPv: cardDef.pv ?? 0,
        attachedObjects: [],
        modifiers: [],
        statusEffects: [],
        usedBaseAction: false,
        usedSpecialAttack: false,
        usedOnceAbilities: [],
      };
      allCards[instanceId] = instance;
      deckInstanceIds.push(instanceId);
    }
  }

  // Shuffle deck
  const shuffledDeck = shuffle(deckInstanceIds);

  // Draw starting hand
  const hand = shuffledDeck.splice(0, STARTING_HAND_SIZE);
  for (const id of hand) {
    allCards[id].zone = "hand";
  }

  // Captain instance
  const captain: CaptainInstance = {
    defId: deckDef.captainId,
    owner: playerId,
    flipped: false,
    currentPv: captainDef.recto.pv,
    tapped: false,
    modifiers: [],
    statusEffects: [],
    usedBaseAction: false,
    usedSpecialAttack: false,
    usedOnceAbilities: [],
  };

  const emptyBoard: Record<Slot, string | null> = {
    V1: null, V2: null, V3: null,
    A1: null, A2: null, A3: null,
  };

  return {
    id: playerId,
    captain,
    deck: shuffledDeck,
    hand,
    graveyard: [],
    board: emptyBoard,
    activeShip: null,
    volonte: 0,
    usedFreeMove: false,
    hasDrawn: false,
    observationUsed: false,
    armamentUsed: false,
    kingUsed: false,
  };
}

export function createInitialState(
  p1Deck: DeckDef,
  p2Deck: DeckDef
): GameState {
  const allCards: Record<string, CardInstance> = {};

  const player1 = createPlayerState("player1", p1Deck, allCards);
  const player2 = createPlayerState("player2", p2Deck, allCards);

  return {
    cards: allCards,
    players: { player1, player2 },
    turnNumber: 1,
    currentPlayer: "player1",
    phase: "main", // T1 starts in main (untap/draw/will handled by startTurn)
    pendingAttack: null,
    log: [],
    winner: null,
    firstPlayer: "player1",
  };
}

// ============================================================
// Turn lifecycle
// ============================================================

/**
 * Start a new turn for the current player.
 * Called at the beginning of each turn.
 * Handles: untap → draw → gain Volonte.
 */
export function startTurn(state: GameState): GameState {
  let next = state;

  // 1. Untap all characters
  next = untapAll(next);

  // 2. Reset per-turn flags
  next = resetTurnFlags(next);

  // 3. Draw 1 card (J1 does NOT draw on T1)
  const isFirstPlayerFirstTurn =
    next.turnNumber === 1 && next.currentPlayer === next.firstPlayer;
  if (!isFirstPlayerFirstTurn) {
    next = drawCard(next, next.currentPlayer);
  }

  // 4. Gain Volonte
  next = gainVolonte(next);

  // 5. Set phase to main
  next = produce(next, (draft) => {
    draft.phase = "main";
  });

  // 6. Process start-of-turn effects (burn, poison, etc.)
  next = processStartOfTurnEffects(next);

  // 6b. Check for KO from burn/desiccation damage
  const { removeFromBoard } = require("./board");
  const { grantKOBonus } = require("./volonte");
  const currentPlayerId = next.currentPlayer;
  const currentPlayerState = next.players[currentPlayerId];
  for (const slot of Object.values(currentPlayerState.board)) {
    if (!slot) continue;
    const card = next.cards[slot];
    if (card && card.zone === "board" && card.currentPv <= 0) {
      const cardDef = require("./cardRegistry").getCardDef(card.defId);
      next = addLog(next, currentPlayerId, `${cardDef.name} est KO (brulure/effet) !`);
      next = grantKOBonus(next, getOpponent(currentPlayerId));
      next = removeFromBoard(next, slot);
    }
  }

  // 7. Apply start-of-turn passives (healAdjacent, etc.)
  const { applyStartOfTurnPassives, recalculatePassiveBuffs } = require("./passives");
  next = applyStartOfTurnPassives(next, next.currentPlayer);

  // 8. Recalculate passive buffs (captain, synergies)
  next = recalculatePassiveBuffs(next, next.currentPlayer);
  next = recalculatePassiveBuffs(next, getOpponent(next.currentPlayer));

  return next;
}

/**
 * End the current player's turn. Switch to opponent.
 */
export function endTurn(state: GameState): GameState {
  return produce(state, (draft) => {
    draft.phase = "end";
    // Switch player
    const nextPlayer: PlayerId =
      draft.currentPlayer === "player1" ? "player2" : "player1";
    // If switching FROM player2 back to player1, increment turn number
    if (draft.currentPlayer === "player2") {
      draft.turnNumber += 1;
    }
    draft.currentPlayer = nextPlayer;
  });
}

/**
 * Untap all characters and captain for the current player.
 */
function untapAll(state: GameState): GameState {
  return produce(state, (draft) => {
    const player = draft.players[draft.currentPlayer];
    // Untap board characters
    for (const slot of Object.values(player.board)) {
      if (slot) {
        const card = draft.cards[slot];
        if (card) card.tapped = false;
      }
    }
    // Untap captain if on board
    player.captain.tapped = false;
  });
}

/**
 * Reset per-turn flags for all characters of current player.
 */
function resetTurnFlags(state: GameState): GameState {
  return produce(state, (draft) => {
    const player = draft.players[draft.currentPlayer];
    player.usedFreeMove = false;
    player.hasDrawn = false;
    player.observationUsed = false;
    player.armamentUsed = false;

    // Reset character per-turn flags
    for (const slot of Object.values(player.board)) {
      if (slot) {
        const card = draft.cards[slot];
        if (card) {
          card.usedBaseAction = false;
          card.usedSpecialAttack = false;
          card.logiaUsedThisTurn = false;
        }
      }
    }
    player.captain.usedBaseAction = false;
    player.captain.usedSpecialAttack = false;

    // Expire turn-duration modifiers on all player's cards
    for (const slot of Object.values(player.board)) {
      if (slot) {
        const card = draft.cards[slot];
        if (card) {
          card.modifiers = card.modifiers.filter(
            (m) => m.duration !== "turn"
          );
        }
      }
    }
    player.captain.modifiers = player.captain.modifiers.filter(
      (m) => m.duration !== "turn"
    );
  });
}

/**
 * Draw a card from deck to hand.
 */
export function drawCard(
  state: GameState,
  playerId: PlayerId
): GameState {
  return produce(state, (draft) => {
    const player = draft.players[playerId];
    if (player.deck.length === 0) {
      // Deck empty — player loses at next draw
      draft.winner = playerId === "player1" ? "player2" : "player1";
      return;
    }
    const drawnId = player.deck.shift()!;
    player.hand.push(drawnId);
    draft.cards[drawnId].zone = "hand";
    player.hasDrawn = true;
  });
}

/**
 * Process start-of-turn status effects (burn, poison, etc.)
 */
function processStartOfTurnEffects(state: GameState): GameState {
  return produce(state, (draft) => {
    const player = draft.players[draft.currentPlayer];

    // Process status effects on board characters
    for (const slot of Object.values(player.board)) {
      if (!slot) continue;
      const card = draft.cards[slot];
      if (!card) continue;

      const remaining: typeof card.statusEffects = [];
      for (const effect of card.statusEffects) {
        // Apply damage
        if (effect.type === "burn" || effect.type === "poison") {
          card.currentPv -= effect.damagePerTurn;
          if (effect.type === "poison" && card.currentPv < 1) {
            card.currentPv = 1; // Poison can't kill
          }
        }
        if (effect.type === "desiccation") {
          card.currentPv -= effect.damagePerTurn;
        }
        // Decrement turns
        if (effect.turnsRemaining > 0) {
          effect.turnsRemaining -= 1;
          if (effect.turnsRemaining > 0) {
            remaining.push(effect);
          }
        } else if (effect.turnsRemaining === -1) {
          // Permanent (poison)
          remaining.push(effect);
        }
      }
      card.statusEffects = remaining;
    }

    // Process captain status effects
    const cap = player.captain;
    const capRemaining: typeof cap.statusEffects = [];
    for (const effect of cap.statusEffects) {
      if (effect.type === "burn" || effect.type === "poison") {
        cap.currentPv -= effect.damagePerTurn;
        if (effect.type === "poison" && cap.currentPv < 1) {
          cap.currentPv = 1;
        }
      }
      if (effect.turnsRemaining > 0) {
        effect.turnsRemaining -= 1;
        if (effect.turnsRemaining > 0) {
          capRemaining.push(effect);
        }
      } else if (effect.turnsRemaining === -1) {
        capRemaining.push(effect);
      }
    }
    cap.statusEffects = capRemaining;
  });
}

// ============================================================
// Win condition check
// ============================================================

export function checkWinCondition(state: GameState): PlayerId | null {
  if (state.winner) return state.winner;

  const p1Pv = state.players.player1.captain.currentPv;
  const p2Pv = state.players.player2.captain.currentPv;

  if (p1Pv <= 0 && p2Pv <= 0) {
    // Both die simultaneously — active player loses
    return state.currentPlayer === "player1" ? "player2" : "player1";
  }
  if (p1Pv <= 0) return "player2";
  if (p2Pv <= 0) return "player1";

  return null;
}

// ============================================================
// Utility: add log entry
// ============================================================

export function addLog(
  state: GameState,
  player: PlayerId,
  message: string
): GameState {
  return produce(state, (draft) => {
    draft.log.push({
      turn: draft.turnNumber,
      player,
      message,
    });
  });
}

/**
 * Get opponent of a player.
 */
export function getOpponent(playerId: PlayerId): PlayerId {
  return playerId === "player1" ? "player2" : "player1";
}
