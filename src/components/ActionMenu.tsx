"use client";

import type { CardDef, CardInstance, GameState, GameAction } from "@/types";
import { getEffectiveAtk, getEffectiveDef } from "@/engine/board";

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
  instance,
  def,
  state,
  validActions,
  onBaseAttack,
  onSpecialAttack,
  onSupportAction,
  onViewDetail,
  onClose,
}: ActionMenuProps) {
  const effectiveAtk = getEffectiveAtk(state, instance.instanceId);
  const effectiveDef = getEffectiveDef(state, instance.instanceId);

  const canBaseAttack = validActions.some(
    (a) => a.type === "baseAttack" && "attackerInstanceId" in a && a.attackerInstanceId === instance.instanceId
  );
  const canSpecialAttack = validActions.some(
    (a) => a.type === "specialAttack" && "attackerInstanceId" in a && a.attackerInstanceId === instance.instanceId
  );
  const canSupport = validActions.some(
    (a) => a.type === "baseSupportAction" && a.instanceId === instance.instanceId
  );

  const isTapped = instance.tapped;
  const usedBase = instance.usedBaseAction;
  const usedSpecial = instance.usedSpecialAttack;
  const hasSickness = instance.deployedTurn === state.turnNumber && !(def.traits?.includes("rush"));
  const isFrozen = instance.statusEffects.some((e) => e.type === "freeze");
  const isImmobilized = instance.statusEffects.some((e) => e.type === "immobilize");
  const playerVol = state.players[instance.owner].volonte;
  const hasSpecial = !!def.specialAttack;
  const pvRatio = instance.currentPv / (def.pv ?? 1);

  const baseDisabledReason = (() => {
    if (isFrozen) return "Gele !";
    if (isImmobilized) return "Immobilise !";
    if (hasSickness) return "Mal de terre (deploye ce tour)";
    if (isTapped) return "Personnage incline";
    if (usedBase) return "Deja utilise ce tour";
    if (effectiveAtk <= 0 && !def.baseAction?.isSupport) return "ATK 0 — ne peut pas attaquer";
    if (!canBaseAttack && !canSupport) return "Pas de cible valide";
    return null;
  })();

  const specDisabledReason = (() => {
    if (!hasSpecial) return null;
    if (isFrozen) return "Gele !";
    if (isImmobilized) return "Immobilise !";
    if (hasSickness) return "Mal de terre (deploye ce tour)";
    if (usedSpecial) return "Deja utilise ce tour";
    if (def.specialAttack!.oncePerGame && instance.usedOnceAbilities.includes(def.specialAttack!.name)) {
      return "Deja utilise (1x/partie)";
    }
    if (playerVol < def.specialAttack!.cost) return `Volonte insuffisante (${playerVol}/${def.specialAttack!.cost})`;
    if (!canSpecialAttack) return "Pas de cible valide";
    return null;
  })();

  const baseAction = def.baseAction;
  const isSupport = baseAction?.isSupport;

  return (
    <div className="fixed inset-0 bg-black/70 backdrop-blur-sm flex items-center justify-center z-40" onClick={onClose}>
      <div className="glass rounded-2xl p-5 min-w-[340px] max-w-[440px] border border-gray-600/30 shadow-2xl animate-modal-enter" onClick={(e) => e.stopPropagation()}>
        {/* Header */}
        <div className="mb-4">
          <div className="flex justify-between items-start">
            <div>
              <h3 className="font-bold text-white text-lg leading-tight">{def.name}</h3>
              <div className="text-[10px] text-gray-500 uppercase tracking-wider mt-0.5">
                {def.type}{def.subtype ? ` — ${def.subtype}` : ""}
              </div>
            </div>
            <div className="flex items-center gap-1.5">
              <div className="w-1.5 h-1.5 rounded-full bg-blue-500 shadow-lg shadow-blue-500/50" />
              <span className="text-sm text-blue-400 font-bold stat-badge">{playerVol} Vol.</span>
            </div>
          </div>

          {/* Stats row */}
          <div className="flex gap-2 mt-3">
            <div className="flex-1 text-center py-1.5 rounded-lg bg-red-950/30 border border-red-800/15">
              <div className="text-[8px] text-gray-500 uppercase tracking-wider">ATK</div>
              <div className="text-red-400 font-bold text-sm stat-badge">{effectiveAtk}</div>
            </div>
            <div className="flex-1 text-center py-1.5 rounded-lg bg-blue-950/30 border border-blue-800/15">
              <div className="text-[8px] text-gray-500 uppercase tracking-wider">DEF</div>
              <div className="text-blue-400 font-bold text-sm stat-badge">{effectiveDef}</div>
            </div>
            <div className={`flex-1 text-center py-1.5 rounded-lg border ${pvRatio <= 0.25 ? "bg-red-950/30 border-red-800/15" : "bg-green-950/30 border-green-800/15"}`}>
              <div className="text-[8px] text-gray-500 uppercase tracking-wider">PV</div>
              <div className={`font-bold text-sm stat-badge ${pvRatio <= 0.25 ? "text-red-400" : "text-green-400"}`}>
                {instance.currentPv}<span className="text-gray-600 text-xs">/{def.pv}</span>
              </div>
            </div>
          </div>

          {/* Status tags */}
          {(isTapped || hasSickness || isFrozen || isImmobilized) && (
            <div className="flex gap-1.5 mt-2 flex-wrap">
              {isTapped && <span className="text-[9px] px-2 py-0.5 rounded-full bg-gray-700/60 text-gray-300 border border-gray-600/30">Incline</span>}
              {hasSickness && <span className="text-[9px] px-2 py-0.5 rounded-full bg-yellow-800/40 text-yellow-300 border border-yellow-700/30">Mal de terre</span>}
              {isFrozen && <span className="text-[9px] px-2 py-0.5 rounded-full bg-cyan-800/40 text-cyan-300 border border-cyan-700/30 animate-status-freeze">Gele</span>}
              {isImmobilized && <span className="text-[9px] px-2 py-0.5 rounded-full bg-pink-800/40 text-pink-300 border border-pink-700/30">Immobilise</span>}
            </div>
          )}
        </div>

        {/* Passive */}
        {def.passive && (
          <div className="mb-3 p-2.5 rounded-xl bg-purple-950/15 border border-purple-800/20">
            <div className="text-[9px] font-bold text-purple-400/80 uppercase tracking-wider">Passif — {def.passive.name}</div>
            <div className="text-[10px] text-gray-400 mt-0.5 leading-relaxed">{def.passive.description}</div>
          </div>
        )}

        <div className="flex flex-col gap-2">
          {/* Base Attack / Support Action */}
          <button
            onClick={(canBaseAttack || canSupport) ? (isSupport && canSupport ? onSupportAction : onBaseAttack) : undefined}
            className={`action-btn w-full text-left p-3 rounded-xl border transition-all ${
              (canBaseAttack || canSupport)
                ? "border-green-600/40 bg-green-950/20 hover:bg-green-900/30 cursor-pointer"
                : "border-gray-700/30 bg-gray-800/15 opacity-40 cursor-not-allowed"
            }`}
          >
            <div className="flex justify-between items-center">
              <span className="text-sm font-bold text-green-300">
                {isSupport ? "✦" : "⚔"} {baseAction?.name ?? "Attaque de base"}
              </span>
              <span className="text-[9px] px-2 py-0.5 rounded-full bg-green-800/50 text-green-200 font-bold">Gratuit</span>
            </div>
            {!isSupport && (
              <div className="text-xs text-gray-400 mt-1.5">
                <span className="text-red-400/80 font-bold stat-badge">{effectiveAtk}</span> ATK
                {baseAction?.element && <span className="ml-1.5 text-amber-300/80">({baseAction.element})</span>}
                {baseAction?.attackTraits?.map((t) => (
                  <span key={t} className="ml-1 px-1.5 py-0.5 rounded-full bg-gray-700/40 text-[9px] text-gray-300">{t}</span>
                ))}
              </div>
            )}
            {isSupport && baseAction && (
              <div className="text-[10px] text-cyan-300/80 mt-1">{baseAction.description}</div>
            )}
            {baseDisabledReason && (
              <div className="text-[10px] text-red-400/80 mt-1.5 flex items-center gap-1">
                <span className="w-1 h-1 rounded-full bg-red-400/60" /> {baseDisabledReason}
              </div>
            )}
          </button>

          {/* Special Attack */}
          {hasSpecial && (
            <button
              onClick={canSpecialAttack ? onSpecialAttack : undefined}
              className={`action-btn w-full text-left p-3 rounded-xl border transition-all ${
                canSpecialAttack
                  ? "border-red-600/40 bg-red-950/20 hover:bg-red-900/30 cursor-pointer"
                  : "border-gray-700/30 bg-gray-800/15 opacity-40 cursor-not-allowed"
              }`}
            >
              <div className="flex justify-between items-center">
                <span className="text-sm font-bold text-red-300">💥 {def.specialAttack!.name}</span>
                <span className="text-[9px] px-2 py-0.5 rounded-full bg-red-800/50 text-red-200 font-bold">
                  {def.specialAttack!.cost} Vol.
                </span>
              </div>
              <div className="text-xs text-gray-400 mt-1.5">
                <span className="stat-badge">{effectiveAtk} + {def.specialAttack!.atkBonus} = </span>
                <span className="font-bold text-red-300 stat-badge">{effectiveAtk + def.specialAttack!.atkBonus}</span> ATK
                {def.specialAttack!.element && <span className="ml-1.5 text-amber-300/80">({def.specialAttack!.element})</span>}
                {def.specialAttack!.attackTraits?.map((t) => (
                  <span key={t} className="ml-1 px-1.5 py-0.5 rounded-full bg-gray-700/40 text-[9px] text-gray-300">{t}</span>
                ))}
                {def.specialAttack!.oncePerGame && (
                  <span className="ml-1 px-1.5 py-0.5 rounded-full bg-yellow-800/40 text-[9px] text-yellow-300">1x/partie</span>
                )}
              </div>
              <div className="text-[10px] text-gray-500 mt-1">{def.specialAttack!.description}</div>
              {specDisabledReason && (
                <div className="text-[10px] text-red-400/80 mt-1.5 flex items-center gap-1">
                  <span className="w-1 h-1 rounded-full bg-red-400/60" /> {specDisabledReason}
                </div>
              )}
            </button>
          )}

          {/* Equipment list */}
          {instance.attachedObjects.length > 0 && (
            <div className="p-2 rounded-xl bg-amber-950/10 border border-amber-800/15">
              <div className="text-[9px] text-amber-400/70 uppercase tracking-wider font-bold mb-1">Equipement</div>
              {instance.attachedObjects.map((objId) => {
                const obj = state.cards[objId];
                if (!obj) return null;
                const { getCardDef } = require("@/engine/cardRegistry");
                const objDef = getCardDef(obj.defId);
                return (
                  <div key={objId} className="text-[10px] text-amber-300/80">
                    ⚔ {objDef.name}
                    {objDef.bonusAtk ? ` +${objDef.bonusAtk} ATK` : ""}
                    {objDef.bonusDef ? ` +${objDef.bonusDef} DEF` : ""}
                    {obj.isAwakened && <span className="text-yellow-400 ml-1">EVEILLE</span>}
                  </div>
                );
              })}
            </div>
          )}

          {/* Bottom actions */}
          <div className="flex gap-2 mt-1">
            <button onClick={onViewDetail}
              className="action-btn flex-1 text-center py-2 rounded-xl border border-gray-600/20 bg-gray-800/20 hover:bg-gray-700/30 text-xs text-gray-400 transition-all">
              Details
            </button>
            <button onClick={onClose}
              className="flex-1 text-center py-2 rounded-xl bg-gray-800/30 hover:bg-gray-700/30 text-gray-500 text-xs transition-all">
              Fermer
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
