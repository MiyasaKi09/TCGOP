"use client";

import { useEffect, useRef } from "react";
import { getCardDef } from "@/engine/cardRegistry";
import FullCard from "./FullCard";
import { announceDuration, type PlayAnnouncement } from "@/lib/announce";

const SIDE = {
  you: { color: "#5BC46A", label: "Vous" },
  foe: { color: "#E0463F", label: "◆ Adversaire" },
};

function Reveal({ ann, onDone }: { ann: PlayAnnouncement; onDone: (id: number) => void }) {
  const anchor = useRef<HTMLDivElement>(null);
  const dur = announceDuration(ann);

  useEffect(() => {
    const el = anchor.current;
    const holdMs = ann.toast ? dur * 0.72 : dur * 0.5;
    const t1 = window.setTimeout(() => {
      if (!el) return;
      if (ann.destId && !ann.toast) {
        const dest = document.querySelector(`[data-inst="${ann.destId}"]`)?.getBoundingClientRect();
        if (dest) {
          const dx = dest.left + dest.width / 2 - window.innerWidth / 2;
          const dy = dest.top + dest.height / 2 - window.innerHeight / 2;
          el.style.transition = "transform .5s cubic-bezier(.4,.1,.3,1), opacity .5s ease-in";
          el.style.transform = `translate(-50%,-50%) translate(${dx}px,${dy}px) scale(.16)`;
          el.style.opacity = "0";
          return;
        }
      }
      el.style.transition = "opacity .4s ease-in";
      el.style.opacity = "0";
    }, holdMs);
    const t2 = window.setTimeout(() => onDone(ann.id), dur);
    return () => { window.clearTimeout(t1); window.clearTimeout(t2); };
  }, [ann, dur, onDone]);

  const side = SIDE[ann.side];

  // text-only toast
  if (ann.toast || !ann.defId) {
    return (
      <div ref={anchor} className="vfx-reveal-anchor">
        <div className="vfx-reveal-pop vfx-reveal-toast" style={{ ["--c" as string]: side.color }} onClick={() => onDone(ann.id)}>
          <span className="vfx-reveal-side" style={{ color: side.color }}>{side.label}</span>
          <span className="vfx-reveal-caption">{ann.caption}</span>
        </div>
      </div>
    );
  }

  let def;
  try { def = getCardDef(ann.defId); } catch { return null; }

  return (
    <div ref={anchor} className="vfx-reveal-anchor">
      <div className="vfx-reveal-pop" style={{ pointerEvents: "auto", cursor: "pointer" }} onClick={() => onDone(ann.id)}>
        <div className="vfx-reveal-side" style={{ color: side.color }}>{side.label}</div>
        <div className="vfx-reveal-frame" style={{ boxShadow: `0 0 0 2px ${side.color}, 0 0 40px 6px ${side.color}66` }}>
          <FullCard def={def} width={ann.big ? 300 : 184} />
        </div>
        <div className="vfx-reveal-caption">{ann.caption}</div>
      </div>
    </div>
  );
}

export default function PlayRevealLayer({ announcements, dismiss }: { announcements: PlayAnnouncement[]; dismiss: (id: number) => void }) {
  const head = announcements[0];
  if (!head) return null;
  return (
    <div className="fixed inset-0 z-40 pointer-events-none">
      <Reveal key={head.id} ann={head} onDone={dismiss} />
    </div>
  );
}
