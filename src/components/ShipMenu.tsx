"use client";

import type { CardDef, CardInstance, GameState } from "@/types";
import FullCard from "./FullCard";
import { useFlipZoom } from "@/lib/useFlipZoom";

interface ShipMenuProps {
  instance: CardInstance;
  def: CardDef;
  state: GameState;
  isYou: boolean;
  canActivate: boolean;
  activateReason?: string | null;
  onActivate: () => void;
  onClose: () => void;
  originRect?: DOMRect | null;
}

export default function ShipMenu({
  instance, def, state, isYou, canActivate, activateReason, onActivate, onClose, originRect,
}: ShipMenuProps) {
  const zoomRef = useFlipZoom<HTMLDivElement>(originRect);
  const active = def.shipActive;

  return (
    <div className="fixed inset-0 bg-black/75 backdrop-blur-sm flex items-center justify-center z-40 p-4" onClick={onClose} onContextMenu={(e) => e.preventDefault()}>
      <div className="rounded-2xl p-4 flex flex-col gap-3 animate-fade-in" style={{ background: "rgba(10,14,20,.94)", boxShadow: "inset 0 0 0 1px rgba(91,198,224,.35), 0 24px 70px rgba(0,0,0,.6)", userSelect: "none" }} onClick={(e) => e.stopPropagation()}>
        <div className="flex items-center justify-between">
          <span className="font-oswald text-[10px] uppercase tracking-widest text-cyan-200/70">⚓ Navire</span>
          {active && <span className="font-oswald font-bold text-sm text-cyan-200">{active.cost} Vol.</span>}
        </div>

        <div ref={zoomRef} style={{ willChange: "transform" }}><FullCard def={def} instance={instance} state={state} width={300} /></div>

        {isYou && active && (
          <button onClick={onActivate} disabled={!canActivate}
            className="action-btn font-oswald px-3 py-2.5 rounded-xl text-sm font-bold transition-all disabled:opacity-40 disabled:cursor-not-allowed"
            style={{ background: "rgba(20,120,150,.75)" }}>
            ⚓ Activer — {active.name}{!canActivate && activateReason ? ` (${activateReason})` : ""}
          </button>
        )}

        <button onClick={onClose} className="font-oswald px-3 py-1.5 rounded-lg text-xs text-white/55 transition-all" style={{ background: "rgba(255,255,255,.06)" }}>Fermer</button>
      </div>
    </div>
  );
}
