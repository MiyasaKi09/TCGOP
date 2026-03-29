"use client";

import type { CardDef } from "@/types";

interface EventConfirmProps {
  def: CardDef;
  playerVol: number;
  onConfirm: () => void;
  onCancel: () => void;
}

const EFFECT_DESCRIPTIONS: Record<string, (effect: NonNullable<CardDef["eventEffect"]>) => string> = {
  gainWill: (e) => `Gagne +${(e as { amount: number }).amount} Volonte ce tour.`,
  draw: (e) => {
    const eff = e as { amount: number; discard?: number };
    return `Pioche ${eff.amount} carte(s)${eff.discard ? `, defausse ${eff.discard}` : ""}.`;
  },
  healAlly: (e) => {
    const eff = e as { amount: number; allAllies?: boolean };
    return eff.allAllies ? `Tous les allies +${eff.amount} PV.` : `1 allie +${eff.amount} PV.`;
  },
  buffAllies: (e) => {
    const eff = e as { stat: string; amount: number; duration: string };
    return `Tous les allies +${eff.amount} ${eff.stat.toUpperCase()} (${eff.duration === "turn" ? "ce tour" : "permanent"}).`;
  },
  damageEnemies: (e) => {
    const eff = e as { amount: number; target: string };
    const targets: Record<string, string> = {
      allFront: "toute la Ligne Avant ennemie",
      allCursed: "tous les Maudits ennemis",
      single: "1 ennemi",
    };
    return `${eff.amount} degats a ${targets[eff.target] ?? eff.target}.`;
  },
  dodgeAll: () => "Tous vos personnages esquivent toutes les attaques ce tour.",
  custom: (e) => (e as { description: string }).description,
};

export default function EventConfirm({ def, playerVol, onConfirm, onCancel }: EventConfirmProps) {
  const canAfford = playerVol >= def.cost;
  const effect = def.eventEffect;
  const description = effect
    ? (EFFECT_DESCRIPTIONS[effect.type]?.(effect) ?? JSON.stringify(effect))
    : "Effet inconnu";

  return (
    <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50" onClick={onCancel}>
      <div className="bg-gray-900 border-2 border-amber-600 rounded-xl p-5 max-w-md w-full mx-4" onClick={(e) => e.stopPropagation()}>
        <h3 className="text-lg font-bold text-amber-400 mb-1">
          {def.type === "ship" ? "🚢 Deployer navire" : "⚡ Jouer evenement"}
        </h3>
        <h2 className="text-xl font-bold text-white mb-3">{def.name}</h2>

        <div className="flex items-center gap-2 mb-3">
          <span className="px-2 py-1 rounded bg-blue-800 text-blue-200 text-sm font-bold">
            Cout : {def.cost} Vol.
          </span>
          <span className={`text-sm ${canAfford ? "text-green-400" : "text-red-400"}`}>
            (vous avez {playerVol} Vol.)
          </span>
        </div>

        {/* Effect description */}
        {def.eventEffect && (
          <div className="p-3 rounded bg-yellow-900/30 border border-yellow-800/50 mb-3">
            <div className="text-sm font-semibold text-yellow-300 mb-1">Effet :</div>
            <div className="text-sm text-gray-200">{description}</div>
          </div>
        )}

        {/* Counter info */}
        {def.counterEffect && (
          <div className="p-3 rounded bg-blue-900/30 border border-blue-800/50 mb-3">
            <div className="text-sm font-semibold text-blue-300 mb-1">Effet (Counter) :</div>
            <div className="text-sm text-gray-200">
              {def.counterEffect.type === "survive"
                ? def.counterEffect.description
                : `Reduit les degats de ${def.counterEffect.amount}.`}
            </div>
          </div>
        )}

        {/* Ship info */}
        {def.type === "ship" && (
          <div className="p-3 rounded bg-cyan-900/30 border border-cyan-800/50 mb-3">
            {def.shipPassive && <div className="text-sm text-gray-200 mb-1">{def.shipPassive}</div>}
            {def.shipActive && (
              <div className="text-sm text-cyan-300">
                {def.shipActive.name} ({def.shipActive.cost} Vol.) : {def.shipActive.description}
              </div>
            )}
          </div>
        )}

        <div className="flex gap-3 mt-4">
          <button
            onClick={canAfford ? onConfirm : undefined}
            disabled={!canAfford}
            className={`flex-1 px-4 py-3 rounded-lg text-base font-semibold ${
              canAfford
                ? "bg-amber-600 hover:bg-amber-500 text-white cursor-pointer"
                : "bg-gray-700 text-gray-500 cursor-not-allowed"
            }`}
          >
            {canAfford ? "Jouer" : "Pas assez de Vol."}
          </button>
          <button
            onClick={onCancel}
            className="flex-1 px-4 py-3 rounded-lg bg-gray-700 hover:bg-gray-600 text-gray-300 text-base"
          >
            Annuler
          </button>
        </div>
      </div>
    </div>
  );
}
