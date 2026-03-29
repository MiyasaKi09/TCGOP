"use client";

import type { CardDef, CardInstance } from "@/types";
import { getEffectiveAtk, getEffectiveDef } from "@/engine/board";
import type { GameState } from "@/types";

interface CardDetailProps {
  def: CardDef;
  instance?: CardInstance;
  state?: GameState;
  onClose: () => void;
}

const TRAIT_LABELS: Record<string, string> = {
  shield: "Bouclier",
  range: "Portee",
  stealth: "Furtif",
  rush: "Rush",
  cursed: "Maudit",
  logia: "Logia",
  piercing: "Percant",
  conqueror: "Conquerant",
};

const ELEMENT_ICONS: Record<string, string> = {
  fire: "🔥 Feu",
  water: "💧 Eau",
  thunder: "⚡ Foudre",
  ice: "❄ Glace",
  sand: "🏜 Sable",
  poison: "☠ Poison",
};

export default function CardDetail({ def, instance, state, onClose }: CardDetailProps) {
  const isChar = def.type === "character";
  const effectiveAtk = instance && state ? getEffectiveAtk(state, instance.instanceId) : def.atk;
  const effectiveDef = instance && state ? getEffectiveDef(state, instance.instanceId) : def.def;

  return (
    <div className="fixed inset-0 bg-black/80 flex items-center justify-center z-50" onClick={onClose}>
      <div
        className="bg-gray-900 border border-gray-600 rounded-xl p-5 max-w-md w-full mx-4 max-h-[90vh] overflow-y-auto"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex justify-between items-start mb-3">
          <div>
            <h2 className="text-lg font-bold text-white">{def.name}</h2>
            <div className="flex gap-2 text-xs text-gray-400 mt-1">
              <span className="px-1.5 py-0.5 rounded bg-blue-800">{def.cost} Vol.</span>
              <span className="px-1.5 py-0.5 rounded bg-gray-700">{def.type}</span>
              {def.subtype && <span className="px-1.5 py-0.5 rounded bg-amber-800">{def.subtype}</span>}
              <span className="px-1.5 py-0.5 rounded bg-gray-700">{def.rarity}</span>
            </div>
          </div>
          <button onClick={onClose} className="text-gray-500 hover:text-white text-xl">✕</button>
        </div>

        {/* Stats */}
        {isChar && (
          <div className="flex gap-4 mb-3 text-sm">
            <div className="flex-1 bg-red-900/30 rounded p-2 text-center">
              <div className="text-red-400 text-xs">ATK</div>
              <div className="text-xl font-bold text-red-300">
                {effectiveAtk}
                {effectiveAtk !== def.atk && (
                  <span className="text-xs text-gray-500 ml-1">({def.atk})</span>
                )}
              </div>
            </div>
            <div className="flex-1 bg-blue-900/30 rounded p-2 text-center">
              <div className="text-blue-400 text-xs">DEF</div>
              <div className="text-xl font-bold text-blue-300">
                {effectiveDef}
                {effectiveDef !== def.def && (
                  <span className="text-xs text-gray-500 ml-1">({def.def})</span>
                )}
              </div>
            </div>
            <div className="flex-1 bg-green-900/30 rounded p-2 text-center">
              <div className="text-green-400 text-xs">PV</div>
              <div className="text-xl font-bold text-green-300">
                {instance ? `${instance.currentPv}/${def.pv}` : def.pv}
              </div>
            </div>
          </div>
        )}

        {/* Traits */}
        {def.traits && def.traits.length > 0 && (
          <div className="flex flex-wrap gap-1.5 mb-3">
            {def.traits.map((t) => (
              <span key={t} className="px-2 py-1 rounded-full bg-gray-700 text-xs text-gray-200">
                {TRAIT_LABELS[t] ?? t}
              </span>
            ))}
          </div>
        )}

        {/* Passive */}
        {def.passive && (
          <div className="mb-3 p-2.5 rounded bg-purple-900/20 border border-purple-800/50">
            <div className="text-xs font-bold text-purple-300 mb-1">PASSIF — {def.passive.name}</div>
            <div className="text-xs text-gray-300">{def.passive.description}</div>
          </div>
        )}

        {/* Base Action */}
        {def.baseAction && (
          <div className="mb-3 p-2.5 rounded bg-green-900/20 border border-green-800/50">
            <div className="flex justify-between items-center mb-1">
              <span className="text-xs font-bold text-green-300">
                {def.baseAction.isSupport ? "🛡 SOUTIEN" : "⚔ ATTAQUE DE BASE"} — {def.baseAction.name}
              </span>
              <span className="text-[10px] px-1.5 py-0.5 rounded bg-green-800 text-green-200">Gratuit</span>
            </div>
            {!def.baseAction.isSupport && (
              <div className="text-xs text-gray-300">
                ATK {def.baseAction.atk}
                {def.baseAction.element && <span className="ml-2">{ELEMENT_ICONS[def.baseAction.element]}</span>}
                {def.baseAction.attackTraits?.map((t) => (
                  <span key={t} className="ml-1 px-1 py-0.5 rounded bg-gray-700 text-[10px]">{t}</span>
                ))}
              </div>
            )}
            {def.baseAction.isSupport && def.baseAction.healAmount && (
              <div className="text-xs text-gray-300">Soigne {def.baseAction.healAmount} PV</div>
            )}
            <div className="text-xs text-gray-400 mt-1">{def.baseAction.description}</div>
          </div>
        )}

        {/* Special Attack */}
        {def.specialAttack && (
          <div className="mb-3 p-2.5 rounded bg-red-900/20 border border-red-800/50">
            <div className="flex justify-between items-center mb-1">
              <span className="text-xs font-bold text-red-300">
                💥 SPECIALE — {def.specialAttack.name}
              </span>
              <span className="text-[10px] px-1.5 py-0.5 rounded bg-red-800 text-red-200">
                {def.specialAttack.cost} Vol.
              </span>
            </div>
            <div className="text-xs text-gray-300">
              ATK +{def.specialAttack.atkBonus}
              {def.specialAttack.element && <span className="ml-2">{ELEMENT_ICONS[def.specialAttack.element]}</span>}
              {def.specialAttack.attackTraits?.map((t) => (
                <span key={t} className="ml-1 px-1 py-0.5 rounded bg-gray-700 text-[10px]">{t}</span>
              ))}
              {def.specialAttack.oncePerGame && (
                <span className="ml-1 px-1 py-0.5 rounded bg-yellow-800 text-[10px]">1x/partie</span>
              )}
              {def.specialAttack.ignoreDef && (
                <span className="ml-1 text-amber-300">Ignore {def.specialAttack.ignoreDef} DEF</span>
              )}
            </div>
            <div className="text-xs text-gray-400 mt-1">{def.specialAttack.description}</div>
          </div>
        )}

        {/* Synergies */}
        {def.synergies && def.synergies.length > 0 && (
          <div className="mb-3 p-2.5 rounded bg-amber-900/20 border border-amber-800/50">
            <div className="text-xs font-bold text-amber-300 mb-1">SYNERGIES</div>
            {def.synergies.map((s, i) => (
              <div key={i} className="text-xs text-gray-300">
                Partenaire: <span className="text-amber-200">{s.partnerId}</span> → +{s.atkBonus} ATK
                {s.onPartnerKO && <span className="text-red-300 ml-1">(KO: +{s.onPartnerKO} ATK)</span>}
              </div>
            ))}
          </div>
        )}

        {/* Object info */}
        {def.type === "object" && (
          <div className="mb-3 p-2.5 rounded bg-amber-900/20 border border-amber-800/50">
            {def.bonusAtk ? <div className="text-sm text-amber-300 font-bold">+{def.bonusAtk} ATK</div> : null}
            {def.bonusDef ? <div className="text-sm text-blue-300 font-bold">+{def.bonusDef} DEF</div> : null}
            {def.restriction && <div className="text-xs text-gray-400 mt-1">Restriction: {def.restriction}</div>}
            {def.equipEffect && <div className="text-xs text-gray-300 mt-1">{def.equipEffect}</div>}
          </div>
        )}

        {/* Ship info */}
        {def.type === "ship" && (
          <div className="mb-3 p-2.5 rounded bg-cyan-900/20 border border-cyan-800/50">
            {def.shipPassive && <div className="text-xs text-gray-300 mb-1">{def.shipPassive}</div>}
            {def.shipActive && (
              <div className="text-xs text-cyan-300 mt-1">
                {def.shipActive.name} ({def.shipActive.cost} Vol.) — {def.shipActive.description}
              </div>
            )}
          </div>
        )}

        {/* Event/Counter info */}
        {def.eventEffect && (
          <div className="mb-3 p-2.5 rounded bg-yellow-900/20 border border-yellow-800/50">
            <div className="text-xs text-gray-300">{JSON.stringify(def.eventEffect)}</div>
          </div>
        )}
        {def.counterEffect && (
          <div className="mb-3 p-2.5 rounded bg-blue-900/20 border border-blue-800/50">
            <div className="text-xs text-blue-300">
              {def.counterEffect.type === "survive" ? def.counterEffect.description : `Reduit ${def.counterEffect.amount} degats`}
            </div>
          </div>
        )}

        {/* Status effects on instance */}
        {instance && instance.statusEffects.length > 0 && (
          <div className="mb-3">
            <div className="text-xs font-bold text-gray-400 mb-1">Effets actifs:</div>
            {instance.statusEffects.map((e, i) => (
              <div key={i} className="text-xs text-gray-300">
                {e.type === "burn" ? "🔥 Brulure" : e.type === "poison" ? "☠ Poison" : e.type === "freeze" ? "❄ Gel" : "🏜 Dessechement"}
                {e.turnsRemaining > 0 ? ` (${e.turnsRemaining} tours)` : " (permanent)"}
                {` — ${e.damagePerTurn} deg./tour`}
              </div>
            ))}
          </div>
        )}

        {/* Equipped objects */}
        {instance && instance.attachedObjects.length > 0 && (
          <div className="mb-3">
            <div className="text-xs font-bold text-amber-400 mb-1">Equipements:</div>
            {instance.attachedObjects.map((objId) => {
              const objInstance = state?.cards[objId];
              if (!objInstance) return null;
              const objDef = (() => { try { return require("@/engine/cardRegistry").getCardDef(objInstance.defId); } catch { return null; } })();
              if (!objDef) return null;
              return (
                <div key={objId} className="text-xs text-amber-300">
                  {objDef.name} {objDef.bonusAtk ? `(+${objDef.bonusAtk} ATK)` : ""}
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
