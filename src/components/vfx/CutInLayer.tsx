"use client";

import { useEffect, useRef, useState } from "react";
import { subscribe } from "@/lib/vfxBus";
import { CARD_ART, CARD_ART_VERSO } from "@/data/cardArt";
import { ELEMENT_STYLE } from "@/lib/vfx";
import type { VfxEvent } from "@/lib/useCombatVfx";

interface CutIn {
  key: number;
  art?: string;
  name: string;
  attackName?: string;
  fromLeft: boolean;
  color: string;
}

const DUR = 1500;

/**
 * Signature cinematic for special / captain attacks: a dark vignette + the
 * attacker's art panel sliding in from its side + the attack name. DOM-based
 * (crisp text, works without WebGL); the Pixi layer adds speedlines/flash.
 * Skippable; collapses to a quick name flash under reduced-motion.
 */
export default function CutInLayer() {
  const [cut, setCut] = useState<CutIn | null>(null);
  const idc = useRef(0);
  const timer = useRef<number | undefined>(undefined);
  const reducedRef = useRef(false);

  useEffect(() => {
    reducedRef.current = !!window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
    const unsub = subscribe((ev: VfxEvent) => {
      if (ev.kind !== "attack" || !ev.big || !ev.attackerName) return;
      const art = ev.defId ? (CARD_ART_VERSO[ev.defId] ?? CARD_ART[ev.defId]) : undefined;
      const fromLeft = (ev.fromX ?? window.innerWidth / 2) < window.innerWidth / 2;
      const color = ELEMENT_STYLE[ev.element]?.color ?? "#E8B84B";
      const key = ++idc.current;
      setCut({ key, art, name: ev.attackerName, attackName: ev.attackName, fromLeft, color });
      window.clearTimeout(timer.current);
      timer.current = window.setTimeout(() => setCut(null), reducedRef.current ? 450 : DUR);
    });
    return () => { unsub(); window.clearTimeout(timer.current); };
  }, []);

  useEffect(() => {
    if (!cut) return;
    const skip = () => { window.clearTimeout(timer.current); setCut(null); };
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape" || e.key === " ") skip(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [cut]);

  if (!cut) return null;
  const dismiss = () => { window.clearTimeout(timer.current); setCut(null); };
  const dur = `${reducedRef.current ? 450 : DUR}ms`;

  return (
    <div key={cut.key} className="cutin-root" onPointerDown={dismiss} style={{ ["--cutin-dur" as string]: dur }}>
      <div className="cutin-vignette" />
      <div className={`cutin-band ${cut.fromLeft ? "cutin-band-l" : "cutin-band-r"}`} style={{ ["--c" as string]: cut.color }}>
        {cut.art && <div className="cutin-art" style={{ backgroundImage: `url('${cut.art}')` }} />}
        <div className="cutin-text">
          {cut.attackName && <div className="cutin-attack" style={{ color: cut.color }}>{cut.attackName}</div>}
          <div className="cutin-name">{cut.name}</div>
        </div>
      </div>
    </div>
  );
}
