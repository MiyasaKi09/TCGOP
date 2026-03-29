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

const RARITY_COLORS: Record<string, string> = {
  C: "border-gray-600",
  U: "border-green-600",
  R: "border-blue-500",
  SR: "border-purple-500",
  L: "border-yellow-400",
  CAP: "border-red-500",
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

export default function Card({ instance, def, onClick, selected, highlight, small }: CardProps) {
  const borderColor = RARITY_COLORS[def.rarity] ?? "border-gray-600";
  const isCharacter = def.type === "character";

  if (small) {
    // Compact version for board slots
    return (
      <div
        onClick={onClick}
        className={`
          relative rounded-lg border-2 p-1.5 cursor-pointer transition-all w-full h-full
          ${borderColor}
          ${selected ? "ring-2 ring-amber-400" : ""}
          ${highlight ? "ring-2 ring-green-400" : ""}
          ${instance.tapped ? "opacity-60" : ""}
          bg-gray-900
        `}
      >
        {/* Cost */}
        <div className="absolute -top-1.5 -left-1.5 w-5 h-5 rounded-full bg-blue-700 flex items-center justify-center text-[9px] font-bold">
          {def.cost}
        </div>

        {/* Name */}
        <div className="font-bold text-[11px] leading-tight mt-1 truncate pr-1">{def.name}</div>

        {/* Stats */}
        {isCharacter && (
          <>
            <div className="flex justify-between mt-1 text-[11px]">
              <span className="text-red-400 font-semibold">⚔{def.atk}</span>
              <span className="text-blue-400 font-semibold">🛡{def.def}</span>
            </div>
            <div className="text-xs text-center mt-0.5">
              <span className={`font-bold ${instance.currentPv <= (def.pv ?? 0) / 3 ? "text-red-400" : "text-green-400"}`}>
                ❤{instance.currentPv}/{def.pv}
              </span>
            </div>
          </>
        )}

        {/* Traits compact */}
        {def.traits && def.traits.length > 0 && (
          <div className="flex gap-0.5 mt-0.5 text-[10px]">
            {def.traits.slice(0, 4).map((t) => (
              <span key={t} title={t}>{TRAIT_ICONS[t] ?? t[0]}</span>
            ))}
          </div>
        )}

        {/* Equipment indicator */}
        {instance.attachedObjects.length > 0 && (
          <div className="text-[10px] text-amber-400 font-semibold">⚔x{instance.attachedObjects.length}</div>
        )}

        {/* Status */}
        {instance.statusEffects.length > 0 && (
          <div className="flex text-[8px]">
            {instance.statusEffects.map((e, i) => (
              <span key={i}>{e.type === "burn" ? "🔥" : e.type === "poison" ? "☠" : e.type === "freeze" ? "❄" : "🏜"}</span>
            ))}
          </div>
        )}

        {instance.tapped && (
          <div className="absolute inset-0 bg-black/30 rounded-lg flex items-center justify-center">
            <span className="text-[8px] text-gray-300 font-bold bg-black/50 px-1 rounded">INCLINE</span>
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
        relative rounded-lg border-2 p-2 cursor-pointer transition-all
        ${borderColor}
        ${selected ? "ring-2 ring-amber-400 scale-105 -translate-y-2" : "hover:-translate-y-1"}
        ${highlight ? "ring-2 ring-green-400" : ""}
        w-40 bg-gray-900 hover:bg-gray-800 flex-shrink-0
      `}
    >
      {/* Cost badge */}
      <div className="absolute -top-2 -left-2 w-6 h-6 rounded-full bg-blue-700 flex items-center justify-center text-xs font-bold z-10">
        {def.cost}
      </div>

      {/* Type + Name */}
      <div className="font-bold text-sm leading-tight mt-1">{def.name}</div>
      <div className="text-xs text-gray-500 capitalize">{def.type}{def.subtype ? ` - ${def.subtype}` : ""}</div>

      {/* Stats for characters */}
      {isCharacter && (
        <div className="flex justify-between mt-2 text-sm">
          <span className="text-red-400 font-semibold">⚔{def.atk}</span>
          <span className="text-blue-400 font-semibold">🛡{def.def}</span>
          <span className="text-green-400 font-semibold">❤{def.pv}</span>
        </div>
      )}

      {/* Traits */}
      {def.traits && def.traits.length > 0 && (
        <div className="flex flex-wrap gap-0.5 mt-1">
          {def.traits.map((t) => (
            <span key={t} className="text-xs" title={t}>
              {TRAIT_ICONS[t] ?? t}
            </span>
          ))}
        </div>
      )}

      {/* Base action preview */}
      {def.baseAction && (
        <div className="text-[11px] text-green-400 mt-1.5 truncate">
          ⚔ {def.baseAction.name}
        </div>
      )}

      {/* Special preview */}
      {def.specialAttack && (
        <div className="text-[11px] text-red-400 truncate">
          💥 {def.specialAttack.name} ({def.specialAttack.cost}V)
        </div>
      )}

      {/* Object info */}
      {def.type === "object" && (
        <div className="text-[10px] text-amber-300 mt-1">
          {def.bonusAtk ? `+${def.bonusAtk} ATK` : ""}
          {def.bonusDef ? ` +${def.bonusDef} DEF` : ""}
        </div>
      )}

      {/* Event effect preview */}
      {def.type === "event" && def.eventEffect && (
        <div className="text-[9px] text-yellow-400 mt-1">
          {def.eventEffect.type === "gainWill" && `+${def.eventEffect.amount} Vol.`}
          {def.eventEffect.type === "draw" && `Pioche ${def.eventEffect.amount}`}
          {def.eventEffect.type === "damageEnemies" && `${def.eventEffect.amount} deg.`}
          {def.eventEffect.type === "buffAllies" && `+${def.eventEffect.amount} ${def.eventEffect.stat}`}
          {def.eventEffect.type === "healAlly" && `+${def.eventEffect.amount} PV`}
        </div>
      )}

      {/* Counter info */}
      {def.type === "counter" && def.counterEffect && (
        <div className="text-[9px] text-blue-300 mt-1">
          {def.counterEffect.type === "survive" ? "Survie 1 PV" : `-${def.counterEffect.amount} deg.`}
        </div>
      )}

      {/* Ship info */}
      {def.type === "ship" && (
        <div className="text-[9px] text-cyan-300 mt-1 truncate">🚢 Navire</div>
      )}

      {/* Playability indicator */}
      {!highlight && (
        <div className="absolute bottom-0 right-0 w-2 h-2 rounded-full bg-gray-700 m-1" />
      )}
      {highlight && (
        <div className="absolute bottom-0 right-0 w-2 h-2 rounded-full bg-green-500 m-1 animate-pulse" />
      )}
    </div>
  );
}
