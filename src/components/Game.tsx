"use client";

import { useState, useMemo } from "react";
import type { GameAction, Slot, PlayerId, DeckDef } from "@/types";
import { useGameEngine } from "@/hooks/useGameEngine";
import { getCardDef, getCaptainDef } from "@/engine/cardRegistry";
import { initializeRegistry } from "@/engine/init";
import BoardSlot from "./BoardSlot";
import Card from "./Card";
import CaptainCard from "./CaptainCard";
import CardDetail from "./CardDetail";
import ActionMenu from "./ActionMenu";
import EventConfirm from "./EventConfirm";
import { FRONT_SLOTS, BACK_SLOTS } from "@/engine/utils";

initializeRegistry();

interface GameProps {
  playerDeck: DeckDef;
  aiDeck: DeckDef;
}

type UIMode =
  | { type: "idle" }
  | { type: "selectingSlot"; cardId: string }
  | { type: "selectingTarget"; attackerId: string; isSpecial: boolean }
  | { type: "selectingEquipTarget"; objectId: string }
  | { type: "actionMenu"; instanceId: string }
  | { type: "cardDetail"; defId: string; instanceId?: string }
  | { type: "confirmEvent"; instanceId: string }
  | { type: "confirmShip"; instanceId: string };

export default function Game({ playerDeck, aiDeck }: GameProps) {
  const { state, validActions, dispatch, isAiTurn, humanPlayer } =
    useGameEngine(playerDeck, aiDeck);
  const [uiMode, setUiMode] = useState<UIMode>({ type: "idle" });
  const [selectedHandCard, setSelectedHandCard] = useState<string | null>(null);

  const aiPlayer: PlayerId = humanPlayer === "player1" ? "player2" : "player1";
  const player = state.players[humanPlayer];
  const opponent = state.players[aiPlayer];
  const inCounterWindow = state.pendingAttack !== null;

  // Deploy slots
  const deploySlots = useMemo(() => {
    if (uiMode.type !== "selectingSlot") return new Set<Slot>();
    const slots = new Set<Slot>();
    for (const a of validActions) {
      if (a.type === "deployCharacter" && a.instanceId === uiMode.cardId) slots.add(a.slot);
    }
    return slots;
  }, [uiMode, validActions]);

  // Equip targets
  const equipTargets = useMemo(() => {
    if (uiMode.type !== "selectingEquipTarget") return new Set<string>();
    const targets = new Set<string>();
    for (const a of validActions) {
      if (a.type === "equipObject" && "objectInstanceId" in a && a.objectInstanceId === uiMode.objectId) {
        targets.add(a.targetInstanceId);
      }
    }
    return targets;
  }, [uiMode, validActions]);

  // Attack targets
  const attackTargets = useMemo(() => {
    if (uiMode.type !== "selectingTarget") return new Set<string>();
    const targets = new Set<string>();
    const actionType = uiMode.isSpecial ? "specialAttack" : "baseAttack";
    for (const a of validActions) {
      if (a.type === actionType && "attackerInstanceId" in a && a.attackerInstanceId === uiMode.attackerId) {
        if ("targetIsCaptain" in a && a.targetIsCaptain) targets.add(`captain_${aiPlayer}`);
        else if ("targetInstanceId" in a) targets.add(a.targetInstanceId);
      }
    }
    return targets;
  }, [uiMode, validActions, aiPlayer]);

  // --- Handlers ---
  const handleHandCardClick = (instanceId: string) => {
    const card = state.cards[instanceId];
    if (!card) return;
    const def = getCardDef(card.defId);

    if (def.type === "character") {
      setSelectedHandCard(instanceId);
      setUiMode({ type: "selectingSlot", cardId: instanceId });
    } else if (def.type === "object") {
      // Select equipment target among board characters
      setSelectedHandCard(instanceId);
      setUiMode({ type: "selectingEquipTarget", objectId: instanceId });
    } else if (def.type === "event") {
      setSelectedHandCard(instanceId);
      setUiMode({ type: "confirmEvent", instanceId });
    } else if (def.type === "ship") {
      setSelectedHandCard(instanceId);
      setUiMode({ type: "confirmShip", instanceId });
    }
  };

  const resetUI = () => {
    setUiMode({ type: "idle" });
    setSelectedHandCard(null);
  };

  const handleSlotClick = (slot: Slot, isPlayerSide: boolean) => {
    if (uiMode.type === "selectingSlot" && isPlayerSide && deploySlots.has(slot)) {
      dispatch({ type: "deployCharacter", instanceId: uiMode.cardId, slot });
      resetUI();
    } else if (uiMode.type === "selectingTarget" && !isPlayerSide) {
      const targetId = opponent.board[slot];
      if (targetId && attackTargets.has(targetId)) {
        dispatch({
          type: uiMode.isSpecial ? "specialAttack" : "baseAttack",
          attackerInstanceId: uiMode.attackerId,
          targetInstanceId: targetId,
        } as GameAction);
        resetUI();
      }
    }
  };

  const handleBoardCharClick = (instanceId: string, isPlayerSide: boolean) => {
    // Equip target selection
    if (uiMode.type === "selectingEquipTarget" && isPlayerSide && equipTargets.has(instanceId)) {
      dispatch({ type: "equipObject", objectInstanceId: uiMode.objectId, targetInstanceId: instanceId });
      resetUI();
      return;
    }
    // Attack target selection
    if (uiMode.type === "selectingTarget" && !isPlayerSide && attackTargets.has(instanceId)) {
      dispatch({
        type: uiMode.isSpecial ? "specialAttack" : "baseAttack",
        attackerInstanceId: uiMode.attackerId,
        targetInstanceId: instanceId,
      } as GameAction);
      resetUI();
      return;
    }
    // Own character → action menu
    if (isPlayerSide) {
      setUiMode({ type: "actionMenu", instanceId });
    } else {
      // Enemy → view detail
      const card = state.cards[instanceId];
      if (card) setUiMode({ type: "cardDetail", defId: card.defId, instanceId });
    }
  };

  const handleCaptainClick = (playerId: PlayerId) => {
    if (uiMode.type === "selectingTarget" && playerId === aiPlayer && attackTargets.has(`captain_${aiPlayer}`)) {
      dispatch({
        type: uiMode.isSpecial ? "specialAttack" : "baseAttack",
        attackerInstanceId: uiMode.attackerId,
        targetInstanceId: `captain_${aiPlayer}`,
        targetIsCaptain: true,
      } as GameAction);
      resetUI();
    }
  };

  // Board row render
  const renderRow = (slots: readonly string[], playerId: PlayerId) => {
    const isPlayerSide = playerId === humanPlayer;
    const ps = state.players[playerId];
    // Check if captain verso is in one of these slots
    const captainSlot = ps.captain.flipped ? ps.captain.slot : null;

    return (
      <div className="flex gap-3 justify-center">
        {slots.map((s) => {
          const slot = s as Slot;

          // Captain verso in this slot?
          if (captainSlot === slot) {
            const capDef = getCaptainDef(ps.captain.defId);
            const isTarget = !isPlayerSide && uiMode.type === "selectingTarget" && attackTargets.has(`captain_${playerId}`);
            return (
              <div
                key={slot}
                onClick={() => {
                  if (isTarget) handleCaptainClick(playerId);
                }}
                className={`w-36 h-48 rounded-lg border-2 flex items-center justify-center transition-all cursor-pointer
                  border-red-600 bg-red-950/20
                  ${isTarget ? "ring-2 ring-red-400 animate-pulse" : ""}
                `}
              >
                <div className="text-center p-1">
                  <div className="text-[11px] font-bold text-red-300">{capDef.name}</div>
                  <div className="text-[10px] text-red-400">VERSO</div>
                  <div className="text-xs mt-1">
                    <span className="text-red-400">⚔{capDef.verso.atk}</span>{" "}
                    <span className="text-blue-400">🛡{capDef.verso.def}</span>
                  </div>
                  <div className={`text-xs font-bold mt-0.5 ${ps.captain.currentPv <= capDef.verso.pv / 3 ? "text-red-400" : "text-green-400"}`}>
                    ❤{ps.captain.currentPv}/{capDef.verso.pv}
                  </div>
                </div>
              </div>
            );
          }
          const charId = ps.board[slot];
          const instance = charId ? state.cards[charId] : null;
          const def = instance ? getCardDef(instance.defId) : null;
          const isValidDeploy = isPlayerSide && deploySlots.has(slot);
          const isValidTarget = !isPlayerSide && uiMode.type === "selectingTarget" && charId !== null && attackTargets.has(charId!);
          const isEquipTarget = isPlayerSide && uiMode.type === "selectingEquipTarget" && charId !== null && equipTargets.has(charId!);

          return (
            <BoardSlot
              key={slot}
              slot={slot}
              instance={instance}
              def={def}
              isPlayerSide={isPlayerSide}
              isValidTarget={isValidTarget}
              isValidDeploy={isValidDeploy || isEquipTarget}
              onClick={() => {
                if (isValidDeploy) handleSlotClick(slot, isPlayerSide);
                else if (charId) handleBoardCharClick(charId, isPlayerSide);
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
    if (counterActions.length === 0) return null;

    return (
      <div className="fixed inset-0 bg-black/70 flex items-center justify-center z-50">
        <div className="bg-gray-900 rounded-xl p-6 border-2 border-red-500 max-w-lg">
          <h3 className="text-xl font-bold text-red-400 mb-3">Attaque entrante !</h3>
          <p className="text-base text-yellow-300 mb-2 font-semibold">
            {pending.attackerId.startsWith("captain_")
              ? getCaptainDef(state.players[pending.attackerId.replace("captain_", "") as PlayerId].captain.defId).name
              : getCardDef(state.cards[pending.attackerId].defId).name}
            {" "}attaque{" "}
            {pending.targetIsCaptain
              ? "votre Capitaine"
              : getCardDef(state.cards[pending.targetId].defId).name}
          </p>
          <p className="text-base text-gray-300 mb-4">
            Degats : <span className="text-red-300 font-bold text-2xl">{pending.rawDamage}</span>
            {pending.element && <span className="ml-2 text-gray-400">({pending.element})</span>}
            {pending.hasHaki && <span className="ml-2 text-purple-300">[Haki]</span>}
          </p>
          <div className="flex gap-3 flex-wrap">
            {counterActions.map((action, i) => {
              if (action.type === "playCounter") {
                const card = state.cards[action.instanceId];
                const def = getCardDef(card.defId);
                return (
                  <button key={i} onClick={() => dispatch(action)}
                    className="px-5 py-3 bg-blue-700 hover:bg-blue-600 rounded-lg text-base font-semibold">
                    🛡 {def.name} ({def.cost} Vol.)
                  </button>
                );
              }
              if (action.type === "useHaki") {
                return (
                  <button key={i} onClick={() => dispatch(action)}
                    className="px-5 py-3 bg-purple-700 hover:bg-purple-600 rounded-lg text-base font-semibold">
                    👁 Haki Observation
                  </button>
                );
              }
              if (action.type === "passCounter") {
                return (
                  <button key={i} onClick={() => dispatch(action)}
                    className="px-5 py-3 bg-gray-700 hover:bg-gray-600 rounded-lg text-base">
                    Subir les degats
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

  // Game over
  if (state.winner) {
    const won = state.winner === humanPlayer;
    return (
      <div className="flex flex-col items-center justify-center min-h-screen gap-6">
        <h1 className={`text-5xl font-bold ${won ? "text-green-400" : "text-red-400"}`}>
          {won ? "VICTOIRE !" : "DEFAITE..."}
        </h1>
        <p className="text-gray-400 text-xl">Tour {state.turnNumber}</p>
        <button onClick={() => window.location.reload()}
          className="px-8 py-4 bg-amber-600 hover:bg-amber-500 rounded-lg text-xl font-semibold">
          Rejouer
        </button>
      </div>
    );
  }

  const playerCaptainDef = getCaptainDef(player.captain.defId);
  const opponentCaptainDef = getCaptainDef(opponent.captain.defId);
  const canFlip = validActions.some((a) => a.type === "flipCaptain");

  // Status bar text
  const statusText = (() => {
    if (isAiTurn) return { text: "Tour de l'IA...", color: "text-yellow-400", pulse: true };
    if (inCounterWindow) return { text: "⚠ Reaction !", color: "text-red-400", pulse: false };
    if (uiMode.type === "selectingTarget") return { text: "Choisissez une cible ennemie", color: "text-red-300", pulse: true };
    if (uiMode.type === "selectingSlot") return { text: "Choisissez un slot pour deployer", color: "text-green-300", pulse: true };
    if (uiMode.type === "selectingEquipTarget") return { text: "Choisissez un personnage a equiper", color: "text-amber-300", pulse: true };
    return { text: "Votre tour", color: "text-green-400", pulse: false };
  })();

  return (
    <div className="min-h-screen flex flex-col p-3 gap-3">
      {/* Header */}
      <div className="flex justify-between items-center px-5 py-3 bg-gray-900 rounded-lg">
        <div>
          <span className="text-gray-400 text-base">Tour</span>{" "}
          <span className="font-bold text-2xl">{state.turnNumber}</span>
        </div>
        <div>
          <span className="text-blue-400 font-bold text-xl">⭐ {player.volonte} Vol.</span>
        </div>
        <div className={`text-base font-semibold ${statusText.color} ${statusText.pulse ? "animate-pulse" : ""}`}>
          {statusText.text}
        </div>
      </div>

      {/* Opponent area */}
      <div className="flex justify-center gap-5 items-start">
        <div
          onClick={() => handleCaptainClick(aiPlayer)}
          className={uiMode.type === "selectingTarget" && attackTargets.has(`captain_${aiPlayer}`) ? "ring-2 ring-red-500 rounded-xl cursor-pointer" : ""}
        >
          <CaptainCard captain={opponent.captain} def={opponentCaptainDef} isOpponent />
        </div>
        <div className="flex flex-col gap-3">
          {renderRow(BACK_SLOTS, aiPlayer)}
          {renderRow(FRONT_SLOTS, aiPlayer)}
        </div>
        <div className="text-sm text-gray-500 min-w-[90px]">
          <p>Main: {opponent.hand.length}</p>
          <p>Deck: {opponent.deck.length}</p>
          {opponent.activeShip && (
            <p className="text-blue-300">🚢 {getCardDef(state.cards[opponent.activeShip].defId).name}</p>
          )}
        </div>
      </div>

      <div className="border-t border-gray-700" />

      {/* Player area */}
      <div className="flex justify-center gap-5 items-start">
        <CaptainCard captain={player.captain} def={playerCaptainDef} />
        <div className="flex flex-col gap-3">
          {renderRow(FRONT_SLOTS, humanPlayer)}
          {renderRow(BACK_SLOTS, humanPlayer)}
        </div>
        <div className="flex flex-col gap-2 min-w-[130px]">
          <p className="text-sm text-gray-500">Deck: {player.deck.length}</p>
          {player.activeShip && (
            <p className="text-sm text-blue-300">🚢 {getCardDef(state.cards[player.activeShip].defId).name}</p>
          )}
          {canFlip && (
            <button
              onClick={() => {
                const slot = Object.entries(player.board).find(([, v]) => v === null)?.[0] as Slot | undefined;
                if (slot) dispatch({ type: "flipCaptain", slot });
              }}
              className="px-4 py-2 bg-red-700 hover:bg-red-600 rounded text-sm font-semibold"
            >
              ⚔ Engager Capitaine
            </button>
          )}
          <button
            onClick={() => { dispatch({ type: "endTurn" }); resetUI(); }}
            disabled={isAiTurn || inCounterWindow}
            className="px-4 py-3 bg-amber-700 hover:bg-amber-600 disabled:opacity-50 rounded text-base font-semibold"
          >
            Fin de tour ➡
          </button>
          {uiMode.type !== "idle" && (
            <button onClick={resetUI} className="px-3 py-2 bg-gray-700 hover:bg-gray-600 rounded text-sm">
              ✕ Annuler
            </button>
          )}
        </div>
      </div>

      {/* Hand */}
      <div className="mt-1">
        <div className="text-sm text-gray-500 mb-1 px-4">Main ({player.hand.length} cartes)</div>
        <div className="flex gap-3 overflow-x-auto px-4 pb-3">
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
              <div key={id} className="flex-shrink-0">
                <Card
                  instance={card}
                  def={def}
                  selected={selectedHandCard === id}
                  highlight={canPlay}
                  onClick={() => {
                    if (canPlay) handleHandCardClick(id);
                    else setUiMode({ type: "cardDetail", defId: card.defId, instanceId: id });
                  }}
                />
              </div>
            );
          })}
        </div>
      </div>

      {/* Log */}
      <div className="bg-gray-900 rounded-lg p-3 max-h-40 overflow-y-auto text-sm text-gray-400">
        {state.log.slice(-15).reverse().map((entry, i) => (
          <div key={i} className="py-0.5">
            <span className="text-gray-600">T{entry.turn}</span>{" "}
            <span className={entry.player === humanPlayer ? "text-green-400" : "text-red-400"}>
              {entry.player === humanPlayer ? "►" : "◄"}
            </span>{" "}
            {entry.message}
          </div>
        ))}
      </div>

      {/* Overlays */}
      {renderCounterWindow()}

      {uiMode.type === "actionMenu" && (() => {
        const inst = state.cards[uiMode.instanceId];
        if (!inst) return null;
        const def = getCardDef(inst.defId);
        return (
          <ActionMenu
            instance={inst} def={def} state={state} validActions={validActions}
            onBaseAttack={() => setUiMode({ type: "selectingTarget", attackerId: uiMode.instanceId, isSpecial: false })}
            onSpecialAttack={() => setUiMode({ type: "selectingTarget", attackerId: uiMode.instanceId, isSpecial: true })}
            onViewDetail={() => setUiMode({ type: "cardDetail", defId: inst.defId, instanceId: uiMode.instanceId })}
            onClose={resetUI}
          />
        );
      })()}

      {uiMode.type === "cardDetail" && (() => {
        const def = getCardDef(uiMode.defId);
        const inst = uiMode.instanceId ? state.cards[uiMode.instanceId] : undefined;
        return <CardDetail def={def} instance={inst} state={state} onClose={resetUI} />;
      })()}

      {/* Event confirmation */}
      {(uiMode.type === "confirmEvent" || uiMode.type === "confirmShip") && (() => {
        const card = state.cards[uiMode.instanceId];
        if (!card) return null;
        const def = getCardDef(card.defId);
        return (
          <EventConfirm
            def={def}
            playerVol={player.volonte}
            onConfirm={() => {
              if (uiMode.type === "confirmEvent") {
                dispatch({ type: "playEvent", instanceId: uiMode.instanceId });
              } else {
                dispatch({ type: "deployShip", instanceId: uiMode.instanceId });
              }
              resetUI();
            }}
            onCancel={resetUI}
          />
        );
      })()}
    </div>
  );
}
