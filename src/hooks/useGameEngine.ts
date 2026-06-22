"use client";

import { useState, useCallback, useMemo, useEffect, useRef } from "react";
import type { GameState, GameAction, PlayerId, DeckDef } from "@/types";
import { createGame } from "@/engine/init";
import { executeAction, getValidActions } from "@/engine/turnManager";
import { aiChooseAction, type Difficulty } from "@/engine/ai";
import { buildAnnouncement, announceDuration, type PlayAnnouncement } from "@/lib/announce";

interface UseGameEngineReturn {
  state: GameState;
  validActions: GameAction[];
  dispatch: (action: GameAction) => void;
  isAiTurn: boolean;
  humanPlayer: PlayerId;
  announcements: PlayAnnouncement[];
  dismissAnnouncement: (id: number) => void;
}

export function useGameEngine(
  humanDeck: DeckDef,
  aiDeck: DeckDef,
  humanPlayer: PlayerId = "player1",
  difficulty: Difficulty = "intermediate"
): UseGameEngineReturn {
  const [state, setState] = useState<GameState>(() => createGame(humanDeck, aiDeck));
  const [stateVersion, setStateVersion] = useState(0);
  const [announcements, setAnnouncements] = useState<PlayAnnouncement[]>([]);
  const aiPlayer: PlayerId = humanPlayer === "player1" ? "player2" : "player1";

  const stateRef = useRef(state);
  stateRef.current = state;
  const busyUntilRef = useRef(0);

  const dismissAnnouncement = useCallback((id: number) => {
    setAnnouncements((a) => a.filter((x) => x.id !== id));
  }, []);

  // Record a reveal for an action (built from the pre-execution state) + extend the
  // AI pacing window so reveals don't overlap and stay readable.
  const announce = useCallback((action: GameAction, pre: GameState) => {
    const ann = buildAnnouncement(action, pre, humanPlayer);
    if (!ann) return;
    setAnnouncements((a) => [...a, ann]);
    busyUntilRef.current = Math.max(busyUntilRef.current, Date.now()) + announceDuration(ann);
  }, [humanPlayer]);

  const updateState = useCallback((updater: (prev: GameState) => GameState) => {
    setState((prev) => {
      const next = updater(prev);
      if (next !== prev) setTimeout(() => setStateVersion((v) => v + 1), 0);
      return next;
    });
  }, []);

  // Human dispatch — announce from the current state, then apply.
  const dispatch = useCallback((action: GameAction) => {
    announce(action, stateRef.current);
    updateState((prev) => {
      try { return executeAction(prev, action); }
      catch (err) { console.error("Action failed:", err); return prev; }
    });
  }, [announce, updateState]);

  const needsAutoAction = useMemo((): "ai_defend" | "auto_pass" | "ai_turn" | "none" => {
    if (state.winner) return "none";
    if (state.pendingAttack && state.currentPlayer === humanPlayer) return "ai_defend";
    if (state.pendingAttack && state.currentPlayer === aiPlayer) {
      const humanActions = getValidActions(state, humanPlayer);
      const hasRealCounter = humanActions.some((a) => a.type !== "passCounter");
      return hasRealCounter ? "none" : "auto_pass";
    }
    if (state.currentPlayer === aiPlayer && !state.pendingAttack) return "ai_turn";
    return "none";
  }, [state, humanPlayer, aiPlayer]);

  useEffect(() => {
    if (needsAutoAction === "none") return;

    const pa = state.pendingAttack;
    const attackPause = pa ? (pa.isSpecial || pa.attackerId.startsWith("captain_") ? 1000 : 750) : 0;
    const baseDelay = needsAutoAction === "ai_turn" ? 650 : (attackPause || 700);
    const wait = Math.max(baseDelay, busyUntilRef.current - Date.now());

    const timer = setTimeout(() => {
      const cur = stateRef.current;
      if (cur.winner) return;

      // Choose the action from the current state.
      let action: GameAction;
      if (needsAutoAction === "auto_pass") {
        if (!cur.pendingAttack) return;
        action = { type: "passCounter" };
      } else if (needsAutoAction === "ai_defend") {
        if (!cur.pendingAttack) return;
        try { action = aiChooseAction(cur, aiPlayer, difficulty); }
        catch { action = { type: "passCounter" }; }
      } else { // ai_turn
        if (cur.currentPlayer !== aiPlayer || cur.pendingAttack) return;
        try { action = aiChooseAction(cur, aiPlayer, difficulty); }
        catch { action = { type: "endTurn" }; }
      }

      announce(action, cur);
      updateState((prev) => {
        try { return executeAction(prev, action); }
        catch {
          try { return executeAction(prev, prev.pendingAttack ? { type: "passCounter" } : { type: "endTurn" }); }
          catch { return prev; }
        }
      });
    }, wait);

    return () => clearTimeout(timer);
  }, [needsAutoAction, stateVersion, aiPlayer, difficulty, updateState, announce, state.pendingAttack]);

  const validActions = useMemo(() => {
    if (state.winner) return [];
    if (state.pendingAttack && state.currentPlayer === aiPlayer) return getValidActions(state, humanPlayer);
    if (!state.pendingAttack && state.currentPlayer === humanPlayer) return getValidActions(state, humanPlayer);
    return [];
  }, [state, humanPlayer, aiPlayer]);

  const isAiTurn = state.currentPlayer === aiPlayer && !state.pendingAttack;

  return { state, validActions, dispatch, isAiTurn, humanPlayer, announcements, dismissAnnouncement };
}
