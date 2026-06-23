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
      <div className="panel halftone p-4 flex flex-col gap-3 animate-fade-in" style={{ boxShadow: "inset 0 0 0 1.5px rgba(91,198,224,.45), var(--shadow-modal)", userSelect: "none" }} onClick={(e) => e.stopPropagation()}>
        <div className="flex items-center justify-between">
          <span className="font-oswald text-[10px] uppercase tracking-widest text-cyan-200/70">⚓ Navire</span>
          {active && <span className="font-oswald font-bold text-sm text-cyan-200">{active.cost} Vol.</span>}
        </div>

        <div ref={zoomRef} style={{ willChange: "transform" }}><FullCard def={def} instance={instance} state={state} width={300} /></div>

        {isYou && active && (
          <button onClick={onActivate} disabled={!canActivate}
            className="btn action-btn px-3 py-2.5 text-sm"
            style={{ background: "linear-gradient(180deg,#1f93b3,#147893)", color: "#fff", boxShadow: "0 4px 0 #0c4a5c, 0 8px 18px rgba(0,0,0,.4)" }}>
            ⚓ Activer — {active.name}{!canActivate && activateReason ? ` (${activateReason})` : ""}
          </button>
        )}

        <button onClick={onClose} className="btn btn-ghost action-btn px-3 py-1.5 text-xs">Fermer</button>
      </div>
    </div>
  );
}
