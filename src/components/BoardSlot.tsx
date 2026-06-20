"use client";

import type { CardInstance, CardDef, Slot } from "@/types";
import Card from "./Card";

interface BoardSlotProps {
  slot: Slot;
  instance: CardInstance | null;
  def: CardDef | null;
  isPlayerSide: boolean;
  isValidTarget?: boolean;
  isValidDeploy?: boolean;
  onClick?: () => void;
  onDrop?: () => void;
}

export default function BoardSlot({
  slot,
  instance,
  def,
  isValidTarget,
  isValidDeploy,
  onClick,
  onDrop,
}: BoardSlotProps) {
  const isFront = slot.startsWith("V");
  const ring = isValidTarget ? "ring-target" : isValidDeploy ? "ring-deploy" : "";

  return (
    <div
      onClick={onClick}
      onDragOver={(e) => { if (onDrop) e.preventDefault(); }}
      onDrop={(e) => { e.preventDefault(); onDrop?.(); }}
      className={`relative w-[5.5rem] h-[7.3rem] rounded-xl flex items-center justify-center transition-all duration-200 cursor-pointer ${ring}`}
      style={
        instance
          ? undefined
          : { border: "1.5px dashed rgba(255,255,255,.16)", background: "rgba(6,10,16,.28)" }
      }
    >
      {instance && def ? (
        <div className="w-full h-full animate-card-enter">
          <Card instance={instance} def={def} small />
        </div>
      ) : (
        <div className="text-center select-none leading-tight">
          <span className="font-oswald text-[11px] text-white/30 font-bold">{slot}</span>
          <br />
          <span className="font-oswald text-[8px] uppercase tracking-widest" style={{ color: isFront ? "rgba(232,149,74,.4)" : "rgba(91,151,216,.4)" }}>
            {isFront ? "Avant" : "Arrière"}
          </span>
        </div>
      )}
    </div>
  );
}
