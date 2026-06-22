"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { Hat, Anchor, SkullCross } from "@/components/icons";

type DeckChoice = "mugiwara" | "marines" | "baroque" | "redhair";

interface DeckMeta {
  label: string;
  captain: string;
  tagline: string;
  accent: string;
  frame: string;
  crest: "hat" | "anchor";
}

const DECKS: Record<DeckChoice, DeckMeta> = {
  mugiwara: {
    label: "Mugiwara", captain: "Monkey D. Luffy", tagline: "Diversité, synergies, sustain",
    accent: "#E8954A", crest: "hat",
    frame: "radial-gradient(120% 80% at 50% 6%, #3a2614 0%, #20140b 60%, #140c07 100%)",
  },
  marines: {
    label: "Marine", captain: "Akainu (Sakazuki)", tagline: "Anti-Fruit, Logia, contrôle",
    accent: "#5B97D8", crest: "anchor",
    frame: "radial-gradient(120% 80% at 50% 6%, #163049 0%, #0c1a29 60%, #08121c 100%)",
  },
  baroque: {
    label: "Baroque Works", captain: "Crocodile (Mr. 0)", tagline: "Agro/tempo, poison & sable",
    accent: "#C9A24B", crest: "hat",
    frame: "radial-gradient(120% 80% at 50% 6%, #2c2618 0%, #1a160d 60%, #100d07 100%)",
  },
  redhair: {
    label: "Red Hair", captain: "Shanks (Akagami)", tagline: "Puissance, Haki, intimidation",
    accent: "#D2473C", crest: "hat",
    frame: "radial-gradient(120% 80% at 50% 6%, #3a1614 0%, #200c0b 60%, #140707 100%)",
  },
};

export default function Home() {
  const [selected, setSelected] = useState<DeckChoice | null>(null);
  const router = useRouter();

  const startGame = () => {
    if (!selected) return;
    router.push(`/game?deck=${selected}`);
  };

  const crew = (key: DeckChoice) => {
    const on = selected === key;
    const m = DECKS[key];
    const Crest = m.crest === "hat" ? Hat : Anchor;
    return (
      <button
        key={key}
        onClick={() => setSelected(key)}
        className="relative w-60 rounded-2xl p-6 text-left overflow-hidden transition-all duration-200 hover:-translate-y-1"
        style={{ background: m.frame, boxShadow: on ? `inset 0 0 0 2px ${m.accent}, 0 0 28px 2px ${m.accent}55` : "inset 0 0 0 1px rgba(255,255,255,.08), 0 10px 26px rgba(0,0,0,.5)" }}
      >
        <div className="absolute inset-0 flex items-center justify-center opacity-[0.10] pointer-events-none">
          <Crest size={150} color="#fff" />
        </div>
        <div className="relative flex items-center gap-2 mb-3">
          <Crest size={22} color={m.accent} />
          <h2 className="font-cinzel text-xl font-bold" style={{ color: m.accent }}>{m.label}</h2>
        </div>
        <p className="relative font-oswald text-sm text-white/75">Capitaine : {m.captain}</p>
        <p className="relative font-spectral italic text-xs text-white/45 mt-1">{m.tagline}</p>
        {on && <div className="absolute top-3 right-3 font-oswald text-[9px] uppercase tracking-widest px-2 py-0.5 rounded-full" style={{ background: m.accent, color: "#0a0d12" }}>Choisi</div>}
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

      <div className="flex flex-wrap gap-6 justify-center max-w-4xl">
        {(Object.keys(DECKS) as DeckChoice[]).map((k) => crew(k))}
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
