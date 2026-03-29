"use client";

import type { CardDef, CardInstance, GameState, GameAction } from "@/types";
import { getEffectiveAtk } from "@/engine/board";

interface ActionMenuProps {
  instance: CardInstance;
  def: CardDef;
  state: GameState;
  validActions: GameAction[];
  onBaseAttack: () => void;
  onSpecialAttack: () => void;
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
  onViewDetail,
  onClose,
}: ActionMenuProps) {
  const effectiveAtk = getEffectiveAtk(state, instance.instanceId);

  const canBaseAttack = validActions.some(
    (a) => a.type === "baseAttack" && "attackerInstanceId" in a && a.attackerInstanceId === instance.instanceId
  );
  const canSpecialAttack = validActions.some(
    (a) => a.type === "specialAttack" && "attackerInstanceId" in a && a.attackerInstanceId === instance.instanceId
  );

  const isTapped = instance.tapped;
  const usedBase = instance.usedBaseAction;
  const usedSpecial = instance.usedSpecialAttack;
  const hasSickness = instance.deployedTurn === state.turnNumber && !(def.traits?.includes("rush"));
  const isFrozen = instance.statusEffects.some((e) => e.type === "freeze");
  const playerVol = state.players[instance.owner].volonte;
  const hasSpecial = !!def.specialAttack;

  const baseDisabledReason = (() => {
    if (isFrozen) return "Gele !";
    if (hasSickness) return "Mal de terre (deploye ce tour)";
    if (isTapped) return "Personnage incline";
    if (usedBase) return "Deja utilise ce tour";
    if (!def.atk || def.atk <= 0) return "ATK 0 — ne peut pas attaquer";
    if (!canBaseAttack) return "Pas de cible valide";
    return null;
  })();

  const specDisabledReason = (() => {
    if (!hasSpecial) return null;
    if (isFrozen) return "Gele !";
    if (hasSickness) return "Mal de terre (deploye ce tour)";
    if (usedSpecial) return "Deja utilise ce tour";
    if (def.specialAttack!.oncePerGame && instance.usedOnceAbilities.includes(def.specialAttack!.name)) {
      return "Deja utilise (1x/partie)";
    }
    if (playerVol < def.specialAttack!.cost) return `Pas assez de Volonte (${playerVol}/${def.specialAttack!.cost})`;
    if (!canSpecialAttack) return "Pas de cible valide";
    return null;
  })();

  const baseAction = def.baseAction;
  const hasBaseEffect = baseAction && (baseAction.healAmount || baseAction.immobilize || baseAction.isSupport);

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-40" onClick={onClose}>
      <div className="bg-gray-900 border border-gray-600 rounded-xl p-4 min-w-[320px] max-w-[420px]" onClick={(e) => e.stopPropagation()}>
        {/* Header */}
        <div className="mb-3">
          <div className="flex justify-between items-center">
            <h3 className="font-bold text-white text-lg">{def.name}</h3>
            <span className="text-xs text-blue-300 font-bold">⭐ {playerVol} Vol.</span>
          </div>
          <div className="flex gap-3 mt-1 text-sm">
            <span className="text-red-400">⚔ ATK {effectiveAtk}</span>
            <span className="text-blue-400">🛡 DEF {def.def}</span>
            <span className={`${instance.currentPv <= (def.pv ?? 0) / 3 ? "text-red-400" : "text-green-400"}`}>
              ❤ PV {instance.currentPv}/{def.pv}
            </span>
          </div>
          {/* Status */}
          <div className="flex gap-1.5 mt-1.5 flex-wrap">
            {isTapped && <span className="text-[10px] px-1.5 py-0.5 rounded bg-gray-700 text-gray-300">Incline</span>}
            {hasSickness && <span className="text-[10px] px-1.5 py-0.5 rounded bg-yellow-800 text-yellow-200">Mal de terre</span>}
            {isFrozen && <span className="text-[10px] px-1.5 py-0.5 rounded bg-cyan-800 text-cyan-200">Gele</span>}
          </div>
        </div>

        {/* Passive info */}
        {def.passive && (
          <div className="mb-2 p-2 rounded bg-purple-900/20 border border-purple-800/40">
            <div className="text-[10px] font-bold text-purple-300">PASSIF — {def.passive.name}</div>
            <div className="text-[10px] text-gray-400">{def.passive.description}</div>
          </div>
        )}

        <div className="flex flex-col gap-2">
          {/* Base Attack — uses character ATK, may have extra effects */}
          <button
            onClick={canBaseAttack ? onBaseAttack : undefined}
            className={`w-full text-left p-3 rounded-lg border transition-all ${
              canBaseAttack
                ? "border-green-600 bg-green-900/30 hover:bg-green-900/50 cursor-pointer"
                : "border-gray-700 bg-gray-800/30 opacity-50 cursor-not-allowed"
            }`}
          >
            <div className="flex justify-between items-center">
              <span className="text-sm font-semibold text-green-300">
                ⚔ {baseAction?.name ?? "Attaque de base"}
              </span>
              <span className="text-[10px] px-2 py-0.5 rounded bg-green-800 text-green-200">Gratuit</span>
            </div>
            <div className="text-xs text-gray-300 mt-1">
              ATK {effectiveAtk}
              {baseAction?.element && <span className="ml-1 text-amber-300">({baseAction.element})</span>}
              {baseAction?.attackTraits?.map((t) => (
                <span key={t} className="ml-1 px-1 py-0.5 rounded bg-gray-700 text-[10px]">{t}</span>
              ))}
            </div>
            {/* Extra effects */}
            {hasBaseEffect && (
              <div className="text-[10px] text-cyan-300 mt-1">
                + {baseAction!.description}
              </div>
            )}
            {baseDisabledReason && (
              <div className="text-[10px] text-red-400 mt-1">{baseDisabledReason}</div>
            )}
          </button>

          {/* Special Attack */}
          {hasSpecial && (
            <button
              onClick={canSpecialAttack ? onSpecialAttack : undefined}
              className={`w-full text-left p-3 rounded-lg border transition-all ${
                canSpecialAttack
                  ? "border-red-600 bg-red-900/30 hover:bg-red-900/50 cursor-pointer"
                  : "border-gray-700 bg-gray-800/30 opacity-50 cursor-not-allowed"
              }`}
            >
              <div className="flex justify-between items-center">
                <span className="text-sm font-semibold text-red-300">💥 {def.specialAttack!.name}</span>
                <span className="text-[10px] px-2 py-0.5 rounded bg-red-800 text-red-200">
                  {def.specialAttack!.cost} Vol.
                </span>
              </div>
              <div className="text-xs text-gray-300 mt-1">
                ATK {effectiveAtk} + {def.specialAttack!.atkBonus} = <span className="font-bold text-red-300">{effectiveAtk + def.specialAttack!.atkBonus}</span>
                {def.specialAttack!.element && <span className="ml-1 text-amber-300">({def.specialAttack!.element})</span>}
                {def.specialAttack!.attackTraits?.map((t) => (
                  <span key={t} className="ml-1 px-1 py-0.5 rounded bg-gray-700 text-[10px]">{t}</span>
                ))}
                {def.specialAttack!.oncePerGame && (
                  <span className="ml-1 px-1 py-0.5 rounded bg-yellow-800 text-[10px]">1x/partie</span>
                )}
              </div>
              <div className="text-[10px] text-gray-400 mt-1">{def.specialAttack!.description}</div>
              {specDisabledReason && (
                <div className="text-[10px] text-red-400 mt-1">{specDisabledReason}</div>
              )}
            </button>
          )}

          {/* View Detail / Close */}
          <div className="flex gap-2">
            <button onClick={onViewDetail}
              className="flex-1 text-center p-2 rounded-lg border border-gray-600 bg-gray-800/30 hover:bg-gray-700/50 text-sm text-gray-300">
              📋 Details
            </button>
            <button onClick={onClose}
              className="flex-1 text-center p-2 rounded-lg bg-gray-800 hover:bg-gray-700 text-gray-400 text-sm">
              Fermer
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
