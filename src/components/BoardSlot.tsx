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
}

export default function BoardSlot({
  slot,
  instance,
  def,
  isPlayerSide,
  isValidTarget,
  isValidDeploy,
  onClick,
}: BoardSlotProps) {
  const isFront = slot.startsWith("V");

  return (
    <div
      onClick={onClick}
      className={`
        w-32 h-40 rounded-lg border-2 flex items-center justify-center transition-all
        ${instance ? "border-gray-600" : "border-dashed border-gray-700"}
        ${isValidTarget ? "border-red-500 bg-red-500/10 ring-2 ring-red-400" : ""}
        ${isValidDeploy ? "border-green-500 bg-green-500/10 ring-2 ring-green-400" : ""}
        ${!instance && !isValidDeploy && !isValidTarget ? "bg-gray-900/30" : ""}
        cursor-pointer
      `}
    >
      {instance && def ? (
        <Card instance={instance} def={def} small />
      ) : (
        <span className="text-xs text-gray-600">
          {slot}
          <br />
          <span className="text-[10px]">{isFront ? "Avant" : "Arriere"}</span>
        </span>
      )}
    </div>
  );
}
