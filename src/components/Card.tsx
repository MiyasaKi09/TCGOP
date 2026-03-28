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

const TYPE_ICONS: Record<string, string> = {
  character: "👤",
  object: "⚔",
  ship: "🚢",
  event: "⚡",
  counter: "🛡",
};

export default function Card({
  instance,
  def,
  onClick,
  selected,
  highlight,
  small,
}: CardProps) {
  const borderColor = RARITY_COLORS[def.rarity] ?? "border-gray-600";
  const isCharacter = def.type === "character";

  return (
    <div
      onClick={onClick}
      className={`
        relative rounded-lg border-2 p-2 cursor-pointer transition-all
        ${borderColor}
        ${selected ? "ring-2 ring-amber-400 scale-105" : ""}
        ${highlight ? "ring-2 ring-green-400" : ""}
        ${instance.tapped ? "opacity-60 rotate-6" : ""}
        ${small ? "w-20 text-[10px]" : "w-28 text-xs"}
        bg-gray-900 hover:bg-gray-800
      `}
    >
      {/* Cost badge */}
      <div className="absolute -top-2 -left-2 w-6 h-6 rounded-full bg-blue-700 flex items-center justify-center text-xs font-bold">
        {def.cost}
      </div>

      {/* Type icon */}
      <div className="text-right text-[10px]">{TYPE_ICONS[def.type]}</div>

      {/* Name */}
      <div className={`font-bold truncate ${small ? "text-[9px]" : "text-xs"}`}>
        {def.name}
      </div>

      {/* Stats for characters */}
      {isCharacter && (
        <div className="flex justify-between mt-1 text-[10px]">
          <span className="text-red-400">⚔{def.atk}</span>
          <span className="text-blue-400">🛡{def.def}</span>
          <span className="text-green-400">❤{instance.currentPv}/{def.pv}</span>
        </div>
      )}

      {/* Traits */}
      {def.traits && def.traits.length > 0 && (
        <div className="flex flex-wrap gap-0.5 mt-1">
          {def.traits.map((t) => (
            <span
              key={t}
              className="px-1 py-0 rounded bg-gray-700 text-[8px]"
            >
              {t}
            </span>
          ))}
        </div>
      )}

      {/* Equipment/Object info */}
      {def.type === "object" && (
        <div className="text-[9px] text-amber-300 mt-1">
          {def.bonusAtk ? `+${def.bonusAtk} ATK` : ""}
          {def.subtype ? ` [${def.subtype}]` : ""}
        </div>
      )}

      {/* Attached objects indicator */}
      {instance.attachedObjects.length > 0 && (
        <div className="text-[9px] text-amber-400 mt-0.5">
          ⚔ x{instance.attachedObjects.length}
        </div>
      )}

      {/* Status effects */}
      {instance.statusEffects.length > 0 && (
        <div className="flex gap-0.5 mt-0.5">
          {instance.statusEffects.map((e, i) => (
            <span key={i} className="text-[10px]">
              {e.type === "burn" ? "🔥" : e.type === "poison" ? "☠" : e.type === "freeze" ? "❄" : "🏜"}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}
