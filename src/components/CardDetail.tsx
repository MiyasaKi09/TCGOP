"use client";

import type { CardDef, CardInstance, GameState } from "@/types";
import { getCardDef } from "@/engine/cardRegistry";
import { TRAIT_LABEL } from "@/data/cardArt";
import FullCard from "./FullCard";

interface CardDetailProps {
  def: CardDef;
  instance?: CardInstance;
  state?: GameState;
  onClose: () => void;
}

const STATUS_LABEL: Record<string, string> = {
  burn: "🔥 Brûlure", poison: "☠ Poison", freeze: "❄ Gel", desiccation: "🏜 Dessèchement", trap: "💣 Piège", immobilize: "🌸 Immobilisé",
};

export default function CardDetail({ def, instance, state, onClose }: CardDetailProps) {
  const hasSide =
    (def.synergies && def.synergies.length > 0) ||
    (instance && instance.statusEffects.length > 0) ||
    (instance && instance.attachedObjects.length > 0) ||
    (def.traits && def.traits.length > 0);

  return (
    <div className="fixed inset-0 bg-black/80 backdrop-blur-sm flex items-center justify-center z-50 p-4" onClick={onClose}>
      <div className="rounded-2xl p-4 flex gap-4 max-h-[92vh] overflow-y-auto animate-modal-enter" style={{ background: "rgba(10,14,20,.94)", boxShadow: "inset 0 0 0 1px rgba(232,184,75,.28), 0 24px 70px rgba(0,0,0,.6)" }} onClick={(e) => e.stopPropagation()}>
        <FullCard def={def} instance={instance} state={state} width={240} />

        <div className="flex flex-col gap-2.5 w-[240px]">
          <div className="flex justify-between items-start">
            <div>
              <div className="font-cinzel font-bold text-white text-base leading-tight">{def.name}</div>
              <div className="font-oswald text-[10px] uppercase tracking-wider text-white/45 mt-0.5">{def.type}{def.subtype ? ` · ${def.subtype}` : ""} · {def.rarity}</div>
            </div>
            <button onClick={onClose} className="text-white/40 hover:text-white text-xl leading-none">✕</button>
          </div>

          {def.traits && def.traits.length > 0 && (
            <div className="flex flex-wrap gap-1.5">
              {def.traits.map((t) => (
                <span key={t} className="font-oswald text-[10px] px-2 py-0.5 rounded-full text-white/85" style={{ background: "rgba(255,255,255,.1)" }}>{TRAIT_LABEL[t] ?? t}</span>
              ))}
            </div>
          )}

          {def.synergies && def.synergies.length > 0 && (
            <div className="rounded-xl p-2.5" style={{ background: "rgba(232,184,75,.1)" }}>
              <div className="font-oswald text-[9px] uppercase tracking-wider font-bold mb-1" style={{ color: "#E8B84B" }}>Synergies</div>
              {def.synergies.map((s, i) => {
                let name = s.partnerId;
                try { name = getCardDef(s.partnerId).name; } catch { /* keep id */ }
                return (
                  <div key={i} className="font-spectral text-[11px] text-white/80">
                    {name} → +{s.atkBonus} ATK{s.onPartnerKO ? ` (KO : +${s.onPartnerKO})` : ""}
                  </div>
                );
              })}
            </div>
          )}

          {instance && instance.statusEffects.length > 0 && (
            <div className="rounded-xl p-2.5" style={{ background: "rgba(255,255,255,.05)" }}>
              <div className="font-oswald text-[9px] uppercase tracking-wider text-white/50 font-bold mb-1">Effets actifs</div>
              {instance.statusEffects.map((e, i) => (
                <div key={i} className="font-spectral text-[11px] text-white/80">
                  {STATUS_LABEL[e.type] ?? e.type}{e.turnsRemaining > 0 ? ` (${e.turnsRemaining} t)` : " (permanent)"}{e.damagePerTurn ? ` — ${e.damagePerTurn}/tour` : ""}
                </div>
              ))}
            </div>
          )}

          {instance && instance.attachedObjects.length > 0 && (
            <div className="rounded-xl p-2.5" style={{ background: "rgba(232,184,75,.08)" }}>
              <div className="font-oswald text-[9px] uppercase tracking-wider text-amber-400/70 font-bold mb-1">Équipement</div>
              {instance.attachedObjects.map((objId) => {
                const objInst = state?.cards[objId];
                if (!objInst) return null;
                let objDef: CardDef | null = null;
                try { objDef = getCardDef(objInst.defId); } catch { objDef = null; }
                if (!objDef) return null;
                return (
                  <div key={objId} className="font-spectral text-[11px] text-amber-200/85">
                    {objDef.name}{objDef.bonusAtk ? ` (+${objDef.bonusAtk} ATK)` : ""}{objDef.bonusDef ? ` (+${objDef.bonusDef} DEF)` : ""}
                  </div>
                );
              })}
            </div>
          )}

          {!hasSide && <div className="font-spectral italic text-white/40 text-sm">Toutes les infos sont sur la carte.</div>}
        </div>
      </div>
    </div>
  );
}
