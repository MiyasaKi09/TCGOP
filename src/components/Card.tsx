"use client";

import type { CardInstance, CardDef } from "@/types";

interface CardProps {
  instance: CardInstance;
  def: CardDef;
  onClick?: () => void;
  selected?: boolean;
  highlight?: boolean;
  small?: boolean;
}

const RARITY_BORDER: Record<string, string> = {
  C: "border-gray-600",
  U: "border-green-600",
  R: "border-blue-500",
  SR: "border-purple-500",
  L: "border-yellow-400",
  CAP: "border-red-500",
};

const RARITY_GLOW: Record<string, string> = {
  C: "rarity-glow-C",
  U: "rarity-glow-U",
  R: "rarity-glow-R",
  SR: "rarity-glow-SR",
  L: "rarity-glow-L",
  CAP: "rarity-glow-CAP",
};

const RARITY_LABEL_BG: Record<string, string> = {
  C: "bg-gray-600",
  U: "bg-green-700",
  R: "bg-blue-600",
  SR: "bg-purple-600",
  L: "bg-yellow-600",
  CAP: "bg-red-600",
};

const TRAIT_ICONS: Record<string, string> = {
  shield: "🛡",
  range: "🏹",
  stealth: "👻",
  rush: "⚡",
  cursed: "😈",
  logia: "💨",
  piercing: "🗡",
  conqueror: "👑",
};

const STATUS_CONFIG: Record<string, { icon: string; class: string }> = {
  burn: { icon: "🔥", class: "animate-status-burn" },
  poison: { icon: "☠", class: "text-green-400" },
  freeze: { icon: "❄", class: "animate-status-freeze" },
  desiccation: { icon: "🏜", class: "text-yellow-600" },
  trap: { icon: "💣", class: "text-orange-400" },
  immobilize: { icon: "🌸", class: "text-pink-400" },
};

