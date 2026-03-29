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
        w-[9.5rem] h-52 rounded-xl border-2 flex items-center justify-center transition-all duration-200 relative
        ${instance
          ? "border-gray-600/60 bg-gray-900/40"
          : "border-dashed border-gray-700/40"
        }
        ${isValidTarget
          ? "border-red-500 bg-red-500/10 ring-2 ring-red-400/70 shadow-lg shadow-red-500/20 animate-pulse"
          : ""
        }
        ${isValidDeploy
          ? "border-green-500 bg-green-500/8 ring-2 ring-green-400/60 shadow-lg shadow-green-500/15"
          : ""
        }
        ${!instance && !isValidDeploy && !isValidTarget
          ? "bg-gray-900/20 hover:bg-gray-800/20 hover:border-gray-600/30"
          : ""
        }
        cursor-pointer
      `}
    >
      {instance && def ? (
        <div className="w-full h-full p-0.5 animate-card-enter">
          <Card instance={instance} def={def} small />
        </div>
      ) : (
        <div className="text-center select-none">
          <span className="text-sm text-gray-600/60 font-mono font-bold">{slot}</span>
          <br />
          <span className={`text-[10px] uppercase tracking-widest ${isFront ? "text-red-900/40" : "text-blue-900/40"}`}>
            {isFront ? "Avant" : "Arriere"}
          </span>
        </div>
      )}
    </div>
  );
}
