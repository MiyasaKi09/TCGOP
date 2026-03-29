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
    if (uiMode.type === "selectingEquipTarget" && isPlayerSide && equipTargets.has(instanceId)) {
      dispatch({ type: "equipObject", objectInstanceId: uiMode.objectId, targetInstanceId: instanceId });
      resetUI();
      return;
    }
    if (uiMode.type === "selectingTarget" && !isPlayerSide && attackTargets.has(instanceId)) {
      dispatch({
        type: uiMode.isSpecial ? "specialAttack" : "baseAttack",
        attackerInstanceId: uiMode.attackerId,
        targetInstanceId: instanceId,
      } as GameAction);
      resetUI();
      return;
    }
    if (isPlayerSide) {
      setUiMode({ type: "actionMenu", instanceId });
    } else {
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
    const captainSlot = ps.captain.flipped ? ps.captain.slot : null;

    return (
      <div className="flex gap-2 justify-center">
        {slots.map((s) => {
          const slot = s as Slot;

          // Captain verso in this slot
          if (captainSlot === slot) {
            const capDef = getCaptainDef(ps.captain.defId);
            const isTarget = !isPlayerSide && uiMode.type === "selectingTarget" && attackTargets.has(`captain_${playerId}`);
            const pvPercent = Math.max(0, (ps.captain.currentPv / capDef.verso.pv) * 100);
            return (
              <div
                key={slot}
                onClick={() => { if (isTarget) handleCaptainClick(playerId); }}
                className={`
                  w-[9.5rem] h-52 rounded-xl border-2 flex items-center justify-center transition-all duration-200 cursor-pointer
                  border-red-500/70 bg-gradient-to-b from-red-950/30 to-gray-900/60 rarity-glow-CAP
                  ${isTarget ? "ring-2 ring-red-400/80 shadow-lg shadow-red-500/30 animate-pulse" : "hover:brightness-110"}
                `}
              >
                <div className="text-center p-2 w-full">
                  <div className="text-[10px] uppercase tracking-widest text-red-400/80 font-bold mb-1">Verso</div>
                  <div className="text-[12px] font-bold text-white mb-1.5">{capDef.name}</div>
                  <div className="flex justify-center gap-3 text-[11px] mb-1.5">
                    <span className="text-red-400 font-bold stat-badge">⚔{capDef.verso.atk}</span>
                    <span className="text-blue-400 font-bold stat-badge">🛡{capDef.verso.def}</span>
                  </div>
                  <div className="w-full h-1.5 rounded-full bg-gray-800/80 overflow-hidden mx-auto">
                    <div
                      className="h-full rounded-full health-bar"
                      style={{
                        width: `${pvPercent}%`,
                        ["--hp-color" as string]: pvPercent > 50 ? "#22c55e" : pvPercent > 25 ? "#eab308" : "#ef4444",
                        ["--hp-color-light" as string]: pvPercent > 50 ? "#4ade80" : pvPercent > 25 ? "#facc15" : "#f87171",
                      }}
                    />
                  </div>
                  <div className={`text-[10px] font-bold mt-1 stat-badge ${pvPercent <= 25 ? "text-red-400" : "text-green-400"}`}>
                    {ps.captain.currentPv}/{capDef.verso.pv}
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

    const attackerName = pending.attackerId.startsWith("captain_")
      ? getCaptainDef(state.players[pending.attackerId.replace("captain_", "") as PlayerId].captain.defId).name
      : getCardDef(state.cards[pending.attackerId].defId).name;
    const targetName = pending.targetIsCaptain
      ? "votre Capitaine"
      : getCardDef(state.cards[pending.targetId].defId).name;

    return (
      <div className="fixed inset-0 bg-black/80 backdrop-blur-sm flex items-center justify-center z-50">
        <div className="glass rounded-2xl p-6 border border-red-500/50 max-w-lg shadow-2xl shadow-red-900/20 animate-modal-enter">
          <div className="flex items-center gap-2 mb-4">
            <div className="w-2 h-2 rounded-full bg-red-500 animate-pulse" />
            <h3 className="text-lg font-bold text-red-400 uppercase tracking-wider">Attaque entrante</h3>
          </div>

          <div className="bg-gray-800/50 rounded-xl p-4 mb-4 border border-gray-700/30">
            <p className="text-sm text-gray-400 mb-1">
              <span className="text-red-300 font-bold">{attackerName}</span>
              {" "}attaque{" "}
              <span className="text-blue-300 font-bold">{targetName}</span>
            </p>
            <div className="flex items-baseline gap-3 mt-2">
              <span className="text-3xl font-black text-red-400 stat-badge">{pending.rawDamage}</span>
              <span className="text-sm text-gray-500">degats</span>
              {pending.element && (
                <span className="text-xs px-2 py-0.5 rounded-full bg-gray-700/50 text-gray-300 capitalize">{pending.element}</span>
              )}
              {pending.hasHaki && (
                <span className="text-xs px-2 py-0.5 rounded-full bg-purple-900/50 text-purple-300">Haki</span>
              )}
            </div>
          </div>

          <div className="flex gap-2 flex-wrap">
            {counterActions.map((action, i) => {
              if (action.type === "playCounter") {
                const card = state.cards[action.instanceId];
                const def = getCardDef(card.defId);
                return (
                  <button key={i} onClick={() => dispatch(action)}
                    className="action-btn px-4 py-2.5 bg-blue-600/80 hover:bg-blue-500/80 rounded-xl text-sm font-bold border border-blue-400/20 shadow-lg shadow-blue-900/20 transition-all">
                    🛡 {def.name} <span className="text-blue-200/70 text-xs">({def.cost}V)</span>
                  </button>
                );
              }
              if (action.type === "useHaki") {
                return (
                  <button key={i} onClick={() => dispatch(action)}
                    className="action-btn px-4 py-2.5 bg-purple-600/80 hover:bg-purple-500/80 rounded-xl text-sm font-bold border border-purple-400/20 shadow-lg shadow-purple-900/20 transition-all">
                    👁 Haki Observation
                  </button>
                );
              }
              if (action.type === "passCounter") {
                return (
                  <button key={i} onClick={() => dispatch(action)}
                    className="action-btn px-4 py-2.5 bg-gray-700/60 hover:bg-gray-600/60 rounded-xl text-sm border border-gray-600/20 transition-all text-gray-300">
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
      <div className="game-bg min-h-screen flex flex-col items-center justify-center gap-8">
        <div className={`text-6xl font-black tracking-tight ${won ? "text-green-400" : "text-red-400"}`}>
          {won ? "VICTOIRE !" : "DEFAITE..."}
        </div>
        <div className="text-gray-500 text-lg">Tour {state.turnNumber}</div>
        <button onClick={() => window.location.reload()}
          className="action-btn px-10 py-4 bg-gradient-to-r from-amber-600 to-amber-700 hover:from-amber-500 hover:to-amber-600 rounded-xl text-lg font-bold shadow-xl shadow-amber-900/30 border border-amber-500/20 transition-all">
          Rejouer
        </button>
      </div>
    );
  }

  const playerCaptainDef = getCaptainDef(player.captain.defId);
  const opponentCaptainDef = getCaptainDef(opponent.captain.defId);
  const canFlip = validActions.some((a) => a.type === "flipCaptain");
  const canActivateShip = validActions.some((a) => a.type === "activateShip");

  // Status bar
  const statusText = (() => {
    if (isAiTurn) return { text: "Tour de l'adversaire...", color: "text-yellow-400", pulse: true };
    if (inCounterWindow) return { text: "Reaction !", color: "text-red-400", pulse: true };
    if (uiMode.type === "selectingTarget") return { text: "Choisissez une cible", color: "text-red-300", pulse: true };
    if (uiMode.type === "selectingSlot") return { text: "Choisissez un slot", color: "text-green-300", pulse: true };
    if (uiMode.type === "selectingEquipTarget") return { text: "Equipez un personnage", color: "text-amber-300", pulse: true };
    return { text: "Votre tour", color: "text-green-400", pulse: false };
  })();

  return (
    <div className="game-bg min-h-screen flex flex-col p-4 gap-3">
      {/* Header */}
      <div className="flex justify-between items-center px-5 py-3 glass rounded-xl border border-gray-700/30">
        <div className="flex items-center gap-3">
          <div className="text-xs text-gray-500 uppercase tracking-widest">Tour</div>
          <div className="text-2xl font-black text-white stat-badge">{state.turnNumber}</div>
        </div>
        <div className="flex items-center gap-2">
          <div className="w-2 h-2 rounded-full bg-blue-500 shadow-lg shadow-blue-500/50" />
          <span className="text-blue-400 font-bold text-lg stat-badge">{player.volonte}</span>
          <span className="text-blue-400/60 text-sm">Volonte</span>
        </div>
        <div className={`text-sm font-semibold ${statusText.color} ${statusText.pulse ? "animate-pulse" : ""}`}>
          {statusText.text}
        </div>
      </div>

      {/* Opponent area */}
      <div className="flex justify-center gap-4 items-start">
        <div
          onClick={() => handleCaptainClick(aiPlayer)}
          className={`transition-all duration-200 ${uiMode.type === "selectingTarget" && attackTargets.has(`captain_${aiPlayer}`) ? "ring-2 ring-red-500/70 rounded-xl cursor-pointer shadow-lg shadow-red-500/20 animate-pulse" : ""}`}
        >
          <CaptainCard captain={opponent.captain} def={opponentCaptainDef} isOpponent />
        </div>
        <div className="flex flex-col gap-2">
          {renderRow(BACK_SLOTS, aiPlayer)}
          {renderRow(FRONT_SLOTS, aiPlayer)}
        </div>
        <div className="glass-light rounded-xl p-3 min-w-[100px] border border-gray-700/20">
          <div className="text-[10px] text-gray-500 uppercase tracking-widest mb-2">Adversaire</div>
          <div className="space-y-1.5 text-xs">
            <div className="flex justify-between">
              <span className="text-gray-500">Main</span>
              <span className="text-gray-300 font-bold stat-badge">{opponent.hand.length}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-gray-500">Deck</span>
              <span className="text-gray-300 font-bold stat-badge">{opponent.deck.length}</span>
            </div>
            {opponent.activeShip && (
              <div className="text-cyan-300/80 text-[10px] mt-1 bg-cyan-900/15 rounded-md px-1.5 py-1 truncate">
                🚢 {getCardDef(state.cards[opponent.activeShip].defId).name}
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Board divider */}
      <div className="board-divider mx-8" />

      {/* Player area */}
      <div className="flex justify-center gap-4 items-start">
        <CaptainCard captain={player.captain} def={playerCaptainDef} />
        <div className="flex flex-col gap-2">
          {renderRow(FRONT_SLOTS, humanPlayer)}
          {renderRow(BACK_SLOTS, humanPlayer)}
        </div>
        <div className="flex flex-col gap-2 min-w-[140px]">
          <div className="glass-light rounded-xl p-3 border border-gray-700/20 space-y-1.5 text-xs">
            <div className="flex justify-between">
              <span className="text-gray-500">Deck</span>
              <span className="text-gray-300 font-bold stat-badge">{player.deck.length}</span>
            </div>
            {player.activeShip && (
              <div className="text-cyan-300/80 text-[10px] mt-1 bg-cyan-900/15 rounded-md px-1.5 py-1 truncate">
                🚢 {getCardDef(state.cards[player.activeShip].defId).name}
              </div>
            )}
          </div>

          <div className="flex flex-col gap-1.5">
            {canActivateShip && player.activeShip && (
              <button
                onClick={() => dispatch({ type: "activateShip", shipInstanceId: player.activeShip! })}
                className="action-btn px-3 py-2 bg-cyan-700/60 hover:bg-cyan-600/60 rounded-xl text-xs font-bold border border-cyan-500/20 shadow-lg shadow-cyan-900/15 transition-all"
              >
                🚢 Activer navire
              </button>
            )}
            {canFlip && (
              <button
                onClick={() => {
                  const slot = Object.entries(player.board).find(([, v]) => v === null)?.[0] as Slot | undefined;
                  if (slot) dispatch({ type: "flipCaptain", slot });
                }}
                className="action-btn px-3 py-2 bg-red-700/60 hover:bg-red-600/60 rounded-xl text-xs font-bold border border-red-500/20 shadow-lg shadow-red-900/15 transition-all"
              >
                ⚔ Engager Capitaine
              </button>
            )}
            <button
              onClick={() => { dispatch({ type: "endTurn" }); resetUI(); }}
              disabled={isAiTurn || inCounterWindow}
              className="action-btn px-3 py-3 bg-gradient-to-r from-amber-700/80 to-amber-600/80 hover:from-amber-600/80 hover:to-amber-500/80 disabled:opacity-30 disabled:cursor-not-allowed rounded-xl text-sm font-bold border border-amber-500/20 shadow-lg shadow-amber-900/20 transition-all"
            >
              Fin de tour ➡
            </button>
            {uiMode.type !== "idle" && (
              <button onClick={resetUI} className="px-3 py-1.5 bg-gray-800/60 hover:bg-gray-700/60 rounded-lg text-xs text-gray-400 border border-gray-700/20 transition-all">
                ✕ Annuler
              </button>
            )}
          </div>
        </div>
      </div>

      {/* Hand */}
      <div className="mt-1">
        <div className="flex items-center gap-2 mb-2 px-4">
          <div className="text-[10px] text-gray-500 uppercase tracking-widest">Main</div>
          <div className="text-xs text-gray-600 font-bold stat-badge">{player.hand.length}</div>
          <div className="flex-1 h-px bg-gray-800" />
        </div>
        <div className="flex gap-2.5 overflow-x-auto px-4 pb-3">
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
      <div className="glass rounded-xl p-3 max-h-36 overflow-y-auto border border-gray-700/20">
        <div className="text-[9px] text-gray-600 uppercase tracking-widest mb-1.5">Journal</div>
        {state.log.slice(-15).reverse().map((entry, i) => (
          <div key={i} className={`py-0.5 text-xs ${i === 0 ? "text-gray-300" : "text-gray-500"}`}>
            <span className="text-gray-700 font-mono text-[10px]">T{entry.turn}</span>{" "}
            <span className={entry.player === humanPlayer ? "text-green-500/80" : "text-red-500/80"}>
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
