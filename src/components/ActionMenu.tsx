"use client";

import type { CardDef, CardInstance, GameState, GameAction } from "@/types";
import { getEffectiveAtk } from "@/engine/board";
import { getCardDef } from "@/engine/cardRegistry";
import FullCard, { type CardActions } from "./FullCard";

interface ActionMenuProps {
  instance: CardInstance;
  def: CardDef;
  state: GameState;
  validActions: GameAction[];
  onBaseAttack: () => void;
  onSpecialAttack: () => void;
  onSupportAction: () => void;
  onViewDetail: () => void;
  onClose: () => void;
}

export default function ActionMenu({
  instance, def, state, validActions,
  onBaseAttack, onSpecialAttack, onSupportAction, onViewDetail, onClose,
}: ActionMenuProps) {
  const effectiveAtk = getEffectiveAtk(state, instance.instanceId);

  const canBaseAttack = validActions.some((a) => a.type === "baseAttack" && "attackerInstanceId" in a && a.attackerInstanceId === instance.instanceId);
  const canSpecialAttack = validActions.some((a) => a.type === "specialAttack" && "attackerInstanceId" in a && a.attackerInstanceId === instance.instanceId);
  const canSupport = validActions.some((a) => a.type === "baseSupportAction" && a.instanceId === instance.instanceId);

  const isTapped = instance.tapped;
  const usedBase = instance.usedBaseAction;
  const usedSpecial = instance.usedSpecialAttack;
  const hasSickness = instance.deployedTurn === state.turnNumber && !(def.traits?.includes("rush"));
  const isFrozen = instance.statusEffects.some((e) => e.type === "freeze");
  const isImmobilized = instance.statusEffects.some((e) => e.type === "immobilize");
  const playerVol = state.players[instance.owner].volonte;
  const base = def.baseAction;
  const isSupport = base?.isSupport;

  const baseReason = (() => {
    if (isFrozen) return "Gelé !";
    if (isImmobilized) return "Immobilisé !";
    if (hasSickness) return "Mal de terre";
    if (isTapped) return "Incliné";
    if (usedBase) return "Déjà utilisé ce tour";
    if (effectiveAtk <= 0 && !base?.isSupport) return "ATK 0";
    if (!canBaseAttack && !canSupport) return "Pas de cible";
    return null;
  })();

  const specReason = (() => {
    if (isFrozen) return "Gelé !";
    if (isImmobilized) return "Immobilisé !";
    if (hasSickness) return "Mal de terre";
    if (usedSpecial) return "Déjà utilisé ce tour";
    if (def.specialAttack?.oncePerGame && instance.usedOnceAbilities.includes(def.specialAttack.name)) return "Déjà utilisé (1x/partie)";
    if (def.specialAttack && playerVol < def.specialAttack.cost) return `Volonté insuffisante (${playerVol}/${def.specialAttack.cost})`;
    if (!canSpecialAttack) return "Pas de cible";
    return null;
  })();

  const actions: CardActions = {
    base: base ? { onClick: isSupport && canSupport ? onSupportAction : onBaseAttack, disabled: !(canBaseAttack || canSupport), reason: baseReason } : undefined,
    special: def.specialAttack ? { onClick: onSpecialAttack, disabled: !canSpecialAttack, reason: specReason } : undefined,
  };

  return (
    <div className="fixed inset-0 bg-black/75 backdrop-blur-sm flex items-center justify-center z-40 p-4" onClick={onClose} onContextMenu={(e) => e.preventDefault()}>
      <div className="rounded-2xl p-4 flex flex-col gap-3 animate-modal-enter" style={{ background: "rgba(10,14,20,.94)", boxShadow: "inset 0 0 0 1px rgba(232,184,75,.3), 0 24px 70px rgba(0,0,0,.6)", userSelect: "none" }} onClick={(e) => e.stopPropagation()}>
        <div className="flex items-center justify-between">
          <span className="font-oswald text-[10px] uppercase tracking-widest text-white/45">Cliquez une attaque sur la carte</span>
          <span className="font-oswald font-bold text-sm" style={{ color: "#E8B84B" }}>{playerVol} Vol.</span>
        </div>

        <FullCard def={def} instance={instance} state={state} width={360} actions={actions} />

        {(isTapped || hasSickness || isFrozen || isImmobilized) && (
          <div className="flex gap-1.5 flex-wrap">
            {isTapped && <span className="font-oswald text-[9px] px-2 py-0.5 rounded-full bg-white/10 text-white/70">Incliné</span>}
            {hasSickness && <span className="font-oswald text-[9px] px-2 py-0.5 rounded-full bg-yellow-800/40 text-yellow-300">Mal de terre</span>}
            {isFrozen && <span className="font-oswald text-[9px] px-2 py-0.5 rounded-full bg-cyan-800/40 text-cyan-300">Gelé</span>}
            {isImmobilized && <span className="font-oswald text-[9px] px-2 py-0.5 rounded-full bg-pink-800/40 text-pink-300">Immobilisé</span>}
          </div>
        )}

        {instance.attachedObjects.length > 0 && (
          <div className="rounded-xl p-2" style={{ background: "rgba(232,184,75,.08)" }}>
            <div className="font-oswald text-[9px] uppercase tracking-wider text-amber-400/70 font-bold mb-1">Équipement</div>
            {instance.attachedObjects.map((objId) => {
              const obj = state.cards[objId];
              if (!obj) return null;
              const objDef = getCardDef(obj.defId);
              return <div key={objId} className="font-spectral text-[11px] text-amber-200/85">⚔ {objDef.name}{objDef.bonusAtk ? ` +${objDef.bonusAtk} ATK` : ""}{objDef.bonusDef ? ` +${objDef.bonusDef} DEF` : ""}</div>;
            })}
          </div>
        )}

        <div className="flex gap-2">
          <button onClick={onViewDetail} className="action-btn flex-1 py-2 rounded-xl text-xs text-white/70 transition-all" style={{ background: "rgba(255,255,255,.06)" }}>Détails</button>
          <button onClick={onClose} className="flex-1 py-2 rounded-xl text-xs text-white/50 transition-all" style={{ background: "rgba(255,255,255,.04)" }}>Fermer</button>
        </div>
      </div>
    </div>
  );
}
