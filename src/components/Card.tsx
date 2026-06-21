"use client";

import type { DragEvent } from "react";
import type { CardInstance, CardDef } from "@/types";
import {
  CARD_ART, CARD_ART_FOCUS, faction, TRAIT_COLOR, RARITY_BORDER, hpColor,
} from "@/data/cardArt";
import { Sword, Shield, Heart, Crest } from "./icons";
import FullCard from "./FullCard";

interface CardProps {
  instance: CardInstance;
  def: CardDef;
  onClick?: () => void;
  selected?: boolean;
  highlight?: boolean;
  small?: boolean;
  /** Full-card width in px (hand). Defaults to 168. */
  width?: number;
  draggable?: boolean;
  onDragStart?: (e: DragEvent) => void;
  onDragEnd?: (e: DragEvent) => void;
}

const STATUS_ICON: Record<string, string> = {
  burn: "🔥", poison: "☠", freeze: "❄", desiccation: "🏜", trap: "💣", immobilize: "🌸",
};

export default function Card({ instance, def, onClick, selected, highlight, small, width = 168, draggable, onDragStart, onDragEnd }: CardProps) {
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
        data-zoomsrc=""
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

  // ===================== HAND / FULL — unified Claude-Design frame =====================
  const radius = Math.round((16 * width) / 300);
  return (
    <div
      onClick={onClick}
      draggable={draggable}
      onDragStart={onDragStart}
      onDragEnd={onDragEnd}
      className={`relative flex-shrink-0 cursor-grab active:cursor-grabbing transition-all duration-200 ${selected ? "scale-105 -translate-y-3" : "hover:-translate-y-1.5 hover:brightness-110"}`}
      style={{ width, borderRadius: radius, boxShadow: selected ? "0 0 0 2px #E8B84B, 0 0 22px 3px rgba(232,184,75,.55)" : undefined, WebkitUserSelect: "none", userSelect: "none" }}
    >
      <FullCard def={def} instance={instance} width={width} />
      {highlight && <div className="absolute -bottom-0.5 left-1/2 -translate-x-1/2 w-10 h-1 rounded-full bg-green-500 shadow-lg shadow-green-500/50 animate-pulse z-10" />}
    </div>
  );
}
