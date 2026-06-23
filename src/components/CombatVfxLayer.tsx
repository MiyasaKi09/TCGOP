"use client";

import { useEffect } from "react";
import type { CSSProperties } from "react";
import { ELEMENT_STYLE, HEAL_COLOR } from "@/lib/vfx";
import type { VfxEvent } from "@/lib/useCombatVfx";

const DURATION: Record<VfxEvent["kind"], number> = {
  attack: 560, banner: 1150, impact: 880, heal: 920, ko: 760, spawn: 500,
};

function Node({ ev, onDone, webglActive }: { ev: VfxEvent; onDone: (id: number) => void; webglActive: boolean }) {
  useEffect(() => {
    const t = window.setTimeout(() => onDone(ev.id), DURATION[ev.kind]);
    return () => window.clearTimeout(t);
  }, [ev.id, ev.kind, onDone]);

  const st = ELEMENT_STYLE[ev.element];

  // When WebGL is live, Pixi owns the projectile entirely.
  if (webglActive && ev.kind === "attack") return null;

  // When WebGL is live, Pixi owns the burst; keep only the crisp DOM number.
  if (webglActive && ev.kind === "impact") {
    if (ev.value == null) return null;
    return (
      <div className={`vfx-num vfx-num-dmg ${ev.big ? "vfx-num-big" : ""}`} style={{ left: ev.toX, top: ev.toY, ["--c" as string]: st.color } as CSSProperties}>
        −{ev.value}
      </div>
    );
  }

  // When WebGL is live, Pixi owns the heal sparkle; keep only the DOM number.
  if (webglActive && ev.kind === "heal") {
    if (ev.value == null) return null;
    return (
      <div className="vfx-num vfx-num-heal" style={{ left: ev.toX, top: ev.toY } as CSSProperties}>
        +{ev.value}
      </div>
    );
  }

  // Pixi owns the KO burst entirely.
  if (webglActive && ev.kind === "ko") return null;

  // The cinematic CutInLayer now owns special/captain naming (DOM, always on).
  if (ev.kind === "banner") return null;

  // Deploy slam: Pixi draws dust; the DOM punch is the .vfx-slam token flash.
  if (ev.kind === "spawn") return null;

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

  // (Special/captain banner is now owned by the cinematic CutInLayer.)

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

export default function CombatVfxLayer({ events, remove, webglActive = false }: { events: VfxEvent[]; remove: (id: number) => void; webglActive?: boolean }) {
  return (
    <div className="fixed inset-0 z-30 pointer-events-none overflow-hidden">
      {events.map((ev) => <Node key={ev.id} ev={ev} onDone={remove} webglActive={webglActive} />)}
    </div>
  );
}
