"use client";

import type { DragEvent } from "react";
import type { CardInstance, CardDef } from "@/types";
import {
  CARD_ART, CARD_ART_FOCUS, faction, RARITY_BORDER, hpColor,
} from "@/data/cardArt";
import { Crest } from "./icons";
import FullCard from "./FullCard";
import StatusBadges from "./StatusBadges";

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
  onMouseEnter?: () => void;
  onMouseLeave?: () => void;
}

export default function Card({ instance, def, onClick, selected, highlight, small, width = 168, draggable, onDragStart, onDragEnd, onMouseEnter, onMouseLeave }: CardProps) {
  const art = CARD_ART[def.id];
  const fac = faction(def.faction);
  const isCharacter = def.type === "character";
  const hpRatio = isCharacter ? instance.currentPv / (def.pv ?? 1) : 1;
  const hpc = hpColor(hpRatio);
  const borderColor = RARITY_BORDER[def.rarity] ?? fac.border;

  // ===================== BOARD TOKEN (full illustration) =====================
  // Art-first: the board shows the artwork only. Details (stats, actions) surface
  // on click via ActionMenu / CardDetail. We keep only essential at-a-glance cues:
  // a thin HP bar WHEN damaged, status badges, equipment count, tapped veil.
  if (small) {
    const maxPv = def.pv ?? 1;
    const damaged = isCharacter && instance.currentPv < maxPv;
    return (
      <div
        onClick={onClick}
        data-zoomsrc=""
        className={`relative w-full h-full rounded-xl overflow-hidden cursor-pointer transition-all duration-150 ${instance.tapped ? "saturate-[.7] brightness-90" : "hover:brightness-110"}`}
        style={{ background: "var(--color-ink-800)", boxShadow: "var(--shadow-token)" }}
      >
        {art ? (
          <div className="absolute inset-0" style={{ backgroundImage: `url('${art}')`, backgroundSize: "cover", backgroundPosition: CARD_ART_FOCUS[def.id] ?? "50% 16%", backgroundRepeat: "no-repeat" }} />
        ) : (
          <>
            <div className="absolute inset-0" style={{ background: fac.frame }} />
            <div className="absolute inset-0 flex items-center justify-center opacity-20">
              <Crest which={fac.crest} size={56} color="#fff" />
            </div>
          </>
        )}

        {/* status + equipment (compact, top-right) */}
        {(instance.statusEffects.length > 0 || instance.attachedObjects.length > 0) && (
          <div className="absolute top-1 right-1 flex flex-col items-end gap-0.5 leading-none">
            <StatusBadges effects={instance.statusEffects} compact />
            {instance.attachedObjects.length > 0 && (
              <span className="font-oswald font-bold text-[9px] px-1 rounded text-gold" style={{ background: "rgba(232,184,75,.25)", border: "1px solid var(--ink-edge)" }}>⚔{instance.attachedObjects.length}</span>
            )}
          </div>
        )}

        {/* damaged → thin HP bar only */}
        {damaged && (
          <div className="absolute left-1.5 right-1.5 bottom-1 h-1 rounded-full overflow-hidden" style={{ background: "rgba(0,0,0,.6)" }}>
            <div className="h-full rounded-full" style={{ width: `${Math.max(0, (instance.currentPv / maxPv) * 100)}%`, background: hpc }} />
          </div>
        )}

        {/* tapped veil */}
        {instance.tapped && (
          <div className="absolute inset-0 flex items-center justify-center" style={{ background: "rgba(5,7,11,.5)" }}>
            <span className="font-oswald text-[8px] tracking-widest uppercase text-white px-2 py-0.5 rounded-full" style={{ background: "rgba(0,0,0,.6)" }}>Incliné</span>
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
      onMouseEnter={onMouseEnter}
      onMouseLeave={onMouseLeave}
      className={`relative flex-shrink-0 cursor-grab active:cursor-grabbing transition-all duration-200 ${selected ? "scale-105 -translate-y-3" : "hover:-translate-y-1.5 hover:brightness-110"}`}
      style={{ width, borderRadius: radius, boxShadow: selected ? "0 0 0 2px var(--color-gold), 0 0 22px 3px rgba(232,184,75,.55)" : undefined, WebkitUserSelect: "none", userSelect: "none" }}
    >
      <FullCard def={def} instance={instance} width={width} />
      {highlight && <div className="absolute -bottom-0.5 left-1/2 -translate-x-1/2 w-10 h-1 rounded-full bg-green-500 shadow-lg shadow-green-500/50 animate-pulse z-10" />}
    </div>
  );
}