export default function Card({ instance, def, onClick, selected, highlight, small }: CardProps) {
  const borderColor = RARITY_BORDER[def.rarity] ?? "border-gray-600";
  const glowClass = RARITY_GLOW[def.rarity] ?? "";
  const isCharacter = def.type === "character";
  const hpRatio = isCharacter ? (instance.currentPv / (def.pv ?? 1)) : 1;

  if (small) {
    return (
      <div
        onClick={onClick}
        className={`
          relative rounded-lg border-2 p-1.5 cursor-pointer transition-all duration-200 w-full h-full
          ${borderColor} ${glowClass}
          ${selected ? "ring-2 ring-amber-400 ring-offset-1 ring-offset-gray-950" : ""}
          ${highlight ? "ring-2 ring-green-400 ring-offset-1 ring-offset-gray-950" : ""}
          ${instance.tapped ? "opacity-50 saturate-50" : "hover:brightness-110"}
          bg-gradient-to-b from-gray-800/90 to-gray-900/95
        `}
      >
        {/* Shimmer overlay for rare+ */}
        {(def.rarity === "SR" || def.rarity === "L") && (
          <div className="absolute inset-0 rounded-lg animate-shimmer pointer-events-none" />
        )}

        {/* Cost badge */}
        <div className="absolute -top-2 -left-2 w-5 h-5 rounded-full bg-blue-600 flex items-center justify-center text-[9px] font-bold shadow-lg shadow-blue-900/50 z-10">
          {def.cost}
        </div>

        {/* Name */}
        <div className="font-bold text-[11px] leading-tight mt-1 truncate pr-1 text-gray-100">{def.name}</div>

        {/* Stats */}
        {isCharacter && (
          <>
            <div className="flex justify-between mt-1 text-[11px] stat-badge">
              <span className="text-red-400 font-bold">⚔{def.atk}</span>
              <span className="text-blue-400 font-bold">🛡{def.def}</span>
            </div>
            <div className="mt-1">
              <div className="w-full h-1.5 rounded-full bg-gray-700/80 overflow-hidden">
                <div
                  className="h-full rounded-full transition-all duration-500 health-bar"
                  style={{
                    width: `${Math.max(0, hpRatio * 100)}%`,
                    ["--hp-color" as string]: hpRatio > 0.5 ? "#22c55e" : hpRatio > 0.25 ? "#eab308" : "#ef4444",
                    ["--hp-color-light" as string]: hpRatio > 0.5 ? "#4ade80" : hpRatio > 0.25 ? "#facc15" : "#f87171",
                  }}
                />
              </div>
              <div className={`text-[9px] text-center mt-0.5 font-bold stat-badge ${hpRatio <= 0.25 ? "text-red-400 animate-health-low" : "text-green-400"}`}>
                {instance.currentPv}/{def.pv}
              </div>
            </div>
          </>
        )}

        {/* Traits */}
        {def.traits && def.traits.length > 0 && (
          <div className="flex gap-0.5 mt-0.5 text-[10px]">
            {def.traits.slice(0, 4).map((t) => (
              <span key={t} title={t} className="drop-shadow-sm">{TRAIT_ICONS[t] ?? t[0]}</span>
            ))}
          </div>
        )}

        {/* Equipment */}
        {instance.attachedObjects.length > 0 && (
          <div className="text-[9px] text-amber-400 font-bold mt-0.5">
            <span className="bg-amber-900/40 px-1 rounded">⚔x{instance.attachedObjects.length}</span>
          </div>
        )}

        {/* Status effects */}
        {instance.statusEffects.length > 0 && (
          <div className="flex gap-0.5 text-[10px] mt-0.5">
            {instance.statusEffects.map((e, i) => {
              const cfg = STATUS_CONFIG[e.type] ?? { icon: "?", class: "" };
              return <span key={i} className={cfg.class}>{cfg.icon}</span>;
            })}
          </div>
        )}

        {/* Tapped overlay */}
        {instance.tapped && (
          <div className="absolute inset-0 bg-black/40 rounded-lg flex items-center justify-center backdrop-blur-[1px]">
            <span className="text-[8px] text-gray-300 font-bold bg-black/60 px-1.5 py-0.5 rounded-full tracking-wider uppercase">Incline</span>
          </div>
        )}
      </div>
    );
  }

  // Full version for hand
  return (
    <div
      onClick={onClick}
      className={`
        relative rounded-xl border-2 p-2.5 cursor-pointer transition-all duration-200
        ${borderColor} ${glowClass}
        ${selected ? "ring-2 ring-amber-400 ring-offset-2 ring-offset-gray-950 scale-105 -translate-y-3" : "hover:-translate-y-1.5 hover:brightness-110"}
        ${highlight ? "ring-2 ring-green-400 ring-offset-1 ring-offset-gray-950" : ""}
        w-[10.5rem] bg-gradient-to-b from-gray-800/80 to-gray-900 flex-shrink-0
      `}
    >
      {/* Shimmer overlay for rare+ */}
      {(def.rarity === "SR" || def.rarity === "L") && (
        <div className="absolute inset-0 rounded-xl animate-shimmer pointer-events-none" />
      )}

      {/* Cost badge */}
      <div className="absolute -top-2.5 -left-2.5 w-7 h-7 rounded-full bg-gradient-to-br from-blue-500 to-blue-700 flex items-center justify-center text-xs font-bold z-10 shadow-lg shadow-blue-900/50 border border-blue-400/30">
        {def.cost}
      </div>

      {/* Rarity badge */}
      <div className={`absolute -top-1.5 -right-1.5 px-1.5 py-0.5 rounded-full text-[8px] font-bold z-10 ${RARITY_LABEL_BG[def.rarity] ?? "bg-gray-600"} shadow-md`}>
        {def.rarity}
      </div>

      {/* Type + Name */}
      <div className="font-bold text-sm leading-tight mt-1 text-white">{def.name}</div>
      <div className="text-[10px] text-gray-500 capitalize tracking-wide">{def.type}{def.subtype ? ` — ${def.subtype}` : ""}</div>

      {/* Stats for characters */}
      {isCharacter && (
        <div className="flex justify-between mt-2 px-1">
          <div className="flex flex-col items-center">
            <span className="text-[9px] text-gray-500 uppercase tracking-wider">ATK</span>
            <span className="text-red-400 font-bold text-sm stat-badge">{def.atk}</span>
          </div>
          <div className="flex flex-col items-center">
            <span className="text-[9px] text-gray-500 uppercase tracking-wider">DEF</span>
            <span className="text-blue-400 font-bold text-sm stat-badge">{def.def}</span>
          </div>
          <div className="flex flex-col items-center">
            <span className="text-[9px] text-gray-500 uppercase tracking-wider">PV</span>
            <span className="text-green-400 font-bold text-sm stat-badge">{def.pv}</span>
          </div>
        </div>
      )}

      {/* Traits */}
      {def.traits && def.traits.length > 0 && (
        <div className="flex flex-wrap gap-1 mt-1.5">
          {def.traits.map((t) => (
            <span key={t} className="text-[10px] bg-gray-700/60 px-1 py-0.5 rounded-full" title={t}>
              {TRAIT_ICONS[t] ?? t}
            </span>
          ))}
        </div>
      )}

      {/* Base action preview */}
      {def.baseAction && (
        <div className="text-[10px] text-green-400/90 mt-2 truncate border-l-2 border-green-600/40 pl-1.5">
          {def.baseAction.isSupport ? "✦" : "⚔"} {def.baseAction.name}
        </div>
      )}

      {/* Special preview */}
      {def.specialAttack && (
        <div className="text-[10px] text-red-400/90 truncate border-l-2 border-red-600/40 pl-1.5 mt-0.5">
          💥 {def.specialAttack.name} <span className="text-gray-500">({def.specialAttack.cost}V)</span>
        </div>
      )}

      {/* Object info */}
      {def.type === "object" && (
        <div className="text-[10px] text-amber-300 mt-1.5 bg-amber-900/20 rounded px-1.5 py-0.5">
          {def.bonusAtk ? `+${def.bonusAtk} ATK` : ""}
          {def.bonusDef ? ` +${def.bonusDef} DEF` : ""}
          {def.subtype === "fruit" && " 🍎"}
        </div>
      )}

      {/* Event effect preview */}
      {def.type === "event" && def.eventEffect && (
        <div className="text-[9px] text-yellow-400/90 mt-1.5 bg-yellow-900/15 rounded px-1.5 py-0.5">
          {def.eventEffect.type === "gainWill" && `+${def.eventEffect.amount} Vol.`}
          {def.eventEffect.type === "draw" && `Pioche ${def.eventEffect.amount}`}
          {def.eventEffect.type === "damageEnemies" && `${def.eventEffect.amount} deg.`}
          {def.eventEffect.type === "buffAllies" && `+${def.eventEffect.amount} ${def.eventEffect.stat}`}
          {def.eventEffect.type === "healAlly" && `+${def.eventEffect.amount} PV`}
        </div>
      )}

      {/* Counter info */}
      {def.type === "counter" && def.counterEffect && (
        <div className="text-[9px] text-blue-300/90 mt-1.5 bg-blue-900/20 rounded px-1.5 py-0.5">
          🛡 {def.counterEffect.type === "survive" ? "Survie 1 PV" : `-${def.counterEffect.amount} deg.`}
        </div>
      )}

      {/* Ship info */}
      {def.type === "ship" && (
        <div className="text-[9px] text-cyan-300/90 mt-1.5 bg-cyan-900/15 rounded px-1.5 py-0.5 truncate">
          🚢 Navire
        </div>
      )}

      {/* Playability glow indicator */}
      {highlight && (
        <div className="absolute -bottom-0.5 left-1/2 -translate-x-1/2 w-8 h-1 rounded-full bg-green-500 shadow-lg shadow-green-500/50 animate-pulse" />
      )}
    </div>
  );
}
