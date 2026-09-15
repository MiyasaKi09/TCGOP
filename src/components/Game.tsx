"use client";

import { useState, useMemo, useRef, useEffect, useCallback } from "react";
import type { DragEvent } from "react";
import type { GameAction, Slot, PlayerId, DeckDef } from "@/types";
import { useGameEngine } from "@/hooks/useGameEngine";
import { getCardDef, getCaptainDef } from "@/engine/cardRegistry";
import { initializeRegistry } from "@/engine/init";
import BoardSlot from "./BoardSlot";
import Card from "./Card";
import CardDetail from "./CardDetail";
import ActionMenu from "./ActionMenu";
import CaptainMenu from "./CaptainMenu";
import ShipMenu from "./ShipMenu";
import FullCard from "./FullCard";
import EventConfirm from "./EventConfirm";
import HakiBar from "./HakiBar";
import HelpPanel from "./HelpPanel";
import CombatVfxLayer from "./CombatVfxLayer";
import PlayRevealLayer from "./PlayRevealLayer";
import VfxStage from "./vfx/VfxStage";
import CutInLayer from "./vfx/CutInLayer";
import AmbientStage from "./vfx/AmbientStage";
import StatusBadges, { StatusLegend } from "./StatusBadges";
import { useCombatVfx } from "@/lib/useCombatVfx";
import type { Difficulty } from "@/engine/ai";
import { FRONT_SLOTS, BACK_SLOTS } from "@/engine/utils";
import { faction, hpColor, CARD_ART, CARD_ART_VERSO } from "@/data/cardArt";
import { Crest } from "./icons";

initializeRegistry();

interface GameProps {
  playerDeck: DeckDef;
  aiDeck: DeckDef;
  difficulty?: Difficulty;
}

