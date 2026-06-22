"use client";

import { useEffect } from "react";
import type { CSSProperties } from "react";
import { ELEMENT_STYLE, HEAL_COLOR } from "@/lib/vfx";
import type { VfxEvent } from "@/lib/useCombatVfx";

const DURATION: Record<VfxEvent["kind"], number> = {
  attack: 560, banner: 1150, impact: 880, heal: 920, ko: 760,
};

function Node({ ev, onDone }: { ev: VfxEvent; onDone: (id: number) => void }) {
  useEffect(() => {
    const t = window.setTimeout(() => onDone(ev.id), DURATION[ev.kind]);
    return () => window.clearTimeout(t);
  }, [ev.id, ev.kind, onDone]);

  const st = ELEMENT_STYLE[ev.element];

  // --- Projectile from attacker → target (+ optional screen tint on big hits) ---
  if (ev.kind === "attack") {
    const dx = (ev.toX ?? 0) - (ev.fromX ?? 0);
    const dy = (ev.toY ?? 0) - (ev.fromY ?? 0);
    const projStyle = {
      left: ev.fromX, top: ev.fromY,
      ["--dx" as string]: `${dx}px`, ["--dy" as string]: `${dy}px`,
      ["--c" as string]: st.color, ["--g" as string]: st.glow,
      ["--dur" as string]: `${DURATION.attack}ms`,
    } as CSSProperties;
    return (
      <>
        {ev.big && <div className="vfx-tint" style={{ ["--tint" as string]: st.tint } as CSSProperties} />}
        <div className={`vfx-proj ${st.proj} ${ev.big ? "vfx-proj-big" : ""}`} style={projStyle}>
          <span className="vfx-glyph">{st.glyph}</span>
        </div>
      </>
    );
  }

  // --- Special / captain banner ---
  if (ev.kind === "banner") {
    return (
      <div className={`vfx-banner ${ev.big ? "vfx-banner-big" : ""}`} style={{ ["--c" as string]: st.color, ["--g" as string]: st.glow } as CSSProperties}>
        <span className="vfx-banner-name">{ev.label}</span>
        {ev.sub && <span className="vfx-banner-sub">{ev.sub}</span>}
      </div>
    );
  }

  // --- Impact burst + damage number ---
  if (ev.kind === "impact") {
    const pos = { left: ev.toX, top: ev.toY, ["--c" as string]: st.color, ["--g" as string]: st.glow } as CSSProperties;
    return (
      <>
        {ev.big && <div className="vfx-tint vfx-tint-soft" style={{ ["--tint" as string]: st.tint } as CSSProperties} />}
        <div className={`vfx-burst ${st.burst} ${ev.big ? "vfx-burst-big" : ""}`} style={pos}>
          <span className="vfx-glyph">{st.glyph}</span>
        </div>
        {ev.value != null && (
          <div className={`vfx-num vfx-num-dmg ${ev.big ? "vfx-num-big" : ""}`} style={{ left: ev.toX, top: ev.toY, ["--c" as string]: st.color } as CSSProperties}>
            −{ev.value}
          </div>
        )}
      </>
    );
  }

  // --- Heal sparkle + number ---
  if (ev.kind === "heal") {
    return (
      <>
        <div className="vfx-burst vfx-burst-heal" style={{ left: ev.toX, top: ev.toY, ["--c" as string]: HEAL_COLOR, ["--g" as string]: "rgba(91,196,106,.7)" } as CSSProperties}>
          <span className="vfx-glyph">✚</span>
        </div>
        {ev.value != null && (
          <div className="vfx-num vfx-num-heal" style={{ left: ev.toX, top: ev.toY } as CSSProperties}>
            +{ev.value}
          </div>
        )}
      </>
    );
  }

  // --- KO shatter ---
  if (ev.kind === "ko") {
    return <div className="vfx-ko-burst" style={{ left: ev.toX, top: ev.toY } as CSSProperties}>✖</div>;
  }

  return null;
}

export default function CombatVfxLayer({ events, remove }: { events: VfxEvent[]; remove: (id: number) => void }) {
  return (
    <div className="fixed inset-0 z-30 pointer-events-none overflow-hidden">
      {events.map((ev) => <Node key={ev.id} ev={ev} onDone={remove} />)}
    </div>
  );
}
