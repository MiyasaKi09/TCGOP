"use client";

import { useState, useCallback, useMemo, useRef } from "react";
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

  const isAiTurn = state.currentPlayer === aiPlayer && !state.pendingAttack;

  const dispatch = useCallback(
    (action: GameAction) => {
      setState((prev) => {
        try {
          const next = executeAction(prev, action);

          // After human ends turn, the engine does startTurn for AI.
          // Schedule AI turn.
          if (
            action.type === "endTurn" &&
            next.currentPlayer === aiPlayer &&
            !next.winner
          ) {
            scheduleAiTurn(next);
          }

          // After passing counter, if it's still a pending attack resolve scenario
          if (action.type === "passCounter" || action.type === "playCounter") {
            // Check if now it's AI's main phase turn and needs to continue
            if (
              next.currentPlayer === aiPlayer &&
              !next.pendingAttack &&
              !next.winner
            ) {
              scheduleAiTurn(next);
            }
          }

          return next;
        } catch (err) {
          console.error("Action failed:", err);
          return prev;
        }
      });
    },
    [aiPlayer]
  );

  const scheduleAiTurn = useCallback(
    (currentState: GameState) => {
      if (aiRunning.current) return;
      aiRunning.current = true;

      const runAi = (st: GameState, step: number) => {
        if (st.winner || st.currentPlayer !== aiPlayer || step > 50) {
          aiRunning.current = false;
          return;
        }

        const action = aiChooseAction(st, aiPlayer);

        setTimeout(() => {
          setState((prev) => {
            // Re-evaluate: state may have changed
            const currentAiState =
              prev.currentPlayer === aiPlayer ? prev : st;
            if (currentAiState.winner) {
              aiRunning.current = false;
              return prev;
            }

            try {
              const next = executeAction(currentAiState, action);

              if (action.type === "endTurn") {
                aiRunning.current = false;
                return next;
              }

              // If there's a pending attack, wait for human counter response
              if (next.pendingAttack) {
                aiRunning.current = false;
                return next;
              }

              // Continue AI turn
              setTimeout(() => runAi(next, step + 1), 400);
              return next;
            } catch {
              // If action fails, just end turn
              try {
                const ended = executeAction(currentAiState, {
                  type: "endTurn",
                });
                aiRunning.current = false;
                return ended;
              } catch {
                aiRunning.current = false;
                return prev;
              }
            }
          });
        }, 600);
      };

      setTimeout(() => runAi(currentState, 0), 800);
    },
    [aiPlayer]
  );

  const validActions = useMemo(
    () => {
      // During counter window, the defender can act
      if (state.pendingAttack) {
        const defenderId = state.currentPlayer === "player1" ? "player2" : "player1";
        if (defenderId === humanPlayer) {
          return getValidActions(state, humanPlayer);
        }
        return [];
      }
      if (state.currentPlayer !== humanPlayer) return [];
      return getValidActions(state, humanPlayer);
    },
    [state, humanPlayer]
  );

  return { state, validActions, dispatch, isAiTurn, humanPlayer };
}