type UIMode =
  | { type: "idle" }
  | { type: "selectingSlot"; cardId: string }
  /** `surcharge` (decision §8.34(b)) aims the captain's `useSurcharge` instead
   *  of a `captainAttack`; `isSpecial` picks the captain's ★ special attack. */
  | { type: "selectingTarget"; attackerId: string; isSpecial: boolean; fruitInstanceId?: string; surcharge?: boolean }
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
  const { state, validActions, dispatch, isAiTurn, humanPlayer, announcements, dismissAnnouncement, notice } =
    useGameEngine(playerDeck, aiDeck, "player1", difficulty);
  const [uiMode, setUiMode] = useState<UIMode>({ type: "idle" });
  const [showHelp, setShowHelp] = useState(false);
  const [logExpanded, setLogExpanded] = useState(false);
  const [selectedHandCard, setSelectedHandCard] = useState<string | null>(null);
  const [hoveredHand, setHoveredHand] = useState<{ id: string; rect: DOMRect } | null>(null);
  const [hoveredUnit, setHoveredUnit] = useState<{ id: string; rect: DOMRect } | null>(null);
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
    // Decision §8.28 (follow-up): an awakened fruit worn by the captain fires a
    // `fruitSpecialAttack` whose attacker is the synthetic `captain_{id}`.
    if (uiMode.fruitInstanceId) {
      const fruitId = uiMode.fruitInstanceId;
      for (const a of validActions) {
        if (a.type !== "fruitSpecialAttack") continue;
        if (a.attackerInstanceId !== uiMode.attackerId || a.fruitInstanceId !== fruitId) continue;
        if (a.targetIsCaptain) targets.add(`captain_${aiPlayer}`);
        else targets.add(a.targetInstanceId);
      }
      return targets;
    }
    const isCaptainAttack = uiMode.attackerId.startsWith("captain_");
    // Decision §8.34: the captain's base attack, its ★ special (same action,
    // `isSpecial`) and its `surcharge` are three separate aims.
    const actionType = isCaptainAttack
      ? (uiMode.surcharge ? "useSurcharge" : "captainAttack")
      : (uiMode.isSpecial ? "specialAttack" : "baseAttack");
    for (const a of validActions) {
      if (a.type === actionType) {
        if (isCaptainAttack && a.type === "useSurcharge") {
          if (a.targetIsCaptain) targets.add(`captain_${aiPlayer}`);
          else targets.add(a.targetInstanceId);
        } else if (isCaptainAttack && a.type === "captainAttack") {
          if (!!a.isSpecial !== uiMode.isSpecial) continue;
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
    if (uiMode.fruitInstanceId) {
      const fruit = state.cards[uiMode.fruitInstanceId];
      if (!fruit) return false;
      const spec = getCardDef(fruit.defId).fruitEffects?.awakening?.specialAttack;
      return !!spec?.attackTraits?.includes("zone");
    }
    if (attackerId.startsWith("captain_")) {
      const pid = attackerId.replace("captain_", "") as PlayerId;
      const cd = getCaptainDef(state.players[pid].captain.defId);
      const atk = uiMode.surcharge
        ? cd.verso.surcharge
        : isSpecial
          ? cd.verso.specialAttack
          : cd.verso.baseAction;
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
        fireAtTarget(uiMode, targetId, false);
        resetUI();
      }
    }
  };

  // Fire whatever is being aimed (character attack, captain attack, or the
  // captain's awakened-fruit special — decision §8.28 follow-up) at `targetId`.
  const fireAtTarget = (
    mode: Extract<UIMode, { type: "selectingTarget" }>,
    targetId: string,
    targetIsCaptain: boolean
  ) => {
    if (mode.fruitInstanceId) {
      dispatch({
        type: "fruitSpecialAttack",
        attackerInstanceId: mode.attackerId,
        fruitInstanceId: mode.fruitInstanceId,
        targetInstanceId: targetId,
        ...(targetIsCaptain ? { targetIsCaptain: true } : {}),
      } as GameAction);
    } else if (mode.attackerId.startsWith("captain_")) {
      // Decision §8.34: `isSpecial` carries the captain's signature move, and
      // the `surcharge` is its own action.
      dispatch({
        type: mode.surcharge ? "useSurcharge" : "captainAttack",
        targetInstanceId: targetId,
        ...(targetIsCaptain ? { targetIsCaptain: true } : {}),
        ...(!mode.surcharge && mode.isSpecial ? { isSpecial: true } : {}),
      } as GameAction);
    } else {
      dispatch({
        type: mode.isSpecial ? "specialAttack" : "baseAttack",
        attackerInstanceId: mode.attackerId,
        targetInstanceId: targetId,
        ...(targetIsCaptain ? { targetIsCaptain: true } : {}),
      } as GameAction);
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
    // C'est l'appartenance à `attackTargets` qui fait autorité, PAS le camp :
    // une spéciale peut viser un allié (soin, buff) ou son propre lanceur
    // (Monster Block de Chopper). Exiger le camp adverse rendait ces attaques
    // injouables — le clic était simplement avalé.
    if (uiMode.type === "selectingTarget" && attackTargets.has(instanceId)) {
      fireAtTarget(uiMode, instanceId, false);
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
      fireAtTarget(uiMode, `captain_${aiPlayer}`, true);
      resetUI();
    }
  };

  // Click on either side's captain (prow or verso). Attack-targeting takes
  // priority; otherwise open its menu (powers + available actions).
  const onCaptainClick = (playerId: PlayerId) => {
    // Decision §8.28 (follow-up): your own captain is an equip target — the
    // three signature SR Devil Fruits are printed "Équipable sur Luffy /
    // Crocodile / Akainu", names only a captain carries.
    if (
      uiMode.type === "selectingEquipTarget" &&
      playerId === humanPlayer &&
      equipTargets.has(`captain_${playerId}`)
    ) {
      dispatch({
        type: "equipObject",
        objectInstanceId: uiMode.objectId,
        targetInstanceId: `captain_${playerId}`,
        targetIsCaptain: true,
      });
      resetUI();
      return;
    }
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

      // Captain verso occupying this slot (compact, art-first token)
      if (captainSlot === slot) {
        const capDef = getCaptainDef(ps.captain.defId);
        const isTarget = !isPlayerSide && uiMode.type === "selectingTarget" && attackTargets.has(`captain_${playerId}`);
        // Decision §8.28 (follow-up): the captain is an equip target too.
        const isCapEquip = isPlayerSide && uiMode.type === "selectingEquipTarget" && equipTargets.has(`captain_${playerId}`);
        const capGear = ps.captain.attachedObjects ?? [];
        const pvPercent = Math.max(0, (ps.captain.currentPv / capDef.verso.pv) * 100);
        const capArt = CARD_ART_VERSO[capDef.id] ?? CARD_ART[capDef.id];
        return (
          <div
            key={slot}
            data-inst={`captain_${playerId}`}
            onClick={() => onCaptainClick(playerId)}
            onDragOver={(e) => { if (isCapEquip) e.preventDefault(); }}
            onDrop={(e) => { e.preventDefault(); onCaptainClick(playerId); }}
            className={`relative flex-1 min-w-0 max-w-[120px] h-[3.9rem] rounded-xl overflow-hidden cursor-pointer transition-all ${isTarget ? "ring-target" : isCapEquip ? "ring-deploy" : "hover:brightness-110"} ${selecting && !isTarget && !isCapEquip ? "slot-dim" : ""}`}
            style={{ background: "#1a0c0c", boxShadow: "inset 0 0 0 2px var(--color-target)" }}
          >
            {capArt && <div className="absolute inset-0" style={{ backgroundImage: `url('${capArt}')`, backgroundSize: "cover", backgroundPosition: "center 14%" }} />}
            <div className="absolute inset-0" style={{ background: "linear-gradient(180deg,rgba(6,10,20,.1),rgba(6,10,20,.9))" }} />
            <div className="absolute top-0.5 left-1 font-oswald text-[7px] uppercase tracking-widest text-red-200 font-bold">★ Verso</div>
            {capGear.length > 0 && (
              <span className="absolute top-0.5 right-1 font-oswald font-bold text-[9px] px-1 rounded text-gold" style={{ background: "rgba(232,184,75,.25)", border: "1px solid var(--ink-edge)" }}>⚔{capGear.length}</span>
            )}
            {/* Les statuts du Capitaine n'etaient affiches NULLE PART sur le
                plateau : brule, gele, empoisonne ou Inciblable, rien ne le
                disait. Meme pastilles que les personnages. */}
            {ps.captain.statusEffects.length > 0 && (
              <div className="absolute top-0.5 left-1 mt-3">
                <StatusBadges effects={ps.captain.statusEffects} compact />
              </div>
            )}
            <div className="absolute left-1 right-1 bottom-1">
              <div className="font-cinzel text-[10px] font-bold text-white truncate leading-none">{capDef.name}</div>
              <div className="hp-gauge w-full h-1.5 rounded-full mt-1">
                <div className="h-full rounded-full" style={{ width: `${pvPercent}%`, background: hpColor(pvPercent / 100) }} />
              </div>
            </div>
          </div>
        );
      }

      const charId = ps.board[slot];
      const instance = charId ? state.cards[charId] : null;
      const def = instance ? getCardDef(instance.defId) : null;
      const isValidDeploy = isPlayerSide && deploySlots.has(slot);
      // Idem au rendu : une case s'allume si le moteur la propose, quel que
      // soit le camp — sinon une spéciale alliée ou auto-ciblée reste invisible.
      const isValidTarget = (uiMode.type === "selectingTarget" && charId !== null && attackTargets.has(charId!))
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
          onHoverChange={(rect) => {
            if (rect && charId) setHoveredUnit({ id: charId, rect });
            else setHoveredUnit((h) => (h && charId && h.id === charId ? null : h));
          }}
        />
      );
    });
  };

  // The command bar for one player: dedicated Captain card + Ship slot + Volonté.
  const renderCommand = (playerId: PlayerId, isYou: boolean) => {
    const ps = state.players[playerId];
    const capDef = getCaptainDef(ps.captain.defId);
    const side = ps.captain.flipped ? capDef.verso : capDef.recto;
    const ratio = Math.max(0, ps.captain.currentPv / side.pv);
    const hpc = hpColor(ratio);
    const capArt = ps.captain.flipped ? (CARD_ART_VERSO[capDef.id] ?? CARD_ART[capDef.id]) : CARD_ART[capDef.id];
    const capTarget = !isYou && uiMode.type === "selectingTarget" && attackTargets.has(`captain_${playerId}`);
    // Decision §8.28 (follow-up): your own captain wears the fruit printed for
    // it, so the command card is a drop/click target while equipping.
    const capEquip = isYou && uiMode.type === "selectingEquipTarget" && equipTargets.has(`captain_${playerId}`);
    const capGear = ps.captain.attachedObjects ?? [];

    // Captain command card
    const capCard = (
      <div
        data-inst={!ps.captain.flipped ? `captain_${playerId}` : undefined}
        onClick={() => onCaptainClick(playerId)}
        onDragOver={(e) => { if (capEquip) e.preventDefault(); }}
        onDrop={(e) => { e.preventDefault(); onCaptainClick(playerId); }}
        className={`cap-cmd ${isYou ? "" : "foe"} ${capTarget ? "ring-target" : ""} ${capEquip ? "ring-deploy" : ""} ${selecting && !capTarget && !capEquip ? "slot-dim" : ""} ${ps.captain.tapped ? "saturate-50 opacity-80" : ""}`}
      >
        {capArt && <div className="art" style={{ backgroundImage: `url('${capArt}')` }} />}
        <div className="sh" />
        <div className="absolute top-1 right-1.5 flex flex-col items-end gap-0.5 font-oswald font-bold text-[10px]">
          <span className="text-atk">⚔{side.atk}</span>
          <span className="text-def">🛡{side.def}</span>
        </div>
        <div className="in">
          <div className="font-oswald text-[7px] uppercase tracking-widest text-white/70">Capitaine</div>
          <div className="font-cinzel text-[12px] font-bold text-white leading-none truncate">{capDef.name}</div>
          <div className="flex items-center gap-1.5 mt-1">
            <div className="hp-gauge flex-1 h-1.5 rounded-full"><div className="h-full rounded-full" style={{ width: `${ratio * 100}%`, background: hpc }} /></div>
            <span className="font-oswald text-[10px] font-bold" style={{ color: hpc }}>{ps.captain.currentPv}</span>
          </div>
          {/* Idem sur la proue, la ou le Capitaine passe le plus clair de la
              partie : sans cela, « Inciblable jusqu'a la fin du tour » n'etait
              visible que dans le journal. */}
          {ps.captain.statusEffects.length > 0 && (
            <div className="mt-1"><StatusBadges effects={ps.captain.statusEffects} compact /></div>
          )}
          {capGear.length > 0 && (
            <div className="flex flex-wrap gap-1 mt-1">
              {capGear.map((objId) => {
                const obj = state.cards[objId];
                if (!obj) return null;
                const objDef = getCardDef(obj.defId);
                return (
                  <span key={objId} className="font-oswald text-[8px] px-1 py-0.5 rounded text-gold truncate max-w-full" style={{ background: "rgba(232,184,75,.2)", border: "1px solid var(--ink-edge)" }}>
                    {obj.isAwakened ? "⭐" : "⚔"} {objDef.name}
                  </span>
                );
              })}
            </div>
          )}
        </div>
      </div>
    );

    // Ship slot
    const shipSlot = ps.activeShip ? (() => {
      const shipDef = getCardDef(state.cards[ps.activeShip!].defId);
      const shipArt = CARD_ART[shipDef.id];
      return (
        <button onClick={() => setUiMode({ type: "shipMenu", instanceId: ps.activeShip!, isYou })} className="ship-cmd">
          {shipArt && <div className="absolute inset-0" style={{ backgroundImage: `url('${shipArt}')`, backgroundSize: "cover", backgroundPosition: "center" }} />}
          <div className="absolute inset-x-0 bottom-0 text-[8px] text-center text-cyan-100 truncate px-1" style={{ background: "rgba(6,12,24,.8)" }}>⚓ {shipDef.name}</div>
        </button>
      );
    })() : (
      <div className="ship-cmd empty">⚓<br />Navire</div>
    );

    // Volonté + counts
    const pipsOn = Math.min(ps.volonte, 10);
    const pips = Array.from({ length: 10 }, (_, i) => <i key={i} className={`will-pip ${i < pipsOn ? "on" : ""}`} />);
    const res = (
      <div className="flex-1 flex flex-col justify-center gap-1.5 min-w-0">
        {isYou ? (
          <>
            <div className="flex items-center gap-1.5 flex-wrap">
              <span className="font-oswald text-[8px] uppercase tracking-widest text-white/55">Volonté</span>
              <span className="flex gap-[3px]">{pips}</span>
              <span className="font-cinzel font-bold text-[13px] text-gold">{ps.volonte}</span>
            </div>
            <div className="flex gap-2 font-oswald text-[10px] text-white/55">
              <span>✋ {ps.hand.length}</span><span>🂠 {ps.deck.length}</span>
            </div>
            <HakiBar state={state} playerId={playerId} onOpenHelp={() => setShowHelp(true)} />
          </>
        ) : (
          <>
            <HakiBar state={state} playerId={playerId} align="right" onOpenHelp={() => setShowHelp(true)} />
            <div className="flex gap-2 font-oswald text-[10px] text-white/55 justify-end"><span>✋ {ps.hand.length}</span><span>🂠 {ps.deck.length}</span></div>
            <div className="flex items-center gap-1.5 justify-end">
              <span className="font-oswald text-[8px] uppercase tracking-widest text-white/55">Volonté</span>
              <span className="font-cinzel font-bold text-[13px] text-gold">{ps.volonte}</span>
            </div>
          </>
        )}
      </div>
    );

    return <div className="g-cmd">{capCard}{shipSlot}{res}</div>;
  };

  // One player's half of the single, flat battlefield: ship deck + 2 lines + command.
  const renderHalf = (playerId: PlayerId, isYou: boolean) => {
    const capDef = getCaptainDef(state.players[playerId].captain.defId);
    const fac = faction(capDef.faction);
    const front = <div className="g-row">{renderLine(FRONT_SLOTS, playerId)}</div>;
    const back = <div className="g-row">{renderLine(BACK_SLOTS, playerId)}</div>;
    const lab = (t: string) => <div className="g-rowlab">{t}</div>;

    return (
      <section className="g-half">
        <div className={`g-floor ${isYou ? "flip" : ""}`} style={{ backgroundImage: `url('${fac.shipImg}')` }} />
        <div className={`g-shade ${isYou ? "you" : "foe"}`} />
        {isYou ? (
          <>
            {lab("Ligne avant")}{front}
            {lab("Ligne arrière")}{back}
            {renderCommand(playerId, true)}
          </>
        ) : (
          <>
            {renderCommand(playerId, false)}
            {lab("Ligne arrière")}{back}
            {lab("Ligne avant")}{front}
          </>
        )}
      </section>
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
        <div className="panel halftone p-6 max-w-lg animate-modal-enter" style={{ boxShadow: "inset 0 0 0 1.5px rgba(224,70,63,.55), var(--shadow-modal)" }}>
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
                  <button key={i} onClick={() => dispatch(action)} className="btn action-btn px-4 py-2.5 text-sm" style={{ background: "linear-gradient(180deg,#3b82f6,#1d4ed8)", color: "#fff", boxShadow: "0 4px 0 #143a8a, 0 8px 18px rgba(0,0,0,.4)" }}>
                    🛡 {def.name} <span className="text-blue-100/80 text-xs">({def.cost}V)</span>
                  </button>
                );
              }
              if (action.type === "useShield") {
                const blockerDef = getCardDef(state.cards[action.blockerInstanceId].defId);
                return (
                  <button key={i} onClick={() => dispatch(action)} className="btn btn-gold action-btn px-4 py-2.5 text-sm">
                    🛡 Bloquer ({blockerDef.name})
                  </button>
                );
              }
              if (action.type === "useHaki") {
                return (
                  <button key={i} onClick={() => dispatch(action)} className="btn action-btn px-4 py-2.5 text-sm" style={{ background: "linear-gradient(180deg,#a855f7,#7c3aed)", color: "#fff", boxShadow: "0 4px 0 #4c1d95, 0 8px 18px rgba(0,0,0,.4)" }}>
                    👁 Haki Observation
                  </button>
                );
              }
              if (action.type === "passCounter") {
                return (
                  <button key={i} onClick={() => dispatch(action)} className="btn btn-ghost action-btn px-4 py-2.5 text-sm">
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
        <div className="speed-lines px-16 py-6">
          <div className={`font-cinzel text-6xl font-black tracking-tight ${won ? "text-green-400" : "text-red-400"}`} style={{ textShadow: "0 3px 0 var(--ink-edge), 0 4px 24px rgba(0,0,0,.7)" }}>
            {won ? "VICTOIRE !" : "DÉFAITE…"}
          </div>
        </div>
        <div className="font-oswald text-white/50 text-lg uppercase tracking-widest">Tour {state.turnNumber}</div>
        <button onClick={() => window.location.reload()} className="btn btn-gold action-btn px-10 py-4 text-lg">
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
    if (uiMode.type === "selectingEquipTarget") {
      const onCaptain = equipTargets.has(`captain_${humanPlayer}`);
      return { text: onCaptain ? "Équipez votre Capitaine" : "Équipez un personnage", color: "text-amber-300", pulse: true };
    }
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
      {/* Manga screentone + ink vignette over the sea */}
      <div className="manga-atmos" />

      {/* HEADER */}
      <header className="halftone relative z-10 shrink-0 flex items-center gap-3 px-4 py-2" style={{ borderBottom: "2px solid var(--ink-edge)", boxShadow: "0 1px 0 rgba(232,184,75,.25)", background: "linear-gradient(180deg,rgba(8,12,18,.92),rgba(6,9,14,.62))" }}>
        <span className="font-cinzel font-extrabold text-[16px] tracking-wider text-gold">TCGOP</span>
        <div className="flex items-center gap-1.5 ml-1">
          <span className="turn-ring">{state.turnNumber}</span>
          <span className="font-oswald text-[8px] uppercase tracking-[.18em] text-white/45 hidden sm:inline">Tour</span>
        </div>
        <div className={`ml-auto status-tag ${statusText.color} ${statusText.pulse ? "animate-pulse" : ""}`}>
          <span className="dot" />{statusText.text}
        </div>
        <button
          onClick={() => setShowHelp(true)}
          title="Règles : Haki, fenêtre de contre, Volonté"
          className="btn btn-ghost action-btn px-2.5 py-1 text-[11px] ml-2"
        >
          ? Règles
        </button>
        <div className="flex items-center gap-2 ml-3 pl-3" style={{ borderLeft: "1px solid rgba(255,255,255,.1)" }}>
          <Crest which={foeFac.crest} size={13} color={foeFac.accent} />
          <span className="font-oswald text-[11px] text-white/55">Main {opponent.hand.length} · Deck {opponent.deck.length}</span>
          {opponent.activeShip && (
            <button onClick={() => setUiMode({ type: "cardDetail", defId: state.cards[opponent.activeShip!].defId, instanceId: opponent.activeShip! })}
              className="font-oswald text-[10px] text-cyan-200/80 bg-cyan-900/30 rounded-md px-1.5 py-0.5 hover:bg-cyan-800/40 transition-all truncate max-w-[120px]" style={{ border: "1.5px solid var(--ink-edge)" }}>
              ⚓ {getCardDef(state.cards[opponent.activeShip].defId).name}
            </button>
          )}
        </div>
      </header>

      {/* Combat VFX + play-reveal overlays */}
      <VfxStage onActiveChange={onVfxActiveChange} />
      <CombatVfxLayer events={vfxEvents} remove={removeVfx} webglActive={webglActive} />
      <CutInLayer />
      <PlayRevealLayer announcements={announcements} dismiss={dismissAnnouncement} />

      {/* BOARD — single flat terrain: foe deck on top, your deck below, cut at the waterline */}
      <main ref={boardRef} className="relative z-10 flex-1 min-h-0 flex flex-col overflow-hidden">
        {renderHalf(aiPlayer, false)}
        <div className="waterline" />
        {renderHalf(humanPlayer, true)}
      </main>

      {/* FOOTER — hand + actions */}
      <footer className="relative z-10 shrink-0 px-3 pb-2 pt-1 flex flex-col gap-1.5" style={{ background: "linear-gradient(0deg,rgba(6,9,14,.8),transparent)", borderTop: "2px solid var(--ink-edge)", boxShadow: "0 -1px 0 rgba(232,184,75,.2)" }}>
        <div className="flex items-end gap-3">
          {/* Hand */}
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2 mb-1">
              <span className="font-oswald text-[10px] uppercase tracking-widest text-white/45">Main</span>
              <span className="font-oswald text-[11px] font-bold text-white/60">{player.hand.length}</span>
              <div className="hidden md:block"><StatusLegend types={["freeze", "burn", "poison", "immobilize", "desiccation", "untargetable"]} /></div>
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

          {/* Actions */}
          <div className="shrink-0 w-40 flex flex-col gap-1.5">
            <div className="flex items-center justify-between font-oswald text-[10px] text-white/50 px-1">
              <span>Deck</span><span className="font-bold text-white/70">{player.deck.length}</span>
            </div>
            {(canFlip || canActivateShip || canKingHaki) && (
              <div className="font-oswald text-[9px] text-amber-300/90 leading-snug px-2 py-1.5 rounded-lg flex flex-col gap-0.5" style={{ background: "rgba(232,184,75,.12)", border: "2px solid var(--ink-edge)", boxShadow: "inset 0 0 0 1px rgba(232,184,75,.3)" }}>
                {canFlip && <span>⚔ Clique ton Capitaine pour l&apos;engager</span>}
                {canKingHaki && <span>👑 Haki des Rois dispo (Capitaine)</span>}
                {canActivateShip && <span>⚓ Clique ton Navire pour l&apos;activer</span>}
              </div>
            )}
            <button onClick={() => { dispatch({ type: "endTurn" }); resetUI(); }} disabled={isAiTurn || inCounterWindow}
              className="btn btn-gold action-btn px-3 py-3 text-sm">Fin de tour ➡</button>
            {uiMode.type !== "idle" && (
              <button onClick={resetUI} className="btn btn-ghost action-btn px-3 py-1.5 text-xs">✕ Annuler</button>
            )}
          </div>
        </div>

        {/* Journal — repliable : lisible par défaut, déroulable pour reconstituer une manche */}
        <div className="halftone rounded-lg" style={{ background: "rgba(8,12,18,.7)", border: "2px solid var(--ink-edge)", boxShadow: "inset 0 0 0 1px rgba(255,255,255,.05)" }}>
          <button
            onClick={() => setLogExpanded((v) => !v)}
            className="w-full flex items-center gap-2 px-2.5 pt-1 pb-0.5 text-left"
            style={{ background: "none", border: 0, cursor: "pointer" }}
          >
            <span className="font-oswald text-[8px] uppercase tracking-widest text-white/40">Journal</span>
            <span className="font-oswald text-[9px] text-white/30 ml-auto">
              {logExpanded ? "Replier ▲" : "Tout voir ▼"}
            </span>
          </button>
          <div className="px-2.5 pb-1.5 overflow-y-auto" style={{ maxHeight: logExpanded ? "38vh" : "92px" }}>
            {state.log.slice(logExpanded ? 0 : -14).reverse().map((entry, i) => {
              const mine = entry.player === humanPlayer;
              return (
                <div
                  key={`${entry.turn}-${i}-${entry.message}`}
                  className={`font-spectral py-0.5 text-xs leading-snug ${i === 0 && !logExpanded ? "text-white/90" : "text-white/55"}`}
                  style={i === 0 && !logExpanded ? { borderLeft: "2px solid var(--gold)", paddingLeft: 6, marginLeft: -6 } : undefined}
                >
                  <span className="font-mono text-[10px] text-white/30">M{entry.turn}</span>{" "}
                  <span
                    className="font-oswald text-[10px] font-bold"
                    style={{ color: mine ? "var(--color-deploy, #5BC46A)" : "var(--color-target, #E0463F)" }}
                    title={mine ? "Toi" : "Adversaire"}
                  >
                    {mine ? "► Toi" : "◄ Adv"}
                  </span>{" "}
                  {entry.message}
                </div>
              );
            })}
          </div>
        </div>
      </footer>

      {/* Overlays */}
      {renderCounterWindow()}

      {showHelp && <HelpPanel state={state} humanPlayer={humanPlayer} onClose={() => setShowHelp(false)} />}

      {/* Équipement porté, révélé EN IMAGE au survol, à côté de l'unité : le
          jeton ne montre qu'un compteur, donc l'effet d'un Fruit ou d'une arme
          était invisible sans ouvrir un menu. */}
      {hoveredUnit && uiMode.type === "idle" && !inCounterWindow && (() => {
        const unit = state.cards[hoveredUnit.id];
        if (!unit || unit.attachedObjects.length === 0) return null;
        const PW = 168;
        const vw = typeof window !== "undefined" ? window.innerWidth : 1600;
        const vh = typeof window !== "undefined" ? window.innerHeight : 900;
        const r = hoveredUnit.rect;
        // À droite de la case, sinon à gauche quand le bord est trop proche.
        const left = r.right + 10 + PW < vw ? r.right + 10 : Math.max(8, r.left - PW - 10);
        const height = unit.attachedObjects.length * 236;
        const top = Math.min(Math.max(8, r.top + r.height / 2 - height / 2), Math.max(8, vh - height - 8));
        return (
          <div className="fixed z-40 pointer-events-none flex flex-col gap-2 animate-fade-in" style={{ left, top }}>
            {unit.attachedObjects.map((objId) => {
              const obj = state.cards[objId];
              if (!obj) return null;
              let objDef;
              try { objDef = getCardDef(obj.defId); } catch { return null; }
              return (
                <div key={objId} className="relative">
                  <FullCard def={objDef} instance={obj} state={state} width={PW} />
                  {obj.isAwakened && (
                    <span
                      className="absolute top-1 left-1 font-oswald font-bold text-[9px] px-1.5 rounded text-gold"
                      style={{ background: "rgba(8,12,18,.85)", border: "1px solid var(--gold)" }}
                    >
                      ⭐ Éveillé
                    </span>
                  )}
                </div>
              );
            })}
          </div>
        );
      })()}

      {/* Refus du moteur : dire pourquoi, plutôt que de ne rien faire. */}
      {notice && (
        <div
          className="fixed left-1/2 bottom-[22%] -translate-x-1/2 z-[60] panel halftone px-4 py-2 animate-fade-in pointer-events-none"
          style={{ boxShadow: "inset 0 0 0 1.5px rgba(224,70,63,.55), var(--shadow-modal)" }}
          role="status"
        >
          <span className="font-oswald text-[11px] uppercase tracking-wider text-red-300">Action refusée</span>
          <div className="font-spectral text-sm text-white/85">{notice}</div>
        </div>
      )}

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
            onAwakenFruit={(fruitInstanceId) => { dispatch({ type: "awakenFruit", fruitInstanceId }); resetUI(); }}
            onFruitSpecial={(fruitInstanceId) =>
              setUiMode({ type: "selectingTarget", attackerId: uiMode.instanceId, isSpecial: true, fruitInstanceId })
            }
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
            onSpecialAttack={() => setUiMode({ type: "selectingTarget", attackerId: `captain_${humanPlayer}`, isSpecial: true })}
            onSurcharge={() => setUiMode({ type: "selectingTarget", attackerId: `captain_${humanPlayer}`, isSpecial: true, surcharge: true })}
            /* Decision §8.28 (follow-up): the fruit the captain wears awakens
               and fires from the captain's own menu. */
            onAwakenFruit={(fruitInstanceId) => { dispatch({ type: "awakenFruit", fruitInstanceId }); resetUI(); }}
            onFruitSpecial={(fruitInstanceId) =>
              setUiMode({ type: "selectingTarget", attackerId: `captain_${humanPlayer}`, isSpecial: true, fruitInstanceId })
            }
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
