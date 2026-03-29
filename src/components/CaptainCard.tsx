"use client";

import type { CaptainInstance, CaptainDef } from "@/types";

interface CaptainCardProps {
  captain: CaptainInstance;
  def: CaptainDef;
  isOpponent?: boolean;
  onClick?: () => void;
}

const STATUS_DISPLAY: Record<string, { icon: string; class: string }> = {
  burn: { icon: "🔥", class: "animate-status-burn" },
  poison: { icon: "☠", class: "text-green-400" },
  freeze: { icon: "❄", class: "animate-status-freeze" },
};

export default function CaptainCard({
  captain,
  def,
  isOpponent,
  onClick,
}: CaptainCardProps) {
  const side = captain.flipped ? def.verso : def.recto;
  const maxPv = captain.flipped ? def.verso.pv : def.recto.pv;
  const pvPercent = Math.max(0, (captain.currentPv / maxPv) * 100);
  const isLowHp = pvPercent <= 25;
  const isMidHp = pvPercent <= 50 && pvPercent > 25;

  return (
    <div
      onClick={onClick}
      className={`
        relative rounded-xl border-2 p-3 transition-all duration-300 cursor-pointer overflow-hidden
        ${captain.flipped
          ? "border-red-500 bg-gradient-to-b from-red-950/40 to-gray-900/90 rarity-glow-CAP"
          : "border-amber-600 bg-gradient-to-b from-amber-950/30 to-gray-900/90"
        }
        ${captain.tapped ? "opacity-50 saturate-50" : ""}
        ${isOpponent ? "opacity-90" : "hover:brightness-110"}
        w-52
      `}
    >
      {/* Background pattern */}
      <div className="absolute inset-0 opacity-[0.03] pointer-events-none"
        style={{ backgroundImage: "url(\"data:image/svg+xml,%3Csvg width='20' height='20' xmlns='http://www.w3.org/2000/svg'%3E%3Cpath d='M0 0h20v20H0z' fill='none'/%3E%3Cpath d='M10 0v20M0 10h20' stroke='white' stroke-width='0.5'/%3E%3C/svg%3E\")" }}
      />

      {/* Header */}
      <div className="flex justify-between items-start mb-2 relative">
        <div>
          <div className="font-bold text-sm text-white leading-tight">{def.name}</div>
          <div className="text-[9px] text-gray-500 uppercase tracking-widest mt-0.5">{def.faction}</div>
        </div>
        <span className={`
          text-[9px] px-2 py-0.5 rounded-full font-bold uppercase tracking-wider
          ${captain.flipped
            ? "bg-red-600/80 text-red-100 shadow-lg shadow-red-900/30"
            : "bg-amber-600/80 text-amber-100 shadow-lg shadow-amber-900/30"
          }
        `}>
          {captain.flipped ? "Verso" : "Recto"}
        </span>
      </div>

      {/* PV Bar */}
      <div className="relative mb-2">
        <div className={`w-full h-2.5 rounded-full bg-gray-800/80 overflow-hidden border border-gray-700/50 ${isLowHp ? "animate-health-low" : ""}`}>
          <div
            className="h-full rounded-full health-bar"
            style={{
              width: `${pvPercent}%`,
              ["--hp-color" as string]: pvPercent > 50 ? "#22c55e" : pvPercent > 25 ? "#eab308" : "#ef4444",
              ["--hp-color-light" as string]: pvPercent > 50 ? "#4ade80" : pvPercent > 25 ? "#facc15" : "#f87171",
            }}
          />
        </div>
        <div className={`text-[10px] text-center mt-1 font-bold stat-badge ${isLowHp ? "text-red-400" : isMidHp ? "text-yellow-400" : "text-green-400"}`}>
          {captain.currentPv} / {maxPv} PV
        </div>
      </div>

      {/* Stats */}
      <div className="flex gap-2 mb-2">
        <div className="flex-1 text-center py-1 rounded-lg bg-red-950/30 border border-red-800/20">
          <div className="text-[8px] text-gray-500 uppercase tracking-wider">ATK</div>
          <div className="text-red-400 font-bold text-sm stat-badge">{side.atk}</div>
        </div>
        <div className="flex-1 text-center py-1 rounded-lg bg-blue-950/30 border border-blue-800/20">
          <div className="text-[8px] text-gray-500 uppercase tracking-wider">DEF</div>
          <div className="text-blue-400 font-bold text-sm stat-badge">{side.def}</div>
        </div>
      </div>

      {/* Passive */}
      <div className="text-[9px] text-purple-300/80 bg-purple-950/20 rounded-md px-2 py-1 border border-purple-800/15 truncate">
        ✦ {side.passive.name}
      </div>

      {/* Traits (verso only) */}
      {captain.flipped && def.verso.traits && def.verso.traits.length > 0 && (
        <div className="flex gap-1 mt-1.5">
          {def.verso.traits.map((t: string) => (
            <span key={t} className="text-[9px] bg-gray-700/50 px-1.5 py-0.5 rounded-full text-gray-300" title={t}>
              {t === "cursed" ? "😈" : t === "logia" ? "💨" : t === "conqueror" ? "👑" : t === "rush" ? "⚡" : t}
            </span>
          ))}
        </div>
      )}

      {/* Status effects */}
      {captain.statusEffects.length > 0 && (
        <div className="flex gap-1.5 mt-2">
          {captain.statusEffects.map((e, i) => {
            const cfg = STATUS_DISPLAY[e.type] ?? { icon: "?", class: "" };
            return <span key={i} className={`text-sm ${cfg.class}`}>{cfg.icon}</span>;
          })}
        </div>
      )}

      {/* Tapped overlay */}
      {captain.tapped && (
        <div className="absolute inset-0 bg-black/40 rounded-xl flex items-center justify-center backdrop-blur-[1px]">
          <span className="text-[9px] text-gray-300 font-bold bg-black/60 px-2 py-1 rounded-full tracking-wider uppercase">Incline</span>
        </div>
      )}
    </div>
  );
}
