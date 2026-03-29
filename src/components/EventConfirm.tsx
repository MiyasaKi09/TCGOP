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

  const isShip = def.type === "ship";

  return (
    <div className="fixed inset-0 bg-black/80 backdrop-blur-sm flex items-center justify-center z-50" onClick={onCancel}>
      <div className="glass rounded-2xl p-5 max-w-md w-full mx-4 border border-amber-600/30 shadow-2xl shadow-amber-900/10 animate-modal-enter" onClick={(e) => e.stopPropagation()}>
        <div className="flex items-center gap-2 mb-3">
          <span className="text-xl">{isShip ? "🚢" : "⚡"}</span>
          <h3 className="text-sm font-bold text-amber-400/80 uppercase tracking-wider">
            {isShip ? "Deployer navire" : "Jouer evenement"}
          </h3>
        </div>
        <h2 className="text-xl font-bold text-white mb-3">{def.name}</h2>

        <div className="flex items-center gap-3 mb-4">
          <div className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl bg-blue-900/30 border border-blue-700/20">
            <div className="w-1.5 h-1.5 rounded-full bg-blue-500" />
            <span className="text-blue-200 text-sm font-bold stat-badge">{def.cost} Vol.</span>
          </div>
          <span className={`text-xs ${canAfford ? "text-green-400/80" : "text-red-400/80"}`}>
            ({playerVol} disponible)
          </span>
        </div>

        {/* Effect description */}
        {def.eventEffect && (
          <div className="p-3 rounded-xl bg-yellow-950/15 border border-yellow-800/20 mb-3">
            <div className="text-[9px] font-bold text-yellow-400/70 uppercase tracking-wider mb-1">Effet</div>
            <div className="text-sm text-gray-200 leading-relaxed">{description}</div>
          </div>
        )}

        {/* Counter info */}
        {def.counterEffect && (
          <div className="p-3 rounded-xl bg-blue-950/15 border border-blue-800/20 mb-3">
            <div className="text-[9px] font-bold text-blue-400/70 uppercase tracking-wider mb-1">Effet Counter</div>
            <div className="text-sm text-gray-200">
              {def.counterEffect.type === "survive"
                ? def.counterEffect.description
                : `Reduit les degats de ${def.counterEffect.amount}.`}
            </div>
          </div>
        )}

        {/* Ship info */}
        {isShip && (
          <div className="p-3 rounded-xl bg-cyan-950/15 border border-cyan-800/20 mb-3 space-y-1.5">
            {def.shipPassive && (
              <div>
                <div className="text-[9px] font-bold text-cyan-400/70 uppercase tracking-wider">Passif</div>
                <div className="text-sm text-gray-200">{def.shipPassive}</div>
              </div>
            )}
            {def.shipActive && (
              <div>
                <div className="text-[9px] font-bold text-cyan-400/70 uppercase tracking-wider">Actif</div>
                <div className="text-sm text-cyan-300">
                  {def.shipActive.name} <span className="text-gray-500">({def.shipActive.cost}V)</span> — {def.shipActive.description}
                </div>
              </div>
            )}
          </div>
        )}

        <div className="flex gap-2 mt-4">
          <button
            onClick={canAfford ? onConfirm : undefined}
            disabled={!canAfford}
            className={`action-btn flex-1 px-4 py-3 rounded-xl text-sm font-bold transition-all ${
              canAfford
                ? "bg-gradient-to-r from-amber-600 to-amber-700 hover:from-amber-500 hover:to-amber-600 text-white border border-amber-500/20 shadow-lg shadow-amber-900/20"
                : "bg-gray-800/40 text-gray-600 cursor-not-allowed border border-gray-700/20"
            }`}
          >
            {canAfford ? "Confirmer" : "Volonte insuffisante"}
          </button>
          <button
            onClick={onCancel}
            className="flex-1 px-4 py-3 rounded-xl bg-gray-800/30 hover:bg-gray-700/30 text-gray-400 text-sm border border-gray-700/20 transition-all"
          >
            Annuler
          </button>
        </div>
      </div>
    </div>
  );
}
