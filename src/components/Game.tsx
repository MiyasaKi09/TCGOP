"use client";

import { useState, useMemo } from "react";
import type { GameAction, Slot, PlayerId, DeckDef } from "@/types";
import { useGameEngine } from "@/hooks/useGameEngine";
import { getCardDef, getCaptainDef } from "@/engine/cardRegistry";
import { initializeRegistry } from "@/engine/init";
import BoardSlot from "./BoardSlot";
import Card from "./Card";
import CaptainCard from "./CaptainCard";
import { ALL_SLOTS, FRONT_SLOTS, BACK_SLOTS } from "@/engine/utils";
import { getEffectiveAtk, getEffectiveDef } from "@/engine/board";

// Ensure registry is initialized
initializeRegistry();

interface GameProps {
  playerDeck: DeckDef;
  aiDeck: DeckDef;
}

type UIMode =
  | { type: "idle" }
  | { type: "selectingSlot"; cardId: string }
  | { type: "selectingTarget"; attackerId: string; isSpecial: boolean }
  | { type: "counterWindow" };

export default function Game({ playerDeck, aiDeck }: GameProps) {
  const { state, validActions, dispatch, isAiTurn, humanPlayer } =
    useGameEngine(playerDeck, aiDeck);
  const [uiMode, setUiMode] = useState<UIMode>({ type: "idle" });
  const [selectedHandCard, setSelectedHandCard] = useState<string | null>(null);

  const aiPlayer: PlayerId = humanPlayer === "player1" ? "player2" : "player1";
  const player = state.players[humanPlayer];
  const opponent = state.players[aiPlayer];

  // Determine if we're in counter window
  const inCounterWindow = state.pendingAttack !== null;

  // Get deploy slots (for selected hand card)
  const deploySlots = useMemo(() => {
    if (!selectedHandCard) return new Set<Slot>();
    const slots = new Set<Slot>();
    for (const action of validActions) {
      if (
        action.type === "deployCharacter" &&
        action.instanceId === selectedHandCard
      ) {
        slots.add(action.slot);
      }
    }
    return slots;
  }, [selectedHandCard, validActions]);

  // Get valid attack targets
  const attackTargets = useMemo(() => {
    if (uiMode.type !== "selectingTarget") return new Set<string>();
    const targets = new Set<string>();
    const actionType = uiMode.isSpecial ? "specialAttack" : "baseAttack";
    for (const action of validActions) {
      if (
        action.type === actionType &&
        "attackerInstanceId" in action &&
        action.attackerInstanceId === uiMode.attackerId
      ) {
        if ("targetIsCaptain" in action && action.targetIsCaptain) {
          targets.add(`captain_${aiPlayer}`);
        } else if ("targetInstanceId" in action) {
          targets.add(action.targetInstanceId);
        }
      }
    }
    return targets;
  }, [uiMode, validActions, aiPlayer]);

  // Handle hand card click
  const handleHandCardClick = (instanceId: string) => {
    const card = state.cards[instanceId];
    if (!card) return;
    const def = getCardDef(card.defId);

    if (def.type === "character") {
      setSelectedHandCard(instanceId);
      setUiMode({ type: "selectingSlot", cardId: instanceId });
    } else if (def.type === "event") {
      dispatch({ type: "playEvent", instanceId });
      setUiMode({ type: "idle" });
    } else if (def.type === "ship") {
      dispatch({ type: "deployShip", instanceId });
      setUiMode({ type: "idle" });
    } else if (def.type === "object") {
      // For objects, equip on the strongest board character
      const boardChars = Object.values(player.board).filter(Boolean) as string[];
      if (boardChars.length > 0) {
        dispatch({
          type: "equipObject",
          objectInstanceId: instanceId,
          targetInstanceId: boardChars[0],
        });
      }
      setUiMode({ type: "idle" });
    }
    setSelectedHandCard(instanceId);
  };

  // Handle slot click (deploy or attack target)
  const handleSlotClick = (slot: Slot, isPlayerSide: boolean) => {
    if (uiMode.type === "selectingSlot" && isPlayerSide) {
      // Deploy character
      dispatch({
        type: "deployCharacter",
        instanceId: uiMode.cardId,
        slot,
      });
      setUiMode({ type: "idle" });
      setSelectedHandCard(null);
    } else if (uiMode.type === "selectingTarget" && !isPlayerSide) {
      // Attack target
      const targetId = opponent.board[slot];
      if (targetId && attackTargets.has(targetId)) {
        dispatch({
          type: uiMode.isSpecial ? "specialAttack" : "baseAttack",
          attackerInstanceId: uiMode.attackerId,
          targetInstanceId: targetId,
        } as GameAction);
        setUiMode({ type: "idle" });
      }
    }
  };

  // Handle board character click (select attacker)
  const handleBoardCharClick = (instanceId: string) => {
    const card = state.cards[instanceId];
    if (!card || card.owner !== humanPlayer) return;

    // Check if this character can attack
    const canBase = validActions.some(
      (a) =>
        a.type === "baseAttack" &&
        "attackerInstanceId" in a &&
        a.attackerInstanceId === instanceId
    );
    const canSpecial = validActions.some(
      (a) =>
        a.type === "specialAttack" &&
        "attackerInstanceId" in a &&
        a.attackerInstanceId === instanceId
    );

    if (canBase) {
      setUiMode({ type: "selectingTarget", attackerId: instanceId, isSpecial: false });
    } else if (canSpecial) {
      setUiMode({ type: "selectingTarget", attackerId: instanceId, isSpecial: true });
    }
  };

  // Handle captain click (for targeting)
  const handleCaptainClick = (playerId: PlayerId) => {
    if (uiMode.type === "selectingTarget" && playerId === aiPlayer) {
      if (attackTargets.has(`captain_${aiPlayer}`)) {
        dispatch({
          type: uiMode.isSpecial ? "specialAttack" : "baseAttack",
          attackerInstanceId: uiMode.attackerId,
          targetInstanceId: `captain_${aiPlayer}`,
          targetIsCaptain: true,
        } as GameAction);
        setUiMode({ type: "idle" });
      }
    }
  };

  // Render a row of slots
  const renderRow = (slots: readonly string[], playerId: PlayerId) => {
    const isPlayerSide = playerId === humanPlayer;
    const playerState = state.players[playerId];

    return (
      <div className="flex gap-2 justify-center">
        {slots.map((s) => {
          const slot = s as Slot;
          const charId = playerState.board[slot];
          const instance = charId ? state.cards[charId] : null;
          const def = instance ? getCardDef(instance.defId) : null;

          const isValidDeploy = isPlayerSide && deploySlots.has(slot);
          const isValidTarget =
            !isPlayerSide &&
            uiMode.type === "selectingTarget" &&
            charId !== null &&
            attackTargets.has(charId!);

          return (
            <BoardSlot
              key={slot}
              slot={slot}
              instance={instance}
              def={def}
              isPlayerSide={isPlayerSide}
              isValidTarget={isValidTarget}
              isValidDeploy={isValidDeploy}
              onClick={() => {
                if (isValidDeploy || isValidTarget) {
                  handleSlotClick(slot, isPlayerSide);
                } else if (isPlayerSide && charId) {
                  handleBoardCharClick(charId);
                }
              }}
            />
          );
        })}
      </div>
    );
  };

  // Counter window
  const renderCounterWindow = () => {
    if (!inCounterWindow) return null;
    const pending = state.pendingAttack!;
    const counterActions = validActions.filter(
      (a) => a.type === "playCounter" || a.type === "passCounter" || (a.type === "useHaki" && a.hakiType === "observation")
    );

    if (counterActions.length === 0) {
      // AI is attacking, auto-resolve
      return null;
    }

    return (
      <div className="fixed inset-0 bg-black/70 flex items-center justify-center z-50">
        <div className="bg-gray-900 rounded-xl p-6 border border-red-500 max-w-lg">
          <h3 className="text-lg font-bold text-red-400 mb-2">Attaque entrante !</h3>
          <p className="text-sm text-gray-300 mb-4">
            Degats : <span className="text-red-300 font-bold">{pending.rawDamage}</span>
            {pending.element && <span className="ml-2">({pending.element})</span>}
          </p>

          <div className="flex gap-3 flex-wrap">
            {counterActions.map((action, i) => {
              if (action.type === "playCounter") {
                const card = state.cards[action.instanceId];
                const def = getCardDef(card.defId);
                return (
                  <button
                    key={i}
                    onClick={() => dispatch(action)}
                    className="px-4 py-2 bg-blue-700 hover:bg-blue-600 rounded-lg text-sm"
                  >
                    {def.name} (cout {def.cost})
                  </button>
                );
              }
              if (action.type === "useHaki") {
                return (
                  <button
                    key={i}
                    onClick={() => dispatch(action)}
                    className="px-4 py-2 bg-purple-700 hover:bg-purple-600 rounded-lg text-sm"
                  >
                    Haki Observation (esquiver)
                  </button>
                );
              }
              if (action.type === "passCounter") {
                return (
                  <button
                    key={i}
                    onClick={() => dispatch(action)}
                    className="px-4 py-2 bg-gray-700 hover:bg-gray-600 rounded-lg text-sm"
                  >
                    Passer
                  </button>
                );
              }
              return null;
            })}
          </div>
        </div>
      </div>
    );
  };

  // Game over screen
  if (state.winner) {
    const won = state.winner === humanPlayer;
    return (
      <div className="flex flex-col items-center justify-center min-h-screen gap-4">
        <h1 className={`text-4xl font-bold ${won ? "text-green-400" : "text-red-400"}`}>
          {won ? "VICTOIRE !" : "DEFAITE..."}
        </h1>
        <p className="text-gray-400">Tour {state.turnNumber}</p>
        <button
          onClick={() => window.location.reload()}
          className="px-6 py-3 bg-amber-600 hover:bg-amber-500 rounded-lg"
        >
          Rejouer
        </button>
      </div>
    );
  }

  const playerCaptainDef = getCaptainDef(player.captain.defId);
  const opponentCaptainDef = getCaptainDef(opponent.captain.defId);

  // Can flip captain?
  const canFlip = validActions.some((a) => a.type === "flipCaptain");

  return (
    <div className="min-h-screen flex flex-col p-2 gap-2">
      {/* Header */}
      <div className="flex justify-between items-center px-4 py-2 bg-gray-900 rounded-lg">
        <div className="text-sm">
          <span className="text-gray-400">Tour</span>{" "}
          <span className="font-bold text-lg">{state.turnNumber}</span>
        </div>
        <div className="text-sm">
          <span className="text-blue-400 font-bold">
            Volonte: {player.volonte}
          </span>
        </div>
        <div className="text-sm">
          {isAiTurn ? (
            <span className="text-yellow-400 animate-pulse">Tour de l&apos;IA...</span>
          ) : inCounterWindow ? (
            <span className="text-red-400">Reaction !</span>
          ) : (
            <span className="text-green-400">Votre tour</span>
          )}
        </div>
      </div>

      {/* Opponent area */}
      <div className="flex justify-center gap-4 items-start">
        <div onClick={() => handleCaptainClick(aiPlayer)}>
          <CaptainCard
            captain={opponent.captain}
            def={opponentCaptainDef}
            isOpponent
          />
        </div>
        <div className="flex flex-col gap-2">
          {renderRow(BACK_SLOTS, aiPlayer)}
          {renderRow(FRONT_SLOTS, aiPlayer)}
        </div>
        <div className="text-xs text-gray-500">
          <p>Main: {opponent.hand.length}</p>
          <p>Deck: {opponent.deck.length}</p>
          {opponent.activeShip && (
            <p className="text-blue-300">
              🚢 {getCardDef(state.cards[opponent.activeShip].defId).name}
            </p>
          )}
        </div>
      </div>

      {/* Divider */}
      <div className="border-t border-gray-700 my-1" />

      {/* Player area */}
      <div className="flex justify-center gap-4 items-start">
        <CaptainCard captain={player.captain} def={playerCaptainDef} />
        <div className="flex flex-col gap-2">
          {renderRow(FRONT_SLOTS, humanPlayer)}
          {renderRow(BACK_SLOTS, humanPlayer)}
        </div>
        <div className="flex flex-col gap-2 text-xs">
          <p className="text-gray-500">Deck: {player.deck.length}</p>
          {player.activeShip && (
            <p className="text-blue-300">
              🚢 {getCardDef(state.cards[player.activeShip].defId).name}
            </p>
          )}
          {canFlip && (
            <button
              onClick={() => {
                const slot = Object.entries(player.board).find(
                  ([, v]) => v === null
                )?.[0] as Slot | undefined;
                if (slot) dispatch({ type: "flipCaptain", slot });
              }}
              className="px-3 py-1 bg-red-700 hover:bg-red-600 rounded text-xs"
            >
              Engager Capitaine
            </button>
          )}
          <button
            onClick={() => {
              dispatch({ type: "endTurn" });
              setUiMode({ type: "idle" });
              setSelectedHandCard(null);
            }}
            disabled={isAiTurn || inCounterWindow}
            className="px-3 py-2 bg-amber-700 hover:bg-amber-600 disabled:opacity-50 rounded text-sm font-semibold"
          >
            Fin de tour
          </button>
          {uiMode.type !== "idle" && (
            <button
              onClick={() => {
                setUiMode({ type: "idle" });
                setSelectedHandCard(null);
              }}
              className="px-3 py-1 bg-gray-700 hover:bg-gray-600 rounded text-xs"
            >
              Annuler
            </button>
          )}
        </div>
      </div>

      {/* Hand */}
      <div className="mt-2">
        <div className="text-xs text-gray-500 mb-1 px-4">
          Main ({player.hand.length} cartes)
        </div>
        <div className="flex gap-2 overflow-x-auto px-4 pb-2">
          {player.hand.map((id) => {
            const card = state.cards[id];
            if (!card) return null;
            const def = getCardDef(card.defId);
            const canPlay = validActions.some((a) => {
              if ("instanceId" in a && a.instanceId === id) return true;
              if ("objectInstanceId" in a && a.objectInstanceId === id) return true;
              return false;
            });
            return (
              <Card
                key={id}
                instance={card}
                def={def}
                selected={selectedHandCard === id}
                highlight={canPlay}
                onClick={() => canPlay && handleHandCardClick(id)}
              />
            );
          })}
        </div>
      </div>

      {/* Log */}
      <div className="bg-gray-900 rounded-lg p-2 max-h-32 overflow-y-auto text-xs text-gray-400">
        {state.log
          .slice(-10)
          .reverse()
          .map((entry, i) => (
            <div key={i} className="py-0.5">
              <span className="text-gray-600">T{entry.turn}</span>{" "}
              {entry.message}
            </div>
          ))}
      </div>

      {/* Counter window overlay */}
      {renderCounterWindow()}
    </div>
  );
}
