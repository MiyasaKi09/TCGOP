"use client";

import { useState, useMemo, useRef, useEffect, useCallback } from "react";
import type { DragEvent } from "react";
import type { GameAction, Slot, PlayerId, DeckDef } from "@/types";
import { useGameEngine } from "@/hooks/useGameEngine";
import { getCardDef, getCaptainDef } from "@/engine/cardRegistry";
import { initializeRegistry } from "@/engine/init";
import BoardSlot from "./BoardSlot";
import Card from "./Card";
import CaptainCard from "./CaptainCard";
import CardDetail from "./CardDetail";
import ActionMenu from "./ActionMenu";
import CaptainMenu from "./CaptainMenu";
import ShipMenu from "./ShipMenu";
import FullCard from "./FullCard";
import EventConfirm from "./EventConfirm";
import CombatVfxLayer from "./CombatVfxLayer";
import PlayRevealLayer from "./PlayRevealLayer";
import VfxStage from "./vfx/VfxStage";
import CutInLayer from "./vfx/CutInLayer";
import AmbientStage from "./vfx/AmbientStage";
import DeckBackdrop from "./board/DeckBackdrop";
import VolonteGauge from "./board/VolonteGauge";
import Pile from "./board/Pile";
import { StatusLegend } from "./StatusBadges";
import { useCombatVfx } from "@/lib/useCombatVfx";
import type { Difficulty } from "@/engine/ai";
import { FRONT_SLOTS, BACK_SLOTS } from "@/engine/utils";
import { faction } from "@/data/cardArt";
import { SkullCross, Crest } from "./icons";

initializeRegistry();

interface GameProps {
  playerDeck: DeckDef;
  aiDeck: DeckDef;
  difficulty?: Difficulty;
}

type UIMode =
  | { type: "idle" }
  | { type: "selectingSlot"; cardId: string }
  | { type: "selectingTarget"; attackerId: string; isSpecial: boolean }
  | { type: "selectingSupportTarget"; instanceId: string }
  | { type: "selectingEquipTarget"; objectId: string }
  | { type: "actionMenu"; instanceId: string }
  | { type: "captainMenu"; playerId: PlayerId }
  | { type: "shipMenu"; instanceId: string; isYou: boolean }
  | { type: "cardDetail"; defId: string; instanceId?: string }
  | { type: "confirmEvent"; instanceId: string }
  | { type: "confirmShip"; instanceId: string }
  | { type: "selectingCaptainSlot" };

