"use client";

import type { CardDef, CardInstance, GameState, GameAction } from "@/types";
import { getEffectiveAtk } from "@/engine/board";
import { canAfford } from "@/engine/volonte";

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

  const canBase = validActions.some(
    (a) =>
      (a.type === "baseAttack" || a.type === "baseSupportAction") &&
      "attackerInstanceId" in a &&
      a.attackerInstanceId === instance.instanceId
  ) || validActions.some(
    (a) =>
      a.type === "baseAttack" &&
      "attackerInstanceId" in a &&
      a.attackerInstanceId === instance.instanceId
  );

  const canSpecial = validActions.some(
    (a) =>
      a.type === "specialAttack" &&
      "attackerInstanceId" in a &&
      a.attackerInstanceId === instance.instanceId
  );

  const hasBaseAttack = def.baseAction && !def.baseAction.isSupport;
  const hasBaseSupport = def.baseAction?.isSupport;
  const hasSpecial = def.specialAttack;

  const isTapped = instance.tapped;
  const usedBase = instance.usedBaseAction;
  const usedSpecial = instance.usedSpecialAttack;

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-40" onClick={onClose}>
      <div
        className="bg-gray-900 border border-gray-600 rounded-xl p-4 min-w-[280px]"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex justify-between items-center mb-3">
          <h3 className="font-bold text-white">{def.name}</h3>
          <div className="text-xs text-gray-400">
            ATK {effectiveAtk} | DEF {def.def} | PV {instance.currentPv}/{def.pv}
          </div>
        </div>

        <div className="flex flex-col gap-2">
          {/* Base Attack */}
          {hasBaseAttack && (
            <button
              onClick={() => { onBaseAttack(); }}
              disabled={isTapped || usedBase}
              className={`w-full text-left p-3 rounded-lg border transition-all ${
                !isTapped && !usedBase && canBase
                  ? "border-green-600 bg-green-900/30 hover:bg-green-900/50 cursor-pointer"
                  : "border-gray-700 bg-gray-800/30 opacity-50 cursor-not-allowed"
              }`}
            >
              <div className="flex justify-between items-center">
                <span className="text-sm font-semibold text-green-300">
                  ⚔ {def.baseAction!.name}
                </span>
                <span className="text-xs px-2 py-0.5 rounded bg-green-800 text-green-200">Gratuit</span>
              </div>
              <div className="text-xs text-gray-400 mt-1">
                ATK {effectiveAtk}
                {def.baseAction!.element && ` | ${def.baseAction!.element}`}
                {def.baseAction!.attackTraits?.map((t) => ` | ${t}`)}
              </div>
              {(isTapped || usedBase) && (
                <div className="text-[10px] text-red-400 mt-1">
                  {isTapped ? "Personnage incline" : "Deja utilise ce tour"}
                </div>
              )}
            </button>
          )}

          {/* Base Support */}
          {hasBaseSupport && (
            <button
              disabled={isTapped || usedBase}
              className={`w-full text-left p-3 rounded-lg border transition-all ${
                !isTapped && !usedBase
                  ? "border-cyan-600 bg-cyan-900/30 hover:bg-cyan-900/50 cursor-pointer"
                  : "border-gray-700 bg-gray-800/30 opacity-50 cursor-not-allowed"
              }`}
            >
              <div className="flex justify-between items-center">
                <span className="text-sm font-semibold text-cyan-300">
                  🛡 {def.baseAction!.name}
                </span>
                <span className="text-xs px-2 py-0.5 rounded bg-cyan-800 text-cyan-200">Gratuit</span>
              </div>
              <div className="text-xs text-gray-400 mt-1">
                {def.baseAction!.healAmount ? `Soigne ${def.baseAction!.healAmount} PV` : ""}
                {def.baseAction!.immobilize ? "Immobilise 1 ennemi" : ""}
                {def.baseAction!.description}
              </div>
              {(isTapped || usedBase) && (
                <div className="text-[10px] text-red-400 mt-1">
                  {isTapped ? "Personnage incline" : "Deja utilise ce tour"}
                </div>
              )}
            </button>
          )}

          {/* Special Attack */}
          {hasSpecial && (
            <button
              onClick={() => { onSpecialAttack(); }}
              disabled={usedSpecial || !canSpecial}
              className={`w-full text-left p-3 rounded-lg border transition-all ${
                !usedSpecial && canSpecial
                  ? "border-red-600 bg-red-900/30 hover:bg-red-900/50 cursor-pointer"
                  : "border-gray-700 bg-gray-800/30 opacity-50 cursor-not-allowed"
              }`}
            >
              <div className="flex justify-between items-center">
                <span className="text-sm font-semibold text-red-300">
                  💥 {def.specialAttack!.name}
                </span>
                <span className="text-xs px-2 py-0.5 rounded bg-red-800 text-red-200">
                  {def.specialAttack!.cost} Vol.
                </span>
              </div>
              <div className="text-xs text-gray-400 mt-1">
                ATK +{def.specialAttack!.atkBonus} (total: {effectiveAtk + def.specialAttack!.atkBonus})
                {def.specialAttack!.element && ` | ${def.specialAttack!.element}`}
                {def.specialAttack!.attackTraits?.map((t) => ` | ${t}`)}
                {def.specialAttack!.oncePerGame && " | 1x/partie"}
              </div>
              <div className="text-xs text-gray-500 mt-1">{def.specialAttack!.description}</div>
              {usedSpecial && <div className="text-[10px] text-red-400 mt-1">Deja utilise ce tour</div>}
              {!canSpecial && !usedSpecial && (
                <div className="text-[10px] text-red-400 mt-1">Pas assez de Volonte ou pas de cible</div>
              )}
            </button>
          )}

          {/* View Detail */}
          <button
            onClick={onViewDetail}
            className="w-full text-left p-2 rounded-lg border border-gray-600 bg-gray-800/30 hover:bg-gray-700/50 transition-all"
          >
            <span className="text-sm text-gray-300">📋 Voir les details</span>
          </button>

          {/* Cancel */}
          <button
            onClick={onClose}
            className="w-full text-center p-2 rounded-lg bg-gray-800 hover:bg-gray-700 text-gray-400 text-sm"
          >
            Fermer
          </button>
        </div>
      </div>
    </div>
  );
}
