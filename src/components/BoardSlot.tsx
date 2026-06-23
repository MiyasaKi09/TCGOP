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
  /** Cell is part of a Zone/AoE attack's impact area. */
  isImpact?: boolean;
  /** A selection is active and this cell is not an eligible choice. */
  isDimmed?: boolean;
  onClick?: () => void;
  onDrop?: () => void;
}

export default function BoardSlot({
  slot,
  instance,
  def,
  isValidTarget,
  isValidDeploy,
  isImpact,
  isDimmed,
  onClick,
  onDrop,
}: BoardSlotProps) {
  const isFront = slot.startsWith("V");
  const ring = isImpact ? "ring-impact" : isValidTarget ? "ring-target" : isValidDeploy ? "ring-deploy" : "";

  return (
    <div
      onClick={onClick}
      onDragOver={(e) => { if (onDrop) e.preventDefault(); }}
      onDrop={(e) => { e.preventDefault(); onDrop?.(); }}
      data-inst={instance?.instanceId}
      className={`relative flex-1 min-w-0 max-w-[120px] h-[3.9rem] rounded-xl flex items-center justify-center transition-all duration-200 cursor-pointer ${ring} ${isDimmed ? "slot-dim" : ""} ${instance ? "" : "slot-empty"}`}
    >
      {instance && def ? (
        <div className="w-full h-full unit-land">
          <Card instance={instance} def={def} small />
        </div>
      ) : (
        <div className="text-center select-none leading-none">
          <span className="font-oswald text-[10px] text-white/35 font-bold">{slot}</span>
          <div className={`font-oswald text-[7px] uppercase tracking-widest mt-0.5 ${isFront ? "text-pirate/60" : "text-marine/60"}`}>
            {isFront ? "Avant" : "Arrière"}
          </div>
        </div>
      )}
    </div>
  );
}
