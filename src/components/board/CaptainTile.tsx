"use client";

import type { CaptainInstance, CaptainDef } from "@/types";
import { faction } from "@/data/cardArt";
import { Crest } from "../icons";

// Compact, iconic captain card from the maquette: faction-tinted dark card with
// a gold pentagon HP badge, a gold star, a faded faction emblem, and a
// red-ATK · NAME · blue-DEF footer. Full passive/verso details live in CaptainMenu.
export default function CaptainTile({ captain, def, highlight }: {
  captain: CaptainInstance; def: CaptainDef; highlight?: boolean;
}) {
  const fac = faction(def.faction);
  const side = captain.flipped ? def.verso : def.recto;
  const isMarine = def.faction === "marine";
  const tint = isMarine
    ? "radial-gradient(120% 90% at 50% 28%, #17293d 0%, #0c1827 62%, #08111c 100%)"
    : "radial-gradient(120% 90% at 50% 28%, #2b2118 0%, #19130c 62%, #110b06 100%)";
  const border = captain.flipped ? "#E0463F" : highlight ? "#E8B84B" : "rgba(255,255,255,.16)";

  return (
    <div
      className="relative w-36 rounded-2xl overflow-hidden"
      style={{ aspectRatio: "5 / 6", background: tint, boxShadow: `inset 0 0 0 2px ${border}, 0 10px 26px rgba(0,0,0,.55)` }}
    >
      {/* faded faction emblem */}
      <div className="absolute inset-0 flex items-center justify-center opacity-[0.13] pointer-events-none">
        <Crest which={fac.crest} size={96} color="#fff" />
      </div>

      {/* HP pentagon, top-left */}
      <div className="absolute top-2 left-2 flex items-center justify-center"
        style={{ width: 44, height: 42, background: "linear-gradient(160deg,#F6D272,#D9A434)", clipPath: "polygon(50% 0%,100% 38%,82% 100%,18% 100%,0% 38%)", boxShadow: "0 2px 6px rgba(0,0,0,.5)" }}>
        <span className="font-cinzel font-bold text-[16px]" style={{ color: "#1a1206", marginTop: -3 }}>{captain.currentPv}</span>
      </div>

      {/* star, top-right */}
      <div className="absolute top-2 right-2 w-9 h-9 rounded-full flex items-center justify-center"
        style={{ background: "linear-gradient(160deg,#F6D272,#D9A434)", boxShadow: "0 2px 6px rgba(0,0,0,.5)" }}>
        <span className="text-[16px] leading-none" style={{ color: "#fff8e6" }}>✦</span>
      </div>

      {/* footer — ATK · NAME · DEF */}
      <div className="absolute left-0 right-0 bottom-0 flex items-center justify-between gap-1 px-2 pb-2 pt-7"
        style={{ background: "linear-gradient(180deg,transparent,rgba(5,7,11,.86) 55%)" }}>
        <span className="w-7 h-7 rounded-full flex items-center justify-center font-oswald font-bold text-[13px] text-white shrink-0"
          style={{ background: "#E0463F", boxShadow: "0 1px 4px rgba(0,0,0,.5)" }}>{side.atk}</span>
        <span className="font-cinzel font-bold text-[12px] text-white text-center leading-none truncate" style={{ textShadow: "0 1px 3px #000" }}>{def.name}</span>
        <span className="w-7 h-7 rounded-full flex items-center justify-center font-oswald font-bold text-[13px] text-white shrink-0"
          style={{ background: "#3F86C9", boxShadow: "0 1px 4px rgba(0,0,0,.5)" }}>{side.def}</span>
      </div>

      {captain.tapped && (
        <div className="absolute inset-0 flex items-center justify-center" style={{ background: "rgba(5,7,11,.5)" }}>
          <span className="font-oswald text-[8px] tracking-widest uppercase text-white px-2 py-0.5 rounded-full" style={{ background: "rgba(0,0,0,.6)" }}>Engagé</span>
        </div>
      )}
    </div>
  );
}
