"use client";

import type { CardDef, CardInstance, GameState, GameAction } from "@/types";
import { getEffectiveAtk } from "@/engine/board";
import { getCardDef } from "@/engine/cardRegistry";
import FullCard from "./FullCard";

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
  const hasSpecial = !!def.specialAttack;
  const base = def.baseAction;
  const isSupport = base?.isSupport;

  const baseDisabledReason = (() => {
    if (isFrozen) return "Gelé !";
    if (isImmobilized) return "Immobilisé !";
    if (hasSickness) return "Mal de terre (déployé ce tour)";
    if (isTapped) return "Personnage incliné";
    if (usedBase) return "Déjà utilisé ce tour";
    if (effectiveAtk <= 0 && !base?.isSupport) return "ATK 0 — ne peut pas attaquer";
    if (!canBaseAttack && !canSupport) return "Pas de cible valide";
    return null;
  })();

  const specDisabledReason = (() => {
    if (!hasSpecial) return null;
    if (isFrozen) return "Gelé !";
    if (isImmobilized) return "Immobilisé !";
    if (hasSickness) return "Mal de terre (déployé ce tour)";
    if (usedSpecial) return "Déjà utilisé ce tour";
    if (def.specialAttack!.oncePerGame && instance.usedOnceAbilities.includes(def.specialAttack!.name)) return "Déjà utilisé (1x/partie)";
    if (playerVol < def.specialAttack!.cost) return `Volonté insuffisante (${playerVol}/${def.specialAttack!.cost})`;
    if (!canSpecialAttack) return "Pas de cible valide";
    return null;
  })();

  const baseEnabled = canBaseAttack || canSupport;

  return (
    <div className="fixed inset-0 bg-black/75 backdrop-blur-sm flex items-center justify-center z-40 p-4" onClick={onClose}>
      <div className="rounded-2xl p-4 flex gap-4 max-w-[560px] animate-modal-enter" style={{ background: "rgba(10,14,20,.94)", boxShadow: "inset 0 0 0 1px rgba(232,184,75,.3), 0 24px 70px rgba(0,0,0,.6)" }} onClick={(e) => e.stopPropagation()}>
        {/* Real card */}
        <FullCard def={def} instance={instance} state={state} width={196} />

        {/* Actions */}
        <div className="flex flex-col gap-2 w-[268px]">
          <div className="flex items-center justify-between">
            <div className="font-oswald text-[10px] uppercase tracking-widest text-white/45">Actions</div>
            <div className="flex items-center gap-1.5">
              <span className="w-1.5 h-1.5 rounded-full" style={{ background: "#E8B84B" }} />
              <span className="font-oswald font-bold text-sm" style={{ color: "#E8B84B" }}>{playerVol} Vol.</span>
            </div>
          </div>

          {(isTapped || hasSickness || isFrozen || isImmobilized) && (
            <div className="flex gap-1.5 flex-wrap">
              {isTapped && <span className="font-oswald text-[9px] px-2 py-0.5 rounded-full bg-white/10 text-white/70">Incliné</span>}
              {hasSickness && <span className="font-oswald text-[9px] px-2 py-0.5 rounded-full bg-yellow-800/40 text-yellow-300">Mal de terre</span>}
              {isFrozen && <span className="font-oswald text-[9px] px-2 py-0.5 rounded-full bg-cyan-800/40 text-cyan-300 animate-status-freeze">Gelé</span>}
              {isImmobilized && <span className="font-oswald text-[9px] px-2 py-0.5 rounded-full bg-pink-800/40 text-pink-300">Immobilisé</span>}
            </div>
          )}

          {/* Base / Support */}
          <button
            onClick={baseEnabled ? (isSupport && canSupport ? onSupportAction : onBaseAttack) : undefined}
            disabled={!baseEnabled}
            className={`action-btn w-full text-left p-3 rounded-xl border transition-all ${baseEnabled ? "border-green-600/40 bg-green-950/25 hover:bg-green-900/35 cursor-pointer" : "border-white/10 bg-white/5 opacity-50 cursor-not-allowed"}`}
          >
            <div className="flex justify-between items-center">
              <span className="font-spectral font-semibold text-sm text-green-300" style={{ fontVariant: "small-caps" }}>{isSupport ? "✦" : "⚔"} {base?.name ?? "Attaque de base"}</span>
              <span className="font-oswald text-[9px] px-2 py-0.5 rounded-full bg-green-800/60 text-green-100">Gratuit</span>
            </div>
            {!isSupport && (
              <div className="font-oswald text-xs text-white/60 mt-1">
                <span className="font-bold" style={{ color: "#FF7062" }}>{effectiveAtk}</span> ATK
                {base?.element && <span className="ml-1.5 text-amber-300/80">({base.element})</span>}
              </div>
            )}
            {isSupport && base?.description && <div className="font-spectral italic text-[11px] text-cyan-200/80 mt-1">{base.description}</div>}
            {baseDisabledReason && <div className="font-oswald text-[10px] text-red-400/80 mt-1">• {baseDisabledReason}</div>}
          </button>

          {/* Special */}
          {hasSpecial && (
            <button
              onClick={canSpecialAttack ? onSpecialAttack : undefined}
              disabled={!canSpecialAttack}
              className={`action-btn w-full text-left p-3 rounded-xl border transition-all ${canSpecialAttack ? "border-red-600/40 bg-red-950/25 hover:bg-red-900/35 cursor-pointer" : "border-white/10 bg-white/5 opacity-50 cursor-not-allowed"}`}
            >
              <div className="flex justify-between items-center">
                <span className="font-spectral font-semibold text-sm text-red-300" style={{ fontVariant: "small-caps" }}>★ {def.specialAttack!.name}</span>
                <span className="font-oswald text-[9px] px-2 py-0.5 rounded-full bg-red-800/60 text-red-100">{def.specialAttack!.cost} Vol.</span>
              </div>
              <div className="font-oswald text-xs text-white/60 mt-1">
                <span className="font-bold" style={{ color: "#E8B84B" }}>{effectiveAtk + def.specialAttack!.atkBonus}</span> ATK
                {def.specialAttack!.element && <span className="ml-1.5 text-amber-300/80">({def.specialAttack!.element})</span>}
                {def.specialAttack!.oncePerGame && <span className="ml-1.5 text-yellow-300/80">1x/partie</span>}
              </div>
              {specDisabledReason && <div className="font-oswald text-[10px] text-red-400/80 mt-1">• {specDisabledReason}</div>}
            </button>
          )}

          {/* Equipment */}
          {instance.attachedObjects.length > 0 && (
            <div className="p-2 rounded-xl" style={{ background: "rgba(232,184,75,.08)" }}>
              <div className="font-oswald text-[9px] uppercase tracking-wider text-amber-400/70 font-bold mb-1">Équipement</div>
              {instance.attachedObjects.map((objId) => {
                const obj = state.cards[objId];
                if (!obj) return null;
                const objDef = getCardDef(obj.defId);
                return (
                  <div key={objId} className="font-spectral text-[11px] text-amber-200/85">
                    ⚔ {objDef.name}{objDef.bonusAtk ? ` +${objDef.bonusAtk} ATK` : ""}{objDef.bonusDef ? ` +${objDef.bonusDef} DEF` : ""}{obj.isAwakened ? " · ÉVEILLÉ" : ""}
                  </div>
                );
              })}
            </div>
          )}

          <div className="flex gap-2 mt-auto">
            <button onClick={onViewDetail} className="action-btn flex-1 py-2 rounded-xl text-xs text-white/70 transition-all" style={{ background: "rgba(255,255,255,.06)" }}>Détails</button>
            <button onClick={onClose} className="flex-1 py-2 rounded-xl text-xs text-white/50 transition-all" style={{ background: "rgba(255,255,255,.04)" }}>Fermer</button>
          </div>
        </div>
      </div>
    </div>
  );
}
