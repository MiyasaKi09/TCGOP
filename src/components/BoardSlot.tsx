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
        w-36 h-48 rounded-lg border-2 flex items-center justify-center transition-all
        ${instance ? "border-gray-600" : "border-dashed border-gray-700"}
        ${isValidTarget ? "border-red-500 bg-red-500/10 ring-2 ring-red-400 animate-pulse" : ""}
        ${isValidDeploy ? "border-green-500 bg-green-500/10 ring-2 ring-green-400" : ""}
        ${!instance && !isValidDeploy && !isValidTarget ? "bg-gray-900/30" : ""}
        cursor-pointer hover:brightness-110
      `}
    >
      {instance && def ? (
        <Card instance={instance} def={def} small />
      ) : (
        <div className="text-center">
          <span className="text-sm text-gray-600 font-mono">{slot}</span>
          <br />
          <span className="text-xs text-gray-700">{isFront ? "Avant" : "Arriere"}</span>
        </div>
      )}
    </div>
  );
}
