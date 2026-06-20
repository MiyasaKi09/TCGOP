"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { Hat, Anchor, SkullCross } from "@/components/icons";

type DeckChoice = "mugiwara" | "marines";

export default function Home() {
  const [selected, setSelected] = useState<DeckChoice | null>(null);
  const router = useRouter();

  const startGame = () => {
    if (!selected) return;
    router.push(`/game?deck=${selected}`);
  };

  const crew = (key: DeckChoice) => {
    const on = selected === key;
    const isPirate = key === "mugiwara";
    const accent = isPirate ? "#E8954A" : "#5B97D8";
    const frame = isPirate
      ? "radial-gradient(120% 80% at 50% 6%, #3a2614 0%, #20140b 60%, #140c07 100%)"
      : "radial-gradient(120% 80% at 50% 6%, #163049 0%, #0c1a29 60%, #08121c 100%)";
    return (
      <button
        onClick={() => setSelected(key)}
        className="relative w-64 rounded-2xl p-6 text-left overflow-hidden transition-all duration-200 hover:-translate-y-1"
        style={{ background: frame, boxShadow: on ? `inset 0 0 0 2px ${accent}, 0 0 28px 2px ${accent}55` : "inset 0 0 0 1px rgba(255,255,255,.08), 0 10px 26px rgba(0,0,0,.5)" }}
      >
        <div className="absolute inset-0 flex items-center justify-center opacity-[0.10] pointer-events-none">
          {isPirate ? <Hat size={150} color="#fff" /> : <Anchor size={150} color="#fff" />}
        </div>
        <div className="relative flex items-center gap-2 mb-3">
          {isPirate ? <Hat size={22} color={accent} /> : <Anchor size={22} color={accent} />}
          <h2 className="font-cinzel text-2xl font-bold" style={{ color: accent }}>{isPirate ? "Mugiwara" : "Marine"}</h2>
        </div>
        <p className="relative font-oswald text-sm text-white/75">Capitaine : {isPirate ? "Monkey D. Luffy" : "Akainu (Sakazuki)"}</p>
        <p className="relative font-spectral italic text-xs text-white/45 mt-1">{isPirate ? "Diversité, synergies, sustain" : "Hiérarchie, Logia, contrôle"}</p>
        {on && <div className="absolute top-3 right-3 font-oswald text-[9px] uppercase tracking-widest px-2 py-0.5 rounded-full" style={{ background: accent, color: "#0a0d12" }}>Choisi</div>}
      </button>
    );
  };

  return (
    <main className="sea-bg min-h-screen flex flex-col items-center justify-center gap-8 p-8">
      <div className="flex flex-col items-center gap-2">
        <div className="flex items-center gap-3">
          <SkullCross size={26} color="#E8B84B" />
          <h1 className="font-cinzel text-4xl sm:text-5xl font-extrabold tracking-wide text-center" style={{ color: "#E8B84B", textShadow: "0 4px 24px rgba(0,0,0,.6)" }}>
            One Piece Grand Line TCG
          </h1>
        </div>
        <p className="font-oswald uppercase tracking-[.2em] text-xs text-white/45">Choisis ton équipage</p>
      </div>

      <div className="flex flex-wrap gap-6 justify-center">
        {crew("mugiwara")}
        {crew("marines")}
      </div>

      <button
        onClick={startGame}
        disabled={!selected}
        className={`action-btn font-oswald font-bold uppercase tracking-wider px-10 py-3.5 rounded-xl text-lg transition-all ${
          selected ? "gold-surface shadow-xl cursor-pointer" : "bg-white/5 text-white/30 cursor-not-allowed"
        }`}
      >
        Commencer le combat
      </button>
    </main>
  );
}