export default function Game({ playerDeck, aiDeck, difficulty = "intermediate" }: GameProps) {
  const { state, validActions, dispatch, isAiTurn, humanPlayer, announcements, dismissAnnouncement } =
    useGameEngine(playerDeck, aiDeck, "player1", difficulty);
  const [uiMode, setUiMode] = useState<UIMode>({ type: "idle" });
  const [selectedHandCard, setSelectedHandCard] = useState<string | null>(null);
  const [hoveredHand, setHoveredHand] = useState<{ id: string; rect: DOMRect } | null>(null);
  const [webglActive, setWebglActive] = useState(false);
  const onVfxActiveChange = useCallback((a: boolean) => setWebglActive(a), []);

  // Remember the rect of the last-clicked card, to zoom the detail/action panel from it.
  const zoomFromRef = useRef<DOMRect | null>(null);
  useEffect(() => {
    const handler = (e: PointerEvent) => {
      const el = (e.target as HTMLElement | null)?.closest?.("[data-zoomsrc]") as HTMLElement | null;
      zoomFromRef.current = el ? el.getBoundingClientRect() : null;
    };
    document.addEventListener("pointerdown", handler, true);
    return () => document.removeEventListener("pointerdown", handler, true);
  }, []);

  const aiPlayer: PlayerId = humanPlayer === "player1" ? "player2" : "player1";
  const player = state.players[humanPlayer];
  const opponent = state.players[aiPlayer];
  const inCounterWindow = state.pendingAttack !== null;

  // Combat VFX: derive animations from state diffs (PV deltas + pendingAttack).
  const boardRef = useRef<HTMLElement | null>(null);
  const { events: vfxEvents, remove: removeVfx } = useCombatVfx(state, boardRef);

  // Deploy slots (characters and captain)
  const deploySlots = useMemo(() => {
    if (uiMode.type === "selectingSlot") {
      const slots = new Set<Slot>();
      for (const a of validActions) {
        if (a.type === "deployCharacter" && a.instanceId === uiMode.cardId) slots.add(a.slot);
      }
      return slots;
    }
    if (uiMode.type === "selectingCaptainSlot") {
      const slots = new Set<Slot>();
      for (const a of validActions) {
        if (a.type === "flipCaptain") slots.add(a.slot);
      }
      return slots;
    }
    return new Set<Slot>();
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

  // Support targets (heal allies, immobilize/trap enemies)
  const supportTargets = useMemo(() => {
    if (uiMode.type !== "selectingSupportTarget") return new Set<string>();
    const targets = new Set<string>();
    for (const a of validActions) {
      if (a.type === "baseSupportAction" && a.instanceId === uiMode.instanceId && a.targetInstanceId) {
        targets.add(a.targetInstanceId);
      }
    }
    return targets;
  }, [uiMode, validActions]);

  // Attack targets (includes captain attacks)
  const attackTargets = useMemo(() => {
    if (uiMode.type !== "selectingTarget") return new Set<string>();
    const targets = new Set<string>();
    const isCaptainAttack = uiMode.attackerId.startsWith("captain_");
    const actionType = isCaptainAttack ? "captainAttack" : (uiMode.isSpecial ? "specialAttack" : "baseAttack");
    for (const a of validActions) {
      if (a.type === actionType) {
        if (isCaptainAttack && a.type === "captainAttack") {
          if (a.targetIsCaptain) targets.add(`captain_${aiPlayer}`);
          else targets.add(a.targetInstanceId);
        } else if ("attackerInstanceId" in a && a.attackerInstanceId === uiMode.attackerId) {
          if ("targetIsCaptain" in a && a.targetIsCaptain) targets.add(`captain_${aiPlayer}`);
          else if ("targetInstanceId" in a) targets.add(a.targetInstanceId);
        }
      }
    }
    return targets;
  }, [uiMode, validActions, aiPlayer]);

  // Does the attack being aimed have the Zone (AoE) trait? Used to light up the
  // whole enemy front as the impact area.
  const attackIsZone = useMemo(() => {
    if (uiMode.type !== "selectingTarget") return false;
    const { attackerId, isSpecial } = uiMode;
    if (attackerId.startsWith("captain_")) {
      const pid = attackerId.replace("captain_", "") as PlayerId;
      const cd = getCaptainDef(state.players[pid].captain.defId);
      const atk = isSpecial ? cd.verso.specialAttack : cd.verso.baseAction;
      return !!atk?.attackTraits?.includes("zone");
    }
    const inst = state.cards[attackerId];
    if (!inst) return false;
    const def = getCardDef(inst.defId);
    const atk = isSpecial ? def.specialAttack : def.baseAction;
    return !!atk?.attackTraits?.includes("zone");
  }, [uiMode, state]);

  const selecting =
    uiMode.type === "selectingTarget" || uiMode.type === "selectingSupportTarget" ||
    uiMode.type === "selectingSlot" || uiMode.type === "selectingEquipTarget" ||
    uiMode.type === "selectingCaptainSlot";

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

  // Drag a hand card → enter the matching placement mode (slots/targets light up).
  const handleHandDragStart = (e: DragEvent, instanceId: string) => {
    const card = state.cards[instanceId];
    if (!card) return;
    const def = getCardDef(card.defId);
    e.dataTransfer.effectAllowed = "move";
    try { e.dataTransfer.setData("text/plain", instanceId); } catch { /* some browsers */ }
    if (def.type === "character") {
      setSelectedHandCard(instanceId);
      setUiMode({ type: "selectingSlot", cardId: instanceId });
    } else if (def.type === "object") {
      setSelectedHandCard(instanceId);
      setUiMode({ type: "selectingEquipTarget", objectId: instanceId });
    }
  };

  const handleSlotClick = (slot: Slot, isPlayerSide: boolean) => {
    if (uiMode.type === "selectingSlot" && isPlayerSide && deploySlots.has(slot)) {
      dispatch({ type: "deployCharacter", instanceId: uiMode.cardId, slot });
      resetUI();
    } else if (uiMode.type === "selectingCaptainSlot" && isPlayerSide && deploySlots.has(slot)) {
      dispatch({ type: "flipCaptain", slot });
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
    if (uiMode.type === "selectingSupportTarget" && supportTargets.has(instanceId)) {
      dispatch({ type: "baseSupportAction", instanceId: uiMode.instanceId, targetInstanceId: instanceId });
      resetUI();
      return;
    }
    if (uiMode.type === "selectingTarget" && !isPlayerSide && attackTargets.has(instanceId)) {
      if (uiMode.attackerId.startsWith("captain_")) {
        dispatch({ type: "captainAttack", targetInstanceId: instanceId } as GameAction);
      } else {
        dispatch({
          type: uiMode.isSpecial ? "specialAttack" : "baseAttack",
          attackerInstanceId: uiMode.attackerId,
          targetInstanceId: instanceId,
        } as GameAction);
      }
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
      if (uiMode.attackerId.startsWith("captain_")) {
        dispatch({ type: "captainAttack", targetInstanceId: `captain_${aiPlayer}`, targetIsCaptain: true } as GameAction);
      } else {
        dispatch({
          type: uiMode.isSpecial ? "specialAttack" : "baseAttack",
          attackerInstanceId: uiMode.attackerId,
          targetInstanceId: `captain_${aiPlayer}`,
          targetIsCaptain: true,
        } as GameAction);
      }
      resetUI();
    }
  };

  // Click on either side's captain (prow or verso). Attack-targeting takes
  // priority; otherwise open its menu (powers + available actions).
  const onCaptainClick = (playerId: PlayerId) => {
    if (uiMode.type === "selectingTarget" && playerId === aiPlayer && attackTargets.has(`captain_${aiPlayer}`)) {
      handleCaptainClick(playerId);
      return;
    }
    if (uiMode.type === "idle") setUiMode({ type: "captainMenu", playerId });
  };

  // Render the 3 slots of one line (front or back) as a vertical column of tokens.
  const renderLine = (slots: readonly string[], playerId: PlayerId) => {
    const isPlayerSide = playerId === humanPlayer;
    const ps = state.players[playerId];
    const captainSlot = ps.captain.flipped ? ps.captain.slot : null;

    return slots.map((s) => {
      const slot = s as Slot;

      // Captain verso occupying this slot
      if (captainSlot === slot) {
        const capDef = getCaptainDef(ps.captain.defId);
        const isTarget = !isPlayerSide && uiMode.type === "selectingTarget" && attackTargets.has(`captain_${playerId}`);
        const pvPercent = Math.max(0, (ps.captain.currentPv / capDef.verso.pv) * 100);
        return (
          <div
            key={slot}
            data-inst={`captain_${playerId}`}
            onClick={() => onCaptainClick(playerId)}
            className={`relative w-[5.5rem] h-[7.3rem] rounded-xl flex flex-col items-center justify-center p-1 cursor-pointer transition-all ${isTarget ? "ring-target" : "hover:brightness-110"} ${selecting && !isTarget ? "slot-dim" : ""}`}
            style={{ background: "radial-gradient(120% 80% at 50% 6%, #3a1414 0%, #1a0c0c 70%)", boxShadow: "inset 0 0 0 2px #E0463F" }}
          >
            <div className="font-oswald text-[8px] uppercase tracking-widest text-red-300/80 font-bold">★ Verso</div>
            <div className="font-cinzel text-[11px] font-bold text-white text-center leading-tight truncate w-full">{capDef.name}</div>
            <div className="flex justify-center gap-2 text-[11px] my-1 font-oswald font-bold">
              <span style={{ color: "#FF7062" }}>⚔{capDef.verso.atk}</span>
              <span style={{ color: "#7FB0E8" }}>🛡{capDef.verso.def}</span>
            </div>
            <div className="w-full h-1.5 rounded-full overflow-hidden" style={{ background: "rgba(8,12,18,.7)" }}>
              <div className="h-full rounded-full" style={{ width: `${pvPercent}%`, background: pvPercent > 50 ? "#5BC46A" : pvPercent > 25 ? "#E8C53B" : "#FF7062" }} />
            </div>
            <div className="font-oswald text-[10px] font-bold mt-0.5" style={{ color: pvPercent <= 25 ? "#FF7062" : "#5BC46A" }}>{ps.captain.currentPv}/{capDef.verso.pv}</div>
          </div>
        );
      }

      const charId = ps.board[slot];
      const instance = charId ? state.cards[charId] : null;
      const def = instance ? getCardDef(instance.defId) : null;
      const isValidDeploy = isPlayerSide && deploySlots.has(slot);
      const isValidTarget = (!isPlayerSide && uiMode.type === "selectingTarget" && charId !== null && attackTargets.has(charId!))
        || (uiMode.type === "selectingSupportTarget" && charId !== null && supportTargets.has(charId!));
      const isEquipTarget = isPlayerSide && uiMode.type === "selectingEquipTarget" && charId !== null && equipTargets.has(charId!);
      const isImpact = !isPlayerSide && attackIsZone && slot.startsWith("V") && isValidTarget;
      const eligible = isValidTarget || isValidDeploy || isEquipTarget;
      const isDimmed = selecting && !eligible;

      const act = () => {
        if (isValidDeploy) handleSlotClick(slot, isPlayerSide);
        else if (charId) handleBoardCharClick(charId, isPlayerSide);
      };
      return (
        <BoardSlot
          key={slot}
          slot={slot}
          instance={instance}
          def={def}
          isPlayerSide={isPlayerSide}
          isValidTarget={isValidTarget}
          isValidDeploy={isValidDeploy || isEquipTarget}
          isImpact={isImpact}
          isDimmed={isDimmed}
          onClick={act}
          onDrop={act}
        />
      );
    });
  };

  // One crew line (3 slots) laid out as a horizontal row on the deck.
  const renderRow = (slots: readonly string[], playerId: PlayerId) => (
    <div className="flex gap-2 justify-center">{renderLine(slots, playerId)}</div>
  );

  // A captain at the side of the deck: card + Navire/Fruit slots + Pioche/Défausse.
  const renderCaptain = (playerId: PlayerId, isYou: boolean) => {
    const ps = state.players[playerId];
    const capDef = getCaptainDef(ps.captain.defId);
    const captainTarget = !isYou && uiMode.type === "selectingTarget" && attackTargets.has(`captain_${playerId}`);

    const navireSlot = (
      <button
        onClick={() => { if (ps.activeShip) setUiMode({ type: "shipMenu", instanceId: ps.activeShip, isYou }); }}
        disabled={!ps.activeShip}
        title={ps.activeShip ? getCardDef(state.cards[ps.activeShip].defId).name : "Navire"}
        className="flex flex-col items-center gap-0.5 group disabled:cursor-default"
      >
        <div className="w-11 h-14 rounded-lg flex items-center justify-center transition-all group-enabled:group-hover:brightness-125"
          style={{ background: ps.activeShip ? "linear-gradient(160deg,#0e2730,#081820)" : "rgba(6,10,16,.4)", boxShadow: ps.activeShip ? "inset 0 0 0 1px rgba(91,198,224,.6)" : "inset 0 0 0 1.5px rgba(255,255,255,.16)" }}>
          <Crest which="anchor" size={17} color={ps.activeShip ? "#7FD0E8" : "rgba(255,255,255,.3)"} />
        </div>
        <span className="font-oswald text-[7px] uppercase tracking-[.15em] text-white/45">Navire</span>
      </button>
    );

    const fruitSlot = (
      <button onClick={() => onCaptainClick(playerId)} className="flex flex-col items-center gap-0.5 group">
        <div className="w-11 h-14 rounded-lg flex items-center justify-center transition-all group-hover:brightness-125"
          style={{ background: ps.captain.flipped ? "linear-gradient(160deg,#2a0f0f,#170a0a)" : "rgba(6,10,16,.4)", boxShadow: ps.captain.flipped ? "inset 0 0 0 1px rgba(224,70,63,.6)" : "inset 0 0 0 1.5px rgba(255,255,255,.16)" }}>
          <span className="text-[17px] leading-none" style={{ color: ps.captain.flipped ? "#FF7062" : "rgba(232,184,75,.5)" }}>✦</span>
        </div>
        <span className="font-oswald text-[7px] uppercase tracking-[.15em] text-white/45">Fruit</span>
      </button>
    );

    const sideSlots = <div className="flex flex-col gap-1.5 justify-center">{navireSlot}{fruitSlot}</div>;
    const captainCard = (
      <div
        data-inst={!ps.captain.flipped ? `captain_${playerId}` : undefined}
        onClick={() => onCaptainClick(playerId)}
        className={`rounded-xl transition-all cursor-pointer ${captainTarget ? "ring-target" : ""} ${selecting && !captainTarget ? "slot-dim" : ""}`}
      >
        <CaptainCard captain={ps.captain} def={capDef} isOpponent={!isYou} />
      </div>
    );

    return (
      <div className="flex flex-col items-center gap-2">
        <div className="flex items-center gap-1.5">
          {isYou ? <>{sideSlots}{captainCard}</> : <>{captainCard}{sideSlots}</>}
        </div>
        <div className="flex gap-2.5">
          <Pile label="Pioche" count={ps.deck.length} />
          <Pile label="Défausse" count={ps.graveyard.length} />
        </div>
      </div>
    );
  };

  // Counter window
  const renderCounterWindow = () => {
    if (!inCounterWindow) return null;
    const pending = state.pendingAttack!;
    const counterActions = validActions.filter(
      (a) => a.type === "playCounter" || a.type === "passCounter" || a.type === "useShield" || (a.type === "useHaki" && a.hakiType === "observation")
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
        <div className="rounded-2xl p-6 max-w-lg shadow-2xl animate-modal-enter" style={{ background: "rgba(12,16,22,.92)", boxShadow: "inset 0 0 0 1px rgba(224,70,63,.5), 0 20px 60px rgba(0,0,0,.6)" }}>
          <div className="flex items-center gap-2 mb-4">
            <div className="w-2 h-2 rounded-full bg-red-500 animate-pulse" />
            <h3 className="font-cinzel text-lg font-bold text-red-400 uppercase tracking-wider">Attaque entrante</h3>
          </div>
          <div className="rounded-xl p-4 mb-4" style={{ background: "rgba(255,255,255,.04)" }}>
            <p className="font-spectral text-sm text-white/70 mb-1">
              <span className="text-red-300 font-bold">{attackerName}</span> attaque <span className="text-blue-300 font-bold">{targetName}</span>
            </p>
            <div className="flex items-baseline gap-3 mt-2">
              <span className="font-oswald text-3xl font-black text-red-400">{pending.rawDamage}</span>
              <span className="text-sm text-white/40">dégâts</span>
              {pending.element && <span className="text-xs px-2 py-0.5 rounded-full bg-white/10 text-white/70 capitalize">{pending.element}</span>}
              {pending.hasHaki && <span className="text-xs px-2 py-0.5 rounded-full bg-purple-900/50 text-purple-300">Haki</span>}
            </div>
          </div>
          <div className="flex gap-2 flex-wrap">
            {counterActions.map((action, i) => {
              if (action.type === "playCounter") {
                const card = state.cards[action.instanceId];
                const def = getCardDef(card.defId);
                return (
                  <button key={i} onClick={() => dispatch(action)} className="action-btn font-oswald px-4 py-2.5 bg-blue-600/80 hover:bg-blue-500/80 rounded-xl text-sm font-bold transition-all">
                    🛡 {def.name} <span className="text-blue-200/70 text-xs">({def.cost}V)</span>
                  </button>
                );
              }
              if (action.type === "useShield") {
                const blockerDef = getCardDef(state.cards[action.blockerInstanceId].defId);
                return (
                  <button key={i} onClick={() => dispatch(action)} className="action-btn font-oswald px-4 py-2.5 bg-amber-700/80 hover:bg-amber-600/80 rounded-xl text-sm font-bold transition-all">
                    🛡 Bloquer ({blockerDef.name})
                  </button>
                );
              }
              if (action.type === "useHaki") {
                return (
                  <button key={i} onClick={() => dispatch(action)} className="action-btn font-oswald px-4 py-2.5 bg-purple-600/80 hover:bg-purple-500/80 rounded-xl text-sm font-bold transition-all">
                    👁 Haki Observation
                  </button>
                );
              }
              if (action.type === "passCounter") {
                return (
                  <button key={i} onClick={() => dispatch(action)} className="action-btn font-oswald px-4 py-2.5 bg-white/10 hover:bg-white/20 rounded-xl text-sm transition-all text-white/80">
                    Subir les dégâts
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
      <div className="sea-bg min-h-screen flex flex-col items-center justify-center gap-8">
        <div className={`font-cinzel text-6xl font-black tracking-tight ${won ? "text-green-400" : "text-red-400"}`} style={{ textShadow: "0 4px 24px rgba(0,0,0,.6)" }}>
          {won ? "VICTOIRE !" : "DÉFAITE…"}
        </div>
        <div className="font-oswald text-white/50 text-lg uppercase tracking-widest">Tour {state.turnNumber}</div>
        <button onClick={() => window.location.reload()} className="action-btn gold-surface font-oswald font-bold px-10 py-4 rounded-xl text-lg shadow-xl transition-all">
          Rejouer
        </button>
      </div>
    );
  }

  const opponentCaptainDef = getCaptainDef(opponent.captain.defId);
  const foeFac = faction(opponentCaptainDef.faction);
  const canFlip = validActions.some((a) => a.type === "flipCaptain");
  const canActivateShip = validActions.some((a) => a.type === "activateShip");
  const canKingHaki = validActions.some((a) => a.type === "useHaki" && a.hakiType === "king");

  const statusText = (() => {
    if (isAiTurn) return { text: "Tour de l'adversaire…", color: "text-yellow-400", pulse: true };
    if (inCounterWindow) return { text: "Réaction !", color: "text-red-400", pulse: true };
    if (uiMode.type === "selectingTarget") return { text: "Choisissez une cible", color: "text-red-300", pulse: true };
    if (uiMode.type === "selectingSupportTarget") return { text: "Cible du pouvoir", color: "text-cyan-300", pulse: true };
    if (uiMode.type === "selectingSlot") return { text: "Choisissez un emplacement", color: "text-green-300", pulse: true };
    if (uiMode.type === "selectingCaptainSlot") return { text: "Placez le capitaine", color: "text-amber-300", pulse: true };
    if (uiMode.type === "selectingEquipTarget") return { text: "Équipez un personnage", color: "text-amber-300", pulse: true };
    return { text: "Votre tour", color: "text-green-400", pulse: false };
  })();

  return (
    <div
      className="sea-bg h-screen flex flex-col overflow-hidden text-white relative"
      style={{ userSelect: "none", WebkitUserSelect: "none" }}
      onContextMenu={(e) => e.preventDefault()}
    >
      {/* Animated sea (behind everything; static .sea-bg shows through on low tier) */}
      <AmbientStage />

      {/* HEADER */}
      <header className="relative z-10 shrink-0 flex items-center gap-3 px-4 py-2" style={{ borderBottom: "1px solid rgba(232,184,75,.18)", background: "linear-gradient(180deg,rgba(8,12,18,.9),rgba(6,9,14,.6))" }}>
        <span className="font-cinzel font-extrabold text-[16px] tracking-wider" style={{ color: "#E8B84B" }}>TCGOP</span>
        <span className="font-oswald text-[9px] uppercase tracking-[.18em] text-white/40 hidden sm:inline">Grand Line</span>
        <span className="font-oswald text-[9px] uppercase tracking-[.22em] text-white/30 hidden sm:inline">Plateau · Abordage</span>
        <div className={`ml-auto font-oswald text-sm font-semibold ${statusText.color} ${statusText.pulse ? "animate-pulse" : ""}`}>{statusText.text}</div>
        <div className="flex items-center gap-1.5 ml-3 pl-3" style={{ borderLeft: "1px solid rgba(255,255,255,.1)" }}>
          <Crest which={foeFac.crest} size={12} color={foeFac.accent} />
          <span className="font-oswald text-[10px] text-white/45">Main adverse {opponent.hand.length}</span>
        </div>
      </header>

      {/* Combat VFX + play-reveal overlays */}
      <VfxStage onActiveChange={onVfxActiveChange} />
      <CombatVfxLayer events={vfxEvents} remove={removeVfx} webglActive={webglActive} />
      <CutInLayer />
      <PlayRevealLayer announcements={announcements} dismiss={dismissAnnouncement} />

      {/* BOARD — single "Abordage" deck (crews top/bottom, captains diagonal) */}
      <main ref={boardRef} className="relative z-10 flex-1 min-h-0 overflow-hidden flex items-center justify-center px-3 py-2">
        <DeckBackdrop />

        {/* discreet turn badge, far left */}
        <div className="absolute left-2 top-1/2 -translate-y-1/2 z-10 flex flex-col items-center gap-1 px-1.5 py-2 rounded-full"
          style={{ background: "rgba(6,12,20,.6)", border: "1px solid rgba(232,184,75,.28)" }}>
          <SkullCross size={12} color="#E8B84B" />
          <span className="font-cinzel font-bold text-[9px] tracking-widest" style={{ color: "#E8B84B", writingMode: "vertical-rl" }}>TOUR {state.turnNumber}</span>
        </div>

        {/* foe Volonté — top-left */}
        <div className="absolute top-2 left-10 z-10">
          <VolonteGauge value={opponent.volonte} label="Volonté adverse" align="left" />
        </div>

        {/* your control cluster — bottom-right: Fin de tour + Volonté */}
        <div className="absolute bottom-2 right-3 z-10 flex flex-col items-end gap-2">
          <button
            onClick={() => { dispatch({ type: "endTurn" }); resetUI(); }}
            disabled={isAiTurn || inCounterWindow}
            className="action-btn gold-surface font-oswald font-bold px-5 py-2.5 rounded-xl text-sm shadow-xl disabled:opacity-30 disabled:cursor-not-allowed transition-all"
          >
            Fin de tour ➡
          </button>
          <VolonteGauge value={player.volonte} label="Volonté" align="right" />
        </div>

        <div className="relative z-10 w-full h-full flex items-stretch justify-center gap-3">
          {/* left edge — your captain, pulled toward the bottom (diagonal) */}
          <div className="shrink-0 flex flex-col justify-end pb-1">{renderCaptain(humanPlayer, true)}</div>

          {/* centre — both crews, foe on top, you on the bottom */}
          <div className="flex-1 min-w-0 flex flex-col items-center justify-center gap-1.5">
            {renderRow(BACK_SLOTS, aiPlayer)}
            {renderRow(FRONT_SLOTS, aiPlayer)}
            <div className="h-px w-2/3 my-0.5" style={{ background: "linear-gradient(90deg,transparent,rgba(232,184,75,.5),transparent)" }} />
            {renderRow(FRONT_SLOTS, humanPlayer)}
            {renderRow(BACK_SLOTS, humanPlayer)}
          </div>

          {/* right edge — foe captain, pulled toward the top (diagonal) */}
          <div className="shrink-0 flex flex-col justify-start pt-1">{renderCaptain(aiPlayer, false)}</div>
        </div>
      </main>

      {/* FOOTER — hand + actions */}
      <footer className="relative z-10 shrink-0 px-3 pb-2 pt-1 flex flex-col gap-1.5" style={{ background: "linear-gradient(0deg,rgba(6,9,14,.75),transparent)" }}>
        <div className="flex items-end gap-3">
          {/* Hand */}
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2 mb-1">
              <span className="font-oswald text-[10px] uppercase tracking-widest text-white/45">Main</span>
              <span className="font-oswald text-[11px] font-bold text-white/60">{player.hand.length}</span>
              <span className="font-oswald text-[8px] uppercase tracking-[.18em] text-white/25 hidden sm:inline">· Survole pour agrandir</span>
              <div className="hidden md:block"><StatusLegend types={["freeze", "burn", "poison", "immobilize", "desiccation"]} /></div>
              <div className="flex-1 h-px bg-white/10" />
            </div>
            <div className="flex gap-2 overflow-x-auto pb-1 items-end" style={{ minHeight: "170px" }}>
              {player.hand.map((id, i) => {
                const card = state.cards[id];
                if (!card) return null;
                const def = getCardDef(card.defId);
                const canPlay = validActions.some((a) => {
                  if ("instanceId" in a && a.instanceId === id) return true;
                  if ("objectInstanceId" in a && a.objectInstanceId === id) return true;
                  return false;
                });
                const dragType = def.type === "character" || def.type === "object";
                // Subtle fan: rotate/lift cards by distance from the hand's centre.
                const off = i - (player.hand.length - 1) / 2;
                const rot = Math.max(-7, Math.min(7, off * 2));
                const ty = Math.min(14, Math.abs(off) * 2.4);
                const hovered = hoveredHand?.id === id;
                return (
                  <div
                    key={id}
                    className="flex-shrink-0 transition-transform duration-200"
                    style={{ transform: hovered ? "rotate(0deg) translateY(-6px)" : `rotate(${rot}deg) translateY(${ty}px)`, transformOrigin: "bottom center" }}
                    onMouseEnter={(e) => setHoveredHand({ id, rect: e.currentTarget.getBoundingClientRect() })}
                    onMouseLeave={() => setHoveredHand((h) => (h?.id === id ? null : h))}
                  >
                    <Card
                      instance={card}
                      def={def}
                      width={118}
                      selected={selectedHandCard === id}
                      highlight={canPlay}
                      draggable={canPlay && dragType}
                      onDragStart={(e) => { setHoveredHand(null); handleHandDragStart(e, id); }}
                      onClick={() => {
                        if (canPlay) handleHandCardClick(id);
                        else setUiMode({ type: "cardDetail", defId: card.defId, instanceId: id });
                      }}
                    />
                  </div>
                );
              })}
              {player.hand.length === 0 && <div className="font-spectral italic text-white/30 text-sm px-2">Main vide</div>}
            </div>
          </div>

          {/* Actions / hints (Fin de tour + piles live on the board) */}
          <div className="shrink-0 w-40 flex flex-col gap-1.5 justify-end">
            {(canFlip || canActivateShip || canKingHaki) && (
              <div className="font-oswald text-[9px] text-amber-300/85 leading-snug px-2 py-1.5 rounded-lg flex flex-col gap-0.5" style={{ background: "rgba(232,184,75,.1)", boxShadow: "inset 0 0 0 1px rgba(232,184,75,.25)" }}>
                {canFlip && <span>⚔ Clique ton Capitaine pour l&apos;engager</span>}
                {canKingHaki && <span>👑 Haki des Rois dispo (Capitaine)</span>}
                {canActivateShip && <span>⚓ Clique ton Navire pour l&apos;activer</span>}
              </div>
            )}
            {uiMode.type !== "idle" && (
              <button onClick={resetUI} className="font-oswald px-3 py-2 bg-white/8 hover:bg-white/15 rounded-lg text-xs text-white/55 transition-all">✕ Annuler</button>
            )}
          </div>
        </div>

        {/* Log */}
        <div className="rounded-lg px-2.5 py-1.5 max-h-[68px] overflow-y-auto" style={{ background: "rgba(8,12,18,.6)", border: "1px solid rgba(255,255,255,.06)" }}>
          {state.log.slice(-12).reverse().map((entry, i) => (
            <div key={i} className={`font-spectral py-0.5 text-xs ${i === 0 ? "text-white/80" : "text-white/45"}`}>
              <span className="font-mono text-[10px] text-white/30">T{entry.turn}</span>{" "}
              <span className={entry.player === humanPlayer ? "text-green-500/80" : "text-red-500/80"}>{entry.player === humanPlayer ? "►" : "◄"}</span>{" "}
              {entry.message}
            </div>
          ))}
        </div>
      </footer>

      {/* Overlays */}
      {renderCounterWindow()}

      {uiMode.type === "actionMenu" && (() => {
        const inst = state.cards[uiMode.instanceId];
        if (!inst) return null;
        const def = getCardDef(inst.defId);
        return (
          <ActionMenu
            instance={inst} def={def} state={state} validActions={validActions} originRect={zoomFromRef.current}
            onBaseAttack={() => setUiMode({ type: "selectingTarget", attackerId: uiMode.instanceId, isSpecial: false })}
            onSpecialAttack={() => setUiMode({ type: "selectingTarget", attackerId: uiMode.instanceId, isSpecial: true })}
            onSupportAction={() => {
              const hasSupportTargets = validActions.some(
                (a) => a.type === "baseSupportAction" && a.instanceId === uiMode.instanceId && a.targetInstanceId
              );
              if (hasSupportTargets) {
                setUiMode({ type: "selectingSupportTarget", instanceId: uiMode.instanceId });
              } else {
                dispatch({ type: "baseSupportAction", instanceId: uiMode.instanceId });
                resetUI();
              }
            }}
            onViewDetail={() => setUiMode({ type: "cardDetail", defId: inst.defId, instanceId: uiMode.instanceId })}
            onClose={resetUI}
          />
        );
      })()}

      {uiMode.type === "captainMenu" && (() => {
        const ps = state.players[uiMode.playerId];
        const capDef = getCaptainDef(ps.captain.defId);
        const isYou = uiMode.playerId === humanPlayer;
        return (
          <CaptainMenu
            captain={ps.captain} def={capDef} state={state} validActions={validActions} isYou={isYou} originRect={zoomFromRef.current}
            onFlip={() => setUiMode({ type: "selectingCaptainSlot" })}
            onAttack={() => setUiMode({ type: "selectingTarget", attackerId: `captain_${humanPlayer}`, isSpecial: false })}
            onKingHaki={() => { dispatch({ type: "useHaki", hakiType: "king" }); resetUI(); }}
            onClose={resetUI}
          />
        );
      })()}

      {uiMode.type === "shipMenu" && (() => {
        const inst = state.cards[uiMode.instanceId];
        if (!inst) return null;
        const def = getCardDef(inst.defId);
        const canActivate = uiMode.isYou && validActions.some((a) => a.type === "activateShip" && a.shipInstanceId === uiMode.instanceId);
        const used = def.shipActive?.oncePerGame && inst.usedOnceAbilities.includes(def.shipActive.name);
        const reason = used ? "déjà utilisé" : !canActivate ? "Volonté insuffisante" : null;
        return (
          <ShipMenu
            instance={inst} def={def} state={state} isYou={uiMode.isYou} canActivate={canActivate} activateReason={reason} originRect={zoomFromRef.current}
            onActivate={() => { dispatch({ type: "activateShip", shipInstanceId: uiMode.instanceId }); resetUI(); }}
            onClose={resetUI}
          />
        );
      })()}

      {uiMode.type === "cardDetail" && (() => {
        const def = getCardDef(uiMode.defId);
        const inst = uiMode.instanceId ? state.cards[uiMode.instanceId] : undefined;
        return <CardDetail def={def} instance={inst} state={state} onClose={resetUI} originRect={zoomFromRef.current} />;
      })()}

      {/* Hand hover preview — large floating card above the hovered hand card */}
      {hoveredHand && uiMode.type === "idle" && !inCounterWindow && (() => {
        const card = state.cards[hoveredHand.id];
        if (!card) return null;
        const def = getCardDef(card.defId);
        const PW = 250, PH = Math.round((419 / 300) * PW);
        const vw = typeof window !== "undefined" ? window.innerWidth : 1200;
        const left = Math.min(Math.max(hoveredHand.rect.left + hoveredHand.rect.width / 2 - PW / 2, 8), vw - PW - 8);
        let top = hoveredHand.rect.top - PH - 12;
        if (top < 8) top = hoveredHand.rect.bottom + 12;
        return (
          <div className="fixed z-30 pointer-events-none animate-fade-in" style={{ left, top }}>
            <FullCard def={def} instance={card} state={state} width={PW} />
          </div>
        );
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
