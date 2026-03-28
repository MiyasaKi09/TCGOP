"use client";

import type { CaptainInstance, CaptainDef } from "@/types";

interface CaptainCardProps {
  captain: CaptainInstance;
  def: CaptainDef;
  isOpponent?: boolean;
  onClick?: () => void;
}

export default function CaptainCard({
  captain,
  def,
  isOpponent,
  onClick,
}: CaptainCardProps) {
  const side = captain.flipped ? def.verso : def.recto;
  const maxPv = captain.flipped ? def.verso.pv : def.recto.pv;
  const pvPercent = Math.max(0, (captain.currentPv / maxPv) * 100);

  return (
    <div
      onClick={onClick}
      className={`
        rounded-xl border-2 p-3 transition-all cursor-pointer
        ${captain.flipped ? "border-red-500 bg-red-950/30" : "border-amber-600 bg-amber-950/20"}
        ${isOpponent ? "opacity-90" : ""}
        w-48
      `}
    >
      <div className="flex justify-between items-center mb-1">
        <span className="font-bold text-sm">{def.name}</span>
        <span className="text-xs px-1.5 py-0.5 rounded bg-gray-700">
          {captain.flipped ? "VERSO" : "RECTO"}
        </span>
      </div>

      {/* PV Bar */}
      <div className="w-full h-3 bg-gray-700 rounded-full overflow-hidden mb-1">
        <div
          className={`h-full transition-all ${
            pvPercent > 50 ? "bg-green-500" : pvPercent > 25 ? "bg-yellow-500" : "bg-red-500"
          }`}
          style={{ width: `${pvPercent}%` }}
        />
      </div>
      <div className="text-xs text-center mb-2">
        PV: {captain.currentPv}/{maxPv}
      </div>

      {/* Stats */}
      <div className="flex justify-between text-xs">
        <span className="text-red-400">ATK: {side.atk}</span>
        <span className="text-blue-400">DEF: {side.def}</span>
      </div>

      {/* Passive name */}
      <div className="text-[10px] text-gray-400 mt-1 truncate">
        {side.passive.name}
      </div>

      {/* Status effects */}
      {captain.statusEffects.length > 0 && (
        <div className="flex gap-1 mt-1">
          {captain.statusEffects.map((e, i) => (
            <span key={i} className="text-sm">
              {e.type === "burn" ? "🔥" : e.type === "freeze" ? "❄" : "☠"}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}
