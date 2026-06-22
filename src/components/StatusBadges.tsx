"use client";

import type { StatusEffect } from "@/types";

interface StatusMeta { icon: string; label: string; color: string; }

// Visual language shared across board tokens, captain, menus and the legend.
export const STATUS_META: Record<string, StatusMeta> = {
  burn: { icon: "🔥", label: "Brûlé", color: "#E0653C" },
  poison: { icon: "☠", label: "Empoisonné", color: "#8FB84A" },
  freeze: { icon: "❄", label: "Gelé", color: "#5CC6E0" },
  desiccation: { icon: "🏜", label: "Dessèchement", color: "#C2925A" },
  trap: { icon: "💣", label: "Piégé", color: "#E0463F" },
  immobilize: { icon: "🌸", label: "Immobilisé", color: "#E879B0" },
  sleep: { icon: "💤", label: "Endormi", color: "#9B8CE0" },
  loseAction: { icon: "⏸", label: "Action perdue", color: "#C9A82E" },
  selfKO: { icon: "⌛", label: "Sursis", color: "#E0463F" },
  noStealth: { icon: "👁", label: "Repéré", color: "#7FB0E8" },
  noHeal: { icon: "🚫", label: "Soins bloqués", color: "#FF8A80" },
};

function turnsLabel(n: number): string {
  return n < 0 ? "∞" : String(n);
}

/** Colored status pills (icon + turns left). `compact` is for small board tokens. */
export default function StatusBadges({ effects, compact }: { effects: StatusEffect[]; compact?: boolean }) {
  if (effects.length === 0) return null;
  return (
    <div className={`relative flex flex-wrap ${compact ? "gap-0.5" : "gap-1"}`}>
      {effects.map((e, i) => {
        const m = STATUS_META[e.type] ?? { icon: "•", label: e.type, color: "#aaa" };
        return (
          <span
            key={i}
            title={`${m.label}${e.turnsRemaining >= 0 ? ` · ${e.turnsRemaining} tour(s)` : " · permanent"}`}
            className={`font-oswald font-bold rounded-full flex items-center ${compact ? "text-[9px] px-1 gap-0.5" : "text-[10px] px-1.5 py-0.5 gap-1"}`}
            style={{ background: `${m.color}33`, color: m.color, boxShadow: `inset 0 0 0 1px ${m.color}66` }}
          >
            <span>{m.icon}</span>
            {!compact && <span>{m.label}</span>}
            <span style={{ opacity: 0.85 }}>{turnsLabel(e.turnsRemaining)}</span>
          </span>
        );
      })}
    </div>
  );
}

/** A compact legend mapping status icons → meaning. */
export function StatusLegend({ types }: { types: string[] }) {
  return (
    <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
      {types.map((t) => {
        const m = STATUS_META[t];
        if (!m) return null;
        return (
          <span key={t} className="flex items-center gap-1 font-oswald text-[9px] text-white/55">
            <span style={{ color: m.color }}>{m.icon}</span>{m.label}
          </span>
        );
      })}
    </div>
  );
}
