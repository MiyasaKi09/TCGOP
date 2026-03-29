"use client";

import { useState, useCallback, useMemo, useRef, useEffect } from "react";
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
  const aiRunning = useRef(false);
  const aiPlayer: PlayerId = humanPlayer === "player1" ? "player2" : "player1";

  // Dispatch human actions
  const dispatch = useCallback(
    (action: GameAction) => {
      setState((prev) => {
        try {
          return executeAction(prev, action);
        } catch (err) {
          console.error("Action failed:", err);
          return prev;
        }
      });
    },
    []
  );

  // AI main turn: runs when it's AI's turn and no pending attack
  const shouldAiAct =
    !state.winner &&
    state.currentPlayer === aiPlayer &&
    !state.pendingAttack;

  useEffect(() => {
    if (!shouldAiAct || aiRunning.current) return;
    aiRunning.current = true;

    const runOneAction = () => {
      setState((prev) => {
        if (prev.winner || prev.currentPlayer !== aiPlayer || prev.pendingAttack) {
          aiRunning.current = false;
          return prev;
        }
        try {
          const action = aiChooseAction(prev, aiPlayer);
          const next = executeAction(prev, action);
          if (action.type === "endTurn" || next.winner) {
            aiRunning.current = false;
            return next;
          }
          if (next.pendingAttack) {
            // AI attacked — need to wait for counter resolution
            aiRunning.current = false;
            return next;
          }
          setTimeout(runOneAction, 500);
          return next;
        } catch {
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

  // Auto-resolve pending attacks when AI is the defender
  // (human attacked, AI needs to counter or pass)
  const aiIsDefender =
    !state.winner &&
    state.pendingAttack !== null &&
    state.currentPlayer === humanPlayer; // human is attacking, so AI defends

  useEffect(() => {
    if (!aiIsDefender) return;

    const timer = setTimeout(() => {
      setState((prev) => {
        if (!prev.pendingAttack) return prev;

        // Check if AI has counters
        const aiActions = getValidActions(prev, aiPlayer);
        const surviveCounter = aiActions.find((a) => {
          if (a.type !== "playCounter") return false;
          try {
            const card = prev.cards[a.instanceId];
            const def = getCardDef(card.defId);
            return def.counterEffect?.type === "survive";
          } catch { return false; }
        });
        const reduceCounter = aiActions.find((a) => a.type === "playCounter");

        const action = surviveCounter ?? reduceCounter ?? { type: "passCounter" as const };
        try {
          return executeAction(prev, action);
        } catch {
          try { return executeAction(prev, { type: "passCounter" }); }
          catch { return prev; }
        }
      });
    }, 600);
    return () => clearTimeout(timer);
  }, [aiIsDefender, aiPlayer]);

  // Auto-resolve pending attacks when HUMAN is the defender
  // but has no counters available — auto-pass
  const humanIsDefender =
    !state.winner &&
    state.pendingAttack !== null &&
    state.currentPlayer === aiPlayer; // AI is attacking, so human defends

  const humanCounterActions = useMemo(() => {
    if (!humanIsDefender) return [];
    return getValidActions(state, humanPlayer);
  }, [humanIsDefender, state, humanPlayer]);

  // If human has no counters (only passCounter), auto-resolve after brief delay
  useEffect(() => {
    if (!humanIsDefender) return;
    // Only auto-pass if human has no real counter options (just passCounter)
    const realCounters = humanCounterActions.filter((a) => a.type !== "passCounter");
    if (realCounters.length > 0) return; // Human has options, show counter window

    const timer = setTimeout(() => {
      setState((prev) => {
        if (!prev.pendingAttack) return prev;
        try { return executeAction(prev, { type: "passCounter" }); }
        catch { return prev; }
      });
    }, 400);
    return () => clearTimeout(timer);
  }, [humanIsDefender, humanCounterActions]);

  // Resume AI turn after a pending attack resolves
  const aiShouldResume =
    !state.winner &&
    state.currentPlayer === aiPlayer &&
    !state.pendingAttack &&
    !aiRunning.current;

  useEffect(() => {
    if (!aiShouldResume) return;
    // Check if AI still has actions to do (not just endTurn)
    const actions = getValidActions(state, aiPlayer);
    const hasRealActions = actions.some((a) => a.type !== "endTurn");
    if (!hasRealActions) return;

    aiRunning.current = true;
    const timer = setTimeout(() => {
      setState((prev) => {
        if (prev.winner || prev.currentPlayer !== aiPlayer || prev.pendingAttack) {
          aiRunning.current = false;
          return prev;
        }
        try {
          const action = aiChooseAction(prev, aiPlayer);
          const next = executeAction(prev, action);
          if (action.type === "endTurn" || next.winner || next.pendingAttack) {
            aiRunning.current = false;
            return next;
          }
          // More actions to do — will be picked up by the main AI effect
          aiRunning.current = false;
          return next;
        } catch {
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
    }, 600);
    return () => clearTimeout(timer);
  }, [aiShouldResume, aiPlayer, state]);

  // Compute valid actions for the human
  const validActions = useMemo(() => {
    if (state.winner) return [];
    if (humanIsDefender) return humanCounterActions;
    if (state.pendingAttack) return []; // AI defending — no human actions
    if (state.currentPlayer !== humanPlayer) return [];
    return getValidActions(state, humanPlayer);
  }, [state, humanPlayer, humanIsDefender, humanCounterActions]);

  const isAiTurn = state.currentPlayer === aiPlayer && !state.pendingAttack;

  return { state, validActions, dispatch, isAiTurn, humanPlayer };
}
