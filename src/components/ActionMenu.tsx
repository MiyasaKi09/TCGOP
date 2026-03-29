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

  // Check what actions are available for this character
  const canBaseAttack = validActions.some(
    (a) => a.type === "baseAttack" && "attackerInstanceId" in a && a.attackerInstanceId === instance.instanceId
  );

  const canSpecialAttack = validActions.some(
    (a) => a.type === "specialAttack" && "attackerInstanceId" in a && a.attackerInstanceId === instance.instanceId
  );

  const hasBaseAttackAction = def.baseAction && !def.baseAction.isSupport;
  const hasBaseSupportAction = def.baseAction?.isSupport;
  const hasSpecial = !!def.specialAttack;

  const isTapped = instance.tapped;
  const usedBase = instance.usedBaseAction;
  const usedSpecial = instance.usedSpecialAttack;
  const hasSickness = instance.deployedTurn === state.turnNumber && !(def.traits?.includes("rush"));
  const isFrozen = instance.statusEffects.some((e) => e.type === "freeze");
  const playerVol = state.players[instance.owner].volonte;

  // Reason why base attack is disabled
  const baseDisabledReason = (() => {
    if (isFrozen) return "Gele !";
    if (hasSickness) return "Mal de terre (deploye ce tour)";
    if (isTapped) return "Personnage incline";
    if (usedBase) return "Deja utilise ce tour";
    if (!hasBaseAttackAction) return "Action de base = soutien (pas d'attaque)";
    return null;
  })();

  // Reason why special is disabled
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

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-40" onClick={onClose}>
      <div className="bg-gray-900 border border-gray-600 rounded-xl p-4 min-w-[320px] max-w-[400px]" onClick={(e) => e.stopPropagation()}>
        {/* Header */}
        <div className="flex justify-between items-center mb-3">
          <h3 className="font-bold text-white text-lg">{def.name}</h3>
          <div className="text-xs text-gray-400">
            ATK {effectiveAtk} | DEF {def.def} | PV {instance.currentPv}/{def.pv}
          </div>
        </div>

        {/* Status indicators */}
        <div className="flex gap-1.5 mb-3 flex-wrap">
          {isTapped && <span className="text-[10px] px-1.5 py-0.5 rounded bg-gray-700 text-gray-300">Incline</span>}
          {hasSickness && <span className="text-[10px] px-1.5 py-0.5 rounded bg-yellow-800 text-yellow-200">Mal de terre</span>}
          {isFrozen && <span className="text-[10px] px-1.5 py-0.5 rounded bg-cyan-800 text-cyan-200">Gele</span>}
          <span className="text-[10px] px-1.5 py-0.5 rounded bg-blue-800 text-blue-200">Vol: {playerVol}</span>
        </div>

        <div className="flex flex-col gap-2">
          {/* Base Attack (only for non-support base actions) */}
          {hasBaseAttackAction && (
            <button
              onClick={canBaseAttack ? onBaseAttack : undefined}
              className={`w-full text-left p-3 rounded-lg border transition-all ${
                canBaseAttack
                  ? "border-green-600 bg-green-900/30 hover:bg-green-900/50 cursor-pointer"
                  : "border-gray-700 bg-gray-800/30 opacity-50 cursor-not-allowed"
              }`}
            >
              <div className="flex justify-between items-center">
                <span className="text-sm font-semibold text-green-300">⚔ {def.baseAction!.name}</span>
                <span className="text-[10px] px-2 py-0.5 rounded bg-green-800 text-green-200">Gratuit</span>
              </div>
              <div className="text-xs text-gray-400 mt-1">
                ATK {effectiveAtk}
                {def.baseAction!.element && ` | ${def.baseAction!.element}`}
                {def.baseAction!.attackTraits?.map((t) => ` | ${t}`).join("")}
              </div>
              {baseDisabledReason && (
                <div className="text-[10px] text-red-400 mt-1">{baseDisabledReason}</div>
              )}
            </button>
          )}

          {/* Base Support (heal, immobilize, etc.) */}
          {hasBaseSupportAction && (
            <div className="w-full text-left p-3 rounded-lg border border-cyan-700 bg-cyan-900/20">
              <div className="flex justify-between items-center">
                <span className="text-sm font-semibold text-cyan-300">🛡 {def.baseAction!.name}</span>
                <span className="text-[10px] px-2 py-0.5 rounded bg-cyan-800 text-cyan-200">Gratuit</span>
              </div>
              <div className="text-xs text-gray-400 mt-1">{def.baseAction!.description}</div>
              {def.baseAction!.healAmount && (
                <div className="text-xs text-green-400 mt-0.5">Soigne {def.baseAction!.healAmount} PV</div>
              )}
              <div className="text-[10px] text-yellow-400 mt-1">
                (Soutien automatique — pas une attaque)
              </div>
            </div>
          )}

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
              <div className="text-xs text-gray-400 mt-1">
                ATK {effectiveAtk} + {def.specialAttack!.atkBonus} = {effectiveAtk + def.specialAttack!.atkBonus}
                {def.specialAttack!.element && ` | ${def.specialAttack!.element}`}
                {def.specialAttack!.attackTraits?.map((t) => ` | ${t}`).join("")}
                {def.specialAttack!.oncePerGame && " | 1x/partie"}
              </div>
              <div className="text-xs text-gray-500 mt-1">{def.specialAttack!.description}</div>
              {specDisabledReason && (
                <div className="text-[10px] text-red-400 mt-1">{specDisabledReason}</div>
              )}
            </button>
          )}

          {/* View Detail */}
          <button onClick={onViewDetail}
            className="w-full text-left p-2 rounded-lg border border-gray-600 bg-gray-800/30 hover:bg-gray-700/50 transition-all">
            <span className="text-sm text-gray-300">📋 Details complets</span>
          </button>

          {/* Cancel */}
          <button onClick={onClose}
            className="w-full text-center p-2 rounded-lg bg-gray-800 hover:bg-gray-700 text-gray-400 text-sm">
            Fermer
          </button>
        </div>
      </div>
    </div>
  );
}
