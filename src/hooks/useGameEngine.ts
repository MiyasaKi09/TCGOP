"use client";

import { useState, useCallback, useMemo, useRef, useEffect } from "react";
import type { GameState, GameAction, PlayerId, DeckDef } from "@/types";
import { createGame } from "@/engine/init";
import { executeAction, getValidActions } from "@/engine/turnManager";
import { aiChooseAction } from "@/engine/ai";

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
  const aiRunning = useRef(false);
  const aiPlayer: PlayerId = humanPlayer === "player1" ? "player2" : "player1";

  // Determine if AI should be acting
  const shouldAiAct =
    !state.winner &&
    state.currentPlayer === aiPlayer &&
    !state.pendingAttack;

  // Use effect to run AI turn when it's the AI's turn
  useEffect(() => {
    if (!shouldAiAct) return;
    if (aiRunning.current) return;

    aiRunning.current = true;

    const runOneAction = () => {
      setState((prev) => {
        // Safety checks
        if (prev.winner || prev.currentPlayer !== aiPlayer || prev.pendingAttack) {
          aiRunning.current = false;
          return prev;
        }

        try {
          const action = aiChooseAction(prev, aiPlayer);
          const next = executeAction(prev, action);

          if (action.type === "endTurn") {
            aiRunning.current = false;
            return next;
          }

          // If AI created a pending attack, stop and wait for human counter
          if (next.pendingAttack) {
            aiRunning.current = false;
            return next;
          }

          // Schedule next AI action
          setTimeout(runOneAction, 500);
          return next;
        } catch (err) {
          console.error("AI action failed:", err);
          // Try to end turn on error
          try {
            const ended = executeAction(prev, { type: "endTurn" });
            aiRunning.current = false;
            return ended;
          } catch {
            aiRunning.current = false;
            return prev;
          }
        }
      });
    };

    const timer = setTimeout(runOneAction, 700);
    return () => clearTimeout(timer);
  }, [shouldAiAct, aiPlayer]);

  // Dispatch human actions
  const dispatch = useCallback(
    (action: GameAction) => {
      setState((prev) => {
        try {
          const next = executeAction(prev, action);
          return next;
        } catch (err) {
          console.error("Action failed:", err);
          return prev;
        }
      });
    },
    []
  );

  // Compute valid actions for the human
  const validActions = useMemo(() => {
    if (state.winner) return [];

    // During counter window, the defender (human) can react
    if (state.pendingAttack) {
      // The defender is the opponent of currentPlayer
      const defenderId =
        state.currentPlayer === "player1" ? "player2" : "player1";
      if (defenderId === humanPlayer) {
        return getValidActions(state, humanPlayer);
      }
      // AI is the defender — AI will handle counters automatically
      return [];
    }

    // Normal turn — only current player can act
    if (state.currentPlayer !== humanPlayer) return [];
    return getValidActions(state, humanPlayer);
  }, [state, humanPlayer]);

  // Auto-resolve AI counter window (when AI is the defender)
  useEffect(() => {
    if (!state.pendingAttack) return;
    if (state.winner) return;

    const defenderId =
      state.currentPlayer === "player1" ? "player2" : "player1";

    // If AI is the defender, auto-play counter or pass
    if (defenderId === aiPlayer) {
      const timer = setTimeout(() => {
        setState((prev) => {
          if (!prev.pendingAttack) return prev;
          const aiCounterActions = getValidActions(prev, aiPlayer);

          // AI tries to play a survive counter first, then reduce, then pass
          const surviveCounter = aiCounterActions.find((a) => {
            if (a.type !== "playCounter") return false;
            const def = (() => {
              try {
                const card = prev.cards[a.instanceId];
                const { getCardDef } = require("@/engine/cardRegistry");
                return getCardDef(card.defId);
              } catch {
                return null;
              }
            })();
            return def?.counterEffect?.type === "survive";
          });

          const reduceCounter = aiCounterActions.find(
            (a) => a.type === "playCounter"
          );

          const actionToPlay = surviveCounter ?? reduceCounter ?? { type: "passCounter" as const };

          try {
            return executeAction(prev, actionToPlay);
          } catch {
            try {
              return executeAction(prev, { type: "passCounter" });
            } catch {
              return prev;
            }
          }
        });
      }, 800);
      return () => clearTimeout(timer);
    }
  }, [state.pendingAttack, state.currentPlayer, aiPlayer]);

  const isAiTurn = state.currentPlayer === aiPlayer && !state.pendingAttack;

  return { state, validActions, dispatch, isAiTurn, humanPlayer };
}
