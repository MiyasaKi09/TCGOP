"use client";

import { useState, useMemo, useRef, useEffect } from "react";
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
import EventConfirm from "./EventConfirm";
import CombatVfxLayer from "./CombatVfxLayer";
import PlayRevealLayer from "./PlayRevealLayer";
import { useCombatVfx } from "@/lib/useCombatVfx";
import type { Difficulty } from "@/engine/ai";
import { FRONT_SLOTS, BACK_SLOTS } from "@/engine/utils";
import { faction } from "@/data/cardArt";
import { Bolt, SkullCross, Crest } from "./icons";

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
  | { type: "cardDetail"; defId: string; instanceId?: string }
  | { type: "confirmEvent"; instanceId: string }
  | { type: "confirmShip"; instanceId: string }
  | { type: "selectingCaptainSlot" };

export default function Game({ playerDeck, aiDeck, difficulty = "intermediate" }: GameProps) {
  const { state, validActions, dispatch, isAiTurn, humanPlayer, announcements, dismissAnnouncement } =
    useGameEngine(playerDeck, aiDeck, "player1", difficulty);
  const [uiMode, setUiMode] = useState<UIMode>({ type: "idle" });
  const [selectedHandCard, setSelectedHandCard] = useState<string | null>(null);

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
            onClick={() => {
              if (isTarget) handleCaptainClick(playerId);
              else if (isPlayerSide && ps.captain.flipped && !ps.captain.tapped) {
                const canCaptainAttack = validActions.some((a) => a.type === "captainAttack");
                if (canCaptainAttack) {
                  setUiMode({ type: "selectingTarget", attackerId: `captain_${playerId}`, isSpecial: false });
                }
              }
            }}
            className={`relative w-[5.5rem] h-[7.3rem] rounded-xl flex flex-col items-center justify-center p-1 cursor-pointer transition-all ${isTarget ? "ring-target" : "hover:brightness-110"}`}
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
          onClick={act}
          onDrop={act}
        />
      );
    });
  };

  // One ship (deck art + captain at the prow + front/back columns).
  const renderShip = (playerId: PlayerId, isYou: boolean) => {
    const ps = state.players[playerId];
    const capDef = getCaptainDef(ps.captain.defId);
    const fac = faction(capDef.faction);
    const front = <div className="flex flex-col gap-2">{renderLine(FRONT_SLOTS, playerId)}</div>;
    const back = <div className="flex flex-col gap-2">{renderLine(BACK_SLOTS, playerId)}</div>;
    // Front line toward the centre (battle line): your front on the right, foe's front on the left.
    const columns = (
      <div className="flex gap-2 justify-center">
        {isYou ? <>{back}{front}</> : <>{front}{back}</>}
      </div>
    );

    const captainTarget = !isYou && uiMode.type === "selectingTarget" && attackTargets.has(`captain_${playerId}`);
    const captainBlock = (
      <div className="flex flex-col items-center gap-1">
        <div
          data-inst={!ps.captain.flipped ? `captain_${playerId}` : undefined}
          onClick={!isYou ? () => handleCaptainClick(playerId) : undefined}
          className={`rounded-xl transition-all ${captainTarget ? "ring-target cursor-pointer" : ""}`}
        >
          <CaptainCard captain={ps.captain} def={capDef} isOpponent={!isYou} />
        </div>
        {isYou && (
          <div className="flex items-center gap-2 px-3 py-1 rounded-full" style={{ background: "rgba(6,12,20,.66)" }}>
            <Bolt size={13} />
            <span className="font-cinzel font-bold text-[13px]" style={{ color: "#E8B84B" }}>{ps.volonte}</span>
            <span className="font-oswald text-[9px] uppercase tracking-wider text-white/45">Volonté</span>
          </div>
        )}
      </div>
    );

    return (
      <div className={`relative h-full ${isYou ? "animate-ship-bob" : "animate-ship-bob2"}`} style={{ aspectRatio: "941 / 1672", maxWidth: "340px", width: "100%" }}>
        {/* eslint-disable-next-line @next/next/no-img-element */}
        <img src={fac.shipImg} alt="" draggable={false}
          className="absolute inset-0 w-full h-full object-contain pointer-events-none select-none"
          style={{ transform: isYou ? undefined : "scaleY(-1)", filter: "drop-shadow(0 8px 20px rgba(0,0,0,.55))" }} />
        <div className="absolute inset-0 flex flex-col items-center justify-center gap-3 p-2">
          {isYou ? <>{captainBlock}{columns}</> : <>{columns}{captainBlock}</>}
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
      className="sea-bg h-screen flex flex-col overflow-hidden text-white"
      style={{ userSelect: "none", WebkitUserSelect: "none" }}
      onContextMenu={(e) => e.preventDefault()}
    >
      {/* HEADER */}
      <header className="shrink-0 flex items-center gap-3 px-4 py-2" style={{ borderBottom: "1px solid rgba(232,184,75,.18)", background: "linear-gradient(180deg,rgba(8,12,18,.9),rgba(6,9,14,.6))" }}>
        <span className="font-cinzel font-extrabold text-[16px] tracking-wider" style={{ color: "#E8B84B" }}>TCGOP</span>
        <span className="font-oswald text-[9px] uppercase tracking-[.18em] text-white/40 hidden sm:inline">Grand Line</span>
        <div className={`ml-auto font-oswald text-sm font-semibold ${statusText.color} ${statusText.pulse ? "animate-pulse" : ""}`}>{statusText.text}</div>
        <div className="flex items-center gap-2 ml-3 pl-3" style={{ borderLeft: "1px solid rgba(255,255,255,.1)" }}>
          <Crest which={foeFac.crest} size={13} color={foeFac.accent} />
          <span className="font-oswald text-[11px] text-white/55">Main {opponent.hand.length} · Deck {opponent.deck.length}</span>
          {opponent.activeShip && (
            <button onClick={() => setUiMode({ type: "cardDetail", defId: state.cards[opponent.activeShip!].defId, instanceId: opponent.activeShip! })}
              className="font-oswald text-[10px] text-cyan-200/80 bg-cyan-900/20 rounded px-1.5 py-0.5 hover:bg-cyan-800/30 transition-all truncate max-w-[120px]">
              ⚓ {getCardDef(state.cards[opponent.activeShip].defId).name}
            </button>
          )}
        </div>
      </header>

      {/* Combat VFX + play-reveal overlays */}
      <CombatVfxLayer events={vfxEvents} remove={removeVfx} />
      <PlayRevealLayer announcements={announcements} dismiss={dismissAnnouncement} />

      {/* BOARD — two ships broadside */}
      <main ref={boardRef} className="flex-1 min-h-0 flex items-stretch justify-center gap-1 px-2 py-2">
        <section className="flex-1 min-w-0 flex items-center justify-center">{renderShip(humanPlayer, true)}</section>

        <div className="shrink-0 flex flex-col items-center justify-center gap-2 px-0.5">
          <div className="vs-line flex-1 w-px" />
          <div className="flex flex-col items-center gap-1 py-3 px-1.5 rounded-full" style={{ background: "rgba(6,18,30,.8)", border: "1px solid rgba(232,184,75,.45)" }}>
            <SkullCross size={15} color="#E8B84B" />
            <span className="font-cinzel font-bold text-[10px] tracking-widest" style={{ color: "#E8B84B", writingMode: "vertical-rl" }}>TOUR {state.turnNumber}</span>
          </div>
          <div className="vs-line flex-1 w-px" />
        </div>

        <section className="flex-1 min-w-0 flex items-center justify-center">{renderShip(aiPlayer, false)}</section>
      </main>

      {/* FOOTER — hand + actions */}
      <footer className="shrink-0 px-3 pb-2 pt-1 flex flex-col gap-1.5" style={{ background: "linear-gradient(0deg,rgba(6,9,14,.75),transparent)" }}>
        <div className="flex items-end gap-3">
          {/* Hand */}
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2 mb-1">
              <span className="font-oswald text-[10px] uppercase tracking-widest text-white/45">Main</span>
              <span className="font-oswald text-[11px] font-bold text-white/60">{player.hand.length}</span>
              <div className="flex-1 h-px bg-white/10" />
            </div>
            <div className="flex gap-2 overflow-x-auto pb-1 items-end" style={{ minHeight: "170px" }}>
              {player.hand.map((id) => {
                const card = state.cards[id];
                if (!card) return null;
                const def = getCardDef(card.defId);
                const canPlay = validActions.some((a) => {
                  if ("instanceId" in a && a.instanceId === id) return true;
                  if ("objectInstanceId" in a && a.objectInstanceId === id) return true;
                  return false;
                });
                const dragType = def.type === "character" || def.type === "object";
                return (
                  <Card
                    key={id}
                    instance={card}
                    def={def}
                    width={118}
                    selected={selectedHandCard === id}
                    highlight={canPlay}
                    draggable={canPlay && dragType}
                    onDragStart={(e) => handleHandDragStart(e, id)}
                    onClick={() => {
                      if (canPlay) handleHandCardClick(id);
                      else setUiMode({ type: "cardDetail", defId: card.defId, instanceId: id });
                    }}
                  />
                );
              })}
              {player.hand.length === 0 && <div className="font-spectral italic text-white/30 text-sm px-2">Main vide</div>}
            </div>
          </div>

          {/* Actions */}
          <div className="shrink-0 w-40 flex flex-col gap-1.5">
            <div className="flex items-center justify-between font-oswald text-[10px] text-white/50 px-1">
              <span>Deck</span><span className="font-bold text-white/70">{player.deck.length}</span>
            </div>
            {canActivateShip && player.activeShip && (
              <button onClick={() => dispatch({ type: "activateShip", shipInstanceId: player.activeShip! })}
                className="action-btn font-oswald px-3 py-2 bg-cyan-700/60 hover:bg-cyan-600/60 rounded-xl text-xs font-bold transition-all">⚓ Activer navire</button>
            )}
            {canFlip && (
              <button onClick={() => setUiMode({ type: "selectingCaptainSlot" })}
                className="action-btn font-oswald px-3 py-2 bg-red-700/60 hover:bg-red-600/60 rounded-xl text-xs font-bold transition-all">⚔ Engager Capitaine</button>
            )}
            {canKingHaki && (
              <button onClick={() => dispatch({ type: "useHaki", hakiType: "king" })}
                className="action-btn font-oswald px-3 py-2 bg-amber-600/70 hover:bg-amber-500/70 rounded-xl text-xs font-bold transition-all">👑 Haki des Rois</button>
            )}
            <button onClick={() => { dispatch({ type: "endTurn" }); resetUI(); }} disabled={isAiTurn || inCounterWindow}
              className="action-btn gold-surface font-oswald px-3 py-3 rounded-xl text-sm font-bold disabled:opacity-30 disabled:cursor-not-allowed transition-all">Fin de tour ➡</button>
            {uiMode.type !== "idle" && (
              <button onClick={resetUI} className="font-oswald px-3 py-1.5 bg-white/8 hover:bg-white/15 rounded-lg text-xs text-white/55 transition-all">✕ Annuler</button>
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

      {uiMode.type === "cardDetail" && (() => {
        const def = getCardDef(uiMode.defId);
        const inst = uiMode.instanceId ? state.cards[uiMode.instanceId] : undefined;
        return <CardDetail def={def} instance={inst} state={state} onClose={resetUI} originRect={zoomFromRef.current} />;
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
