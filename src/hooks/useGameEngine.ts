"use client";

import { useState, useCallback, useMemo, useEffect } from "react";
import type { GameState, GameAction, PlayerId, DeckDef } from "@/types";
import { createGame } from "@/engine/init";
import { executeAction, getValidActions } from "@/engine/turnManager";
import { aiChooseAction } from "@/engine/ai";
import { getCardDef } from "@/engine/cardRegistry";

interface UseGameEngineReturn {
  state: GameState;
  validActions: GameAction[];
  dispatch: (action: GameAction) => void;
  isAiTurn: boolean;
  humanPlayer: PlayerId;
}

export function useGameEngine(
  humanDeck: DeckDef,
  aiDeck: DeckDef,
  humanPlayer: PlayerId = "player1"
): UseGameEngineReturn {
  const [state, setState] = useState<GameState>(() =>
    createGame(humanDeck, aiDeck)
  );
  const aiPlayer: PlayerId = humanPlayer === "player1" ? "player2" : "player1";

  // Human dispatch
  const dispatch = useCallback((action: GameAction) => {
    setState((prev) => {
      try {
        return executeAction(prev, action);
      } catch (err) {
        console.error("Action failed:", err);
        return prev;
      }
    });
  }, []);

  // Determine what needs to happen automatically
  const needsAutoAction = useMemo(() => {
    if (state.winner) return "none" as const;

    // Case A: Human attacked, AI must defend (counter or pass)
    if (state.pendingAttack && state.currentPlayer === humanPlayer) {
      return "ai_defend" as const;
    }

    // Case B: AI attacked, human has no real counters → auto pass
    if (state.pendingAttack && state.currentPlayer === aiPlayer) {
      const humanActions = getValidActions(state, humanPlayer);
      const hasRealCounter = humanActions.some((a) => a.type !== "passCounter");
      if (!hasRealCounter) return "auto_pass" as const;
      return "none" as const; // Human has counters, show UI
    }

    // Case C: AI's turn, no pending attack
    if (state.currentPlayer === aiPlayer && !state.pendingAttack) {
      return "ai_turn" as const;
    }

    return "none" as const;
  }, [state, humanPlayer, aiPlayer]);

  // Execute automatic actions with a delay
  useEffect(() => {
    if (needsAutoAction === "none") return;

    const delay = needsAutoAction === "auto_pass" ? 300 :
                  needsAutoAction === "ai_defend" ? 500 : 600;

    const timer = setTimeout(() => {
      setState((prev) => {
        // Re-check conditions inside setState to avoid stale state
        if (prev.winner) return prev;

        if (needsAutoAction === "ai_defend") {
          if (!prev.pendingAttack) return prev;
          // AI tries to play counter
          const aiActions = getValidActions(prev, aiPlayer);
          const survive = aiActions.find((a) => {
            if (a.type !== "playCounter") return false;
            try {
              const card = prev.cards[a.instanceId];
              const def = getCardDef(card.defId);
              return def.counterEffect?.type === "survive";
            } catch { return false; }
          });
          const reduce = aiActions.find((a) => a.type === "playCounter");
          const action: GameAction = survive ?? reduce ?? { type: "passCounter" };
          try { return executeAction(prev, action); }
          catch {
            try { return executeAction(prev, { type: "passCounter" }); }
            catch { return prev; }
          }
        }

        if (needsAutoAction === "auto_pass") {
          if (!prev.pendingAttack) return prev;
          try { return executeAction(prev, { type: "passCounter" }); }
          catch { return prev; }
        }

        if (needsAutoAction === "ai_turn") {
          if (prev.currentPlayer !== aiPlayer || prev.pendingAttack) return prev;
          try {
            const action = aiChooseAction(prev, aiPlayer);
            return executeAction(prev, action);
          } catch {
            try { return executeAction(prev, { type: "endTurn" }); }
            catch { return prev; }
          }
        }

        return prev;
      });
    }, delay);

    return () => clearTimeout(timer);
  }, [needsAutoAction, aiPlayer]);

  // Valid actions for human
  const validActions = useMemo(() => {
    if (state.winner) return [];
    // Human defending against AI attack
    if (state.pendingAttack && state.currentPlayer === aiPlayer) {
      return getValidActions(state, humanPlayer);
    }
    // Human's turn
    if (!state.pendingAttack && state.currentPlayer === humanPlayer) {
      return getValidActions(state, humanPlayer);
    }
    return [];
  }, [state, humanPlayer, aiPlayer]);

  const isAiTurn = state.currentPlayer === aiPlayer && !state.pendingAttack;

  return { state, validActions, dispatch, isAiTurn, humanPlayer };
}
