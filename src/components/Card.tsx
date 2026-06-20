"use client";

import type { CardInstance, CardDef } from "@/types";
import {
  CARD_ART, CARD_ART_FOCUS, faction, TRAIT_COLOR, RARITY_BORDER, hpColor,
} from "@/data/cardArt";
import { Sword, Shield, Heart, Crest } from "./icons";

interface CardProps {
  instance: CardInstance;
  def: CardDef;
  onClick?: () => void;
  selected?: boolean;
  highlight?: boolean;
  small?: boolean;
  /** Full-card width in px (hand). Defaults to 168. */
  width?: number;
}

const STATUS_ICON: Record<string, string> = {
  burn: "🔥", poison: "☠", freeze: "❄", desiccation: "🏜", trap: "💣", immobilize: "🌸",
};

export default function Card({ instance, def, onClick, selected, highlight, small, width = 168 }: CardProps) {
  const art = CARD_ART[def.id];
  const fac = faction(def.faction);
  const isCharacter = def.type === "character";
  const hpRatio = isCharacter ? instance.currentPv / (def.pv ?? 1) : 1;
  const hpc = hpColor(hpRatio);
  const borderColor = RARITY_BORDER[def.rarity] ?? fac.border;
  const traits = def.traits ?? [];

  // ===================== BOARD TOKEN =====================
  if (small) {
    return (
      <div
        onClick={onClick}
        className={`relative w-full h-full rounded-xl overflow-hidden cursor-pointer transition-all duration-150 ${instance.tapped ? "saturate-[.7] brightness-90" : "hover:brightness-110"}`}
        style={{ background: "#0b0e13", boxShadow: "0 5px 16px rgba(0,0,0,.5)" }}
      >
        {art ? (
          <div className="absolute inset-0" style={{ backgroundImage: `url('${art}')`, backgroundSize: "172%", backgroundPosition: CARD_ART_FOCUS[def.id] ?? "50% 30%", backgroundRepeat: "no-repeat" }} />
        ) : (
          <>
            <div className="absolute inset-0" style={{ background: fac.frame }} />
            <div className="absolute inset-0 flex items-center justify-center opacity-20">
              <Crest which={fac.crest} size={56} color="#fff" />
            </div>
          </>
        )}

        {/* top scrim */}
        <div className="absolute top-0 left-0 right-0 h-9" style={{ background: "linear-gradient(180deg,rgba(4,6,10,.62),transparent)" }} />

        {/* cost */}
        <div className="absolute top-1 left-1 w-5 h-5 rounded-full flex items-center justify-center font-oswald font-bold text-[12px]"
          style={{ background: "rgba(8,10,14,.85)", border: "1.5px solid #E8B84B", color: "#E8B84B", boxShadow: "0 1px 4px rgba(0,0,0,.5)" }}>
          {def.cost}
        </div>

        {/* trait dots */}
        {traits.length > 0 && (
          <div className="absolute top-1.5 right-1 flex flex-col gap-[3px] items-end">
            {traits.map((t) => (
              <span key={t} title={t} className="w-[9px] h-[9px] rounded-full" style={{ background: TRAIT_COLOR[t] ?? "#888", boxShadow: "0 0 0 1px rgba(0,0,0,.35)" }} />
            ))}
          </div>
        )}

        {/* status + equipment row */}
        {(instance.statusEffects.length > 0 || instance.attachedObjects.length > 0) && (
          <div className="absolute top-7 right-1 flex flex-col items-end gap-0.5 text-[10px] leading-none">
            {instance.statusEffects.map((e, i) => <span key={i}>{STATUS_ICON[e.type] ?? "•"}</span>)}
            {instance.attachedObjects.length > 0 && (
              <span className="font-oswald font-bold text-[9px] px-1 rounded" style={{ background: "rgba(232,184,75,.25)", color: "#E8B84B" }}>⚔{instance.attachedObjects.length}</span>
            )}
          </div>
        )}

        {/* bottom info */}
        <div className="absolute left-0 right-0 bottom-0 px-1.5 pb-1.5 pt-4" style={{ background: "linear-gradient(180deg,transparent,rgba(5,7,11,.78) 36%,rgba(5,7,11,.95))" }}>
          <div className="font-oswald font-semibold text-[12px] text-white text-center truncate" style={{ textShadow: "0 1px 3px #000" }}>{def.name}</div>
          {isCharacter && (
            <div className="flex justify-center items-center gap-2 mt-0.5 font-oswald font-bold text-[14px]">
              <span className="flex items-center gap-0.5" style={{ color: "#FF7062" }}><Sword size={11} />{def.atk}</span>
              <span className="flex items-center gap-0.5" style={{ color: "#7FB0E8" }}><Shield size={11} />{def.def}</span>
              <span className="flex items-center gap-0.5" style={{ color: hpc }}><Heart size={12} color={hpc} />{instance.currentPv}</span>
            </div>
          )}
        </div>

        {/* tapped veil */}
        {instance.tapped && (
          <div className="absolute inset-0 flex items-center justify-center" style={{ background: "rgba(5,7,11,.5)" }}>
            <span className="font-oswald text-[9px] tracking-widest uppercase text-white px-2 py-0.5 rounded-full" style={{ background: "rgba(0,0,0,.6)" }}>Incliné</span>
          </div>
        )}

        {/* border / selected */}
        <div className="absolute inset-0 rounded-xl pointer-events-none" style={{ boxShadow: `inset 0 0 0 1.5px ${borderColor}` }} />
        {selected && <div className="absolute inset-0 rounded-xl pointer-events-none ring-select" />}
      </div>
    );
  }

  // ===================== HAND — FULL ILLUSTRATION =====================
  if (art) {
    return (
      <div
        onClick={onClick}
        className={`relative rounded-xl overflow-hidden flex-shrink-0 cursor-pointer transition-all duration-200
          ${selected ? "scale-105 -translate-y-3" : "hover:-translate-y-1.5 hover:brightness-110"}`}
        style={{ width, boxShadow: selected ? "0 0 0 2px #E8B84B, 0 0 22px 2px rgba(232,184,75,.5)" : `0 0 0 1.5px ${borderColor}, 0 10px 24px rgba(0,0,0,.5)` }}
      >
        {/* eslint-disable-next-line @next/next/no-img-element */}
        <img src={art} alt={def.name} className="block w-full h-auto select-none" draggable={false} />
        {highlight && <div className="absolute -bottom-0 left-1/2 -translate-x-1/2 w-10 h-1 rounded-full bg-green-500 shadow-lg shadow-green-500/50 animate-pulse" />}
      </div>
    );
  }

  // ===================== HAND — RECREATED FRAME =====================
  return (
    <div
      onClick={onClick}
      className={`relative rounded-xl overflow-hidden flex-shrink-0 cursor-pointer transition-all duration-200 p-3 flex flex-col
        ${selected ? "scale-105 -translate-y-3" : "hover:-translate-y-1.5 hover:brightness-110"}`}
      style={{ width, height: Math.round(width * 1.39), background: fac.frame, boxShadow: selected ? "0 0 0 2px #E8B84B, 0 0 22px 2px rgba(232,184,75,.5)" : `0 0 0 1.5px ${borderColor}, 0 10px 24px rgba(0,0,0,.5)` }}
    >
      <div className="absolute inset-0 flex items-center justify-center opacity-[0.12] pointer-events-none">
        <Crest which={fac.crest} size={150} color="#fff" />
      </div>

      {/* cost + rarity */}
      <div className="flex justify-between items-start relative">
        <span className="font-oswald font-bold text-[34px] leading-none" style={{ color: "#E8B84B", textShadow: "0 2px 4px rgba(0,0,0,.85)" }}>{def.cost}</span>
        <span className="font-oswald text-[9px] px-1.5 py-0.5 rounded-full" style={{ background: borderColor, color: "#fff" }}>{def.rarity}</span>
      </div>

      <div className="relative mt-auto">
        <div className="font-oswald font-bold text-[16px] text-white leading-tight" style={{ textShadow: "0 1px 4px rgba(0,0,0,.9)" }}>{def.name}</div>
        <div className="font-spectral italic text-[10px] text-white/55 capitalize">{def.type}{def.subtype ? ` — ${def.subtype}` : ""}</div>

        {isCharacter && (
          <div className="flex items-center gap-3 mt-1.5 font-oswald font-bold text-[15px]">
            <span className="flex items-center gap-1" style={{ color: "#FF7062" }}><Sword size={13} />{def.atk}</span>
            <span className="flex items-center gap-1" style={{ color: "#7FB0E8" }}><Shield size={13} />{def.def}</span>
            <span className="flex items-center gap-1" style={{ color: "#5BC46A" }}><Heart size={14} color="#5BC46A" />{def.pv}</span>
          </div>
        )}

        {traits.length > 0 && (
          <div className="flex flex-wrap gap-1 mt-1.5">
            {traits.map((t) => (
              <span key={t} className="font-oswald text-[9px] px-1.5 py-0.5 rounded-full text-white" style={{ background: TRAIT_COLOR[t] ?? "rgba(255,255,255,.18)" }}>{t}</span>
            ))}
          </div>
        )}

        {def.baseAction && <div className="font-spectral italic text-[10px] text-white/80 mt-1.5 truncate">{def.baseAction.isSupport ? "✦" : "⚔"} {def.baseAction.name}</div>}
        {def.specialAttack && <div className="font-spectral italic text-[10px] mt-0.5 truncate" style={{ color: "#E8B84B" }}>★ {def.specialAttack.name} ({def.specialAttack.cost}V)</div>}
        {def.type === "object" && (
          <div className="font-oswald text-[10px] mt-1.5 px-1.5 py-0.5 rounded" style={{ background: "rgba(232,184,75,.15)", color: "#E8B84B" }}>
            {def.bonusAtk ? `+${def.bonusAtk} ATK ` : ""}{def.bonusDef ? `+${def.bonusDef} DEF` : ""}{def.subtype === "fruit" ? " 🍎" : ""}
          </div>
        )}
        {def.type === "ship" && <div className="font-oswald text-[10px] mt-1.5 px-1.5 py-0.5 rounded text-cyan-200" style={{ background: "rgba(34,180,200,.15)" }}>⚓ Navire</div>}
        {def.type === "counter" && <div className="font-oswald text-[10px] mt-1.5 px-1.5 py-0.5 rounded text-blue-200" style={{ background: "rgba(80,140,220,.18)" }}>🛡 Contre</div>}
        {def.type === "event" && <div className="font-oswald text-[10px] mt-1.5 px-1.5 py-0.5 rounded" style={{ background: "rgba(232,184,75,.12)", color: "#E8C53B" }}>✦ Événement</div>}
      </div>

      {highlight && <div className="absolute bottom-1 left-1/2 -translate-x-1/2 w-10 h-1 rounded-full bg-green-500 shadow-lg shadow-green-500/50 animate-pulse" />}
    </div>
  );
}
