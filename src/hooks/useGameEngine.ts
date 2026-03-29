"use client";

import { useState, useCallback, useMemo, useEffect, useRef } from "react";
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
  // Counter that increments every state change to force useEffect re-trigger
  const [stateVersion, setStateVersion] = useState(0);
  const aiPlayer: PlayerId = humanPlayer === "player1" ? "player2" : "player1";

  // Wrap setState to also bump version
  const updateState = useCallback((updater: (prev: GameState) => GameState) => {
    setState((prev) => {
      const next = updater(prev);
      if (next !== prev) {
        // Schedule version bump on next microtask to ensure re-render
        setTimeout(() => setStateVersion((v) => v + 1), 0);
      }
      return next;
    });
  }, []);

  // Human dispatch
  const dispatch = useCallback((action: GameAction) => {
    updateState((prev) => {
      try {
        return executeAction(prev, action);
      } catch (err) {
        console.error("Action failed:", err);
        return prev;
      }
    });
  }, [updateState]);

  // Determine what needs to happen automatically
  const needsAutoAction = useMemo((): "ai_defend" | "auto_pass" | "ai_turn" | "none" => {
    if (state.winner) return "none";

    // Human attacked, AI must defend
    if (state.pendingAttack && state.currentPlayer === humanPlayer) {
      return "ai_defend";
    }

    // AI attacked, human has no real counters
    if (state.pendingAttack && state.currentPlayer === aiPlayer) {
      const humanActions = getValidActions(state, humanPlayer);
      const hasRealCounter = humanActions.some((a) => a.type !== "passCounter");
      if (!hasRealCounter) return "auto_pass";
      return "none";
    }

    // AI's turn
    if (state.currentPlayer === aiPlayer && !state.pendingAttack) {
      return "ai_turn";
    }

    return "none";
  }, [state, humanPlayer, aiPlayer]);

  // Execute automatic actions — depends on stateVersion to re-trigger
  useEffect(() => {
    if (needsAutoAction === "none") return;

    const delay = needsAutoAction === "auto_pass" ? 300 :
                  needsAutoAction === "ai_defend" ? 500 : 600;

    const timer = setTimeout(() => {
      updateState((prev) => {
        if (prev.winner) return prev;

        if (needsAutoAction === "ai_defend") {
          if (!prev.pendingAttack) return prev;
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
  }, [needsAutoAction, stateVersion, aiPlayer, updateState]);

  // Valid actions for human
  const validActions = useMemo(() => {
    if (state.winner) return [];
    if (state.pendingAttack && state.currentPlayer === aiPlayer) {
      return getValidActions(state, humanPlayer);
    }
    if (!state.pendingAttack && state.currentPlayer === humanPlayer) {
      return getValidActions(state, humanPlayer);
    }
    return [];
  }, [state, humanPlayer, aiPlayer]);

  const isAiTurn = state.currentPlayer === aiPlayer && !state.pendingAttack;

  return { state, validActions, dispatch, isAiTurn, humanPlayer };
}
