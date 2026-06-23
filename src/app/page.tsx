"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { Hat, Anchor, SkullCross } from "@/components/icons";

type DeckChoice = "mugiwara" | "marines" | "baroque" | "redhair";
type Level = "beginner" | "intermediate" | "expert";

interface DeckMeta {
  label: string;
  captain: string;
  tagline: string;
  accent: string;
  frame: string;
  crest: "hat" | "anchor" | "skull";
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
    accent: "#C9A24B", crest: "skull",
    frame: "radial-gradient(120% 80% at 50% 6%, #2c2618 0%, #1a160d 60%, #100d07 100%)",
  },
  redhair: {
    label: "Red Hair", captain: "Shanks (Akagami)", tagline: "Puissance, Haki, intimidation",
    accent: "#D2473C", crest: "skull",
    frame: "radial-gradient(120% 80% at 50% 6%, #3a1614 0%, #200c0b 60%, #140707 100%)",
  },
};

const CREST = { hat: Hat, anchor: Anchor, skull: SkullCross } as const;

const LEVELS: { key: Level; label: string; sub: string }[] = [
  { key: "beginner", label: "Débutant", sub: "Joue prudemment, encaisse souvent" },
  { key: "intermediate", label: "Intermédiaire", sub: "Heuristique solide, gère ses tempos" },
  { key: "expert", label: "Expert", sub: "Anticipe, contre et cherche le létal" },
];

export default function Home() {
  const [playerDeck, setPlayerDeck] = useState<DeckChoice | null>(null);
  const [aiDeck, setAiDeck] = useState<DeckChoice | null>(null);
  const [level, setLevel] = useState<Level>("intermediate");
  const router = useRouter();

  const ready = playerDeck && aiDeck;

  const startGame = () => {
    if (!ready) return;
    router.push(`/game?deck=${playerDeck}&foe=${aiDeck}&ai=${level}`);
  };

  const crew = (
    key: DeckChoice,
    selected: DeckChoice | null,
    onPick: (k: DeckChoice) => void
  ) => {
    const on = selected === key;
    const m = DECKS[key];
    const Crest = CREST[m.crest];
    return (
      <button
        key={key}
        onClick={() => onPick(key)}
        className="panel-clip relative w-52 p-5 text-left overflow-hidden transition-all duration-200 hover:-translate-y-1"
        style={{ background: m.frame, border: "2px solid var(--ink-edge)", boxShadow: on ? `inset 0 0 0 2px ${m.accent}, 0 0 28px 2px ${m.accent}55` : "inset 0 0 0 1px rgba(255,255,255,.08), 0 10px 26px rgba(0,0,0,.5)" }}
      >
        <div className="halftone absolute inset-0 opacity-40 pointer-events-none" />
        <div className="absolute inset-0 flex items-center justify-center opacity-[0.10] pointer-events-none">
          <Crest size={130} color="#fff" />
        </div>
        <div className="relative flex items-center gap-2 mb-2.5">
          <Crest size={20} color={m.accent} />
          <h2 className="font-cinzel text-lg font-bold" style={{ color: m.accent }}>{m.label}</h2>
        </div>
        <p className="relative font-oswald text-xs text-white/75">Capitaine : {m.captain}</p>
        <p className="relative font-spectral italic text-[11px] text-white/45 mt-1">{m.tagline}</p>
        {on && <div className="absolute top-2.5 right-2.5 font-oswald text-[9px] uppercase tracking-widest px-2 py-0.5 rounded-full" style={{ background: m.accent, color: "#0a0d12" }}>Choisi</div>}
      </button>
    );
  };

  return (
    <main className="sea-bg min-h-screen flex flex-col items-center gap-7 p-8 py-12">
      <div className="speed-lines flex flex-col items-center gap-2 px-10 py-4">
        <div className="flex items-center gap-3">
          <SkullCross size={26} color="var(--color-gold)" />
          <h1 className="font-cinzel text-3xl sm:text-4xl font-extrabold tracking-wide text-center text-gold" style={{ textShadow: "0 2px 0 var(--ink-edge), 0 4px 24px rgba(0,0,0,.7)" }}>
            One Piece Grand Line TCG
          </h1>
        </div>
        <p className="font-oswald uppercase tracking-[.2em] text-xs text-white/50">Prépare le duel</p>
      </div>

      <section className="flex flex-col items-center gap-3">
        <p className="font-oswald uppercase tracking-[.18em] text-[11px] text-emerald-300/80">Votre équipage</p>
        <div className="flex flex-wrap gap-4 justify-center max-w-4xl">
          {(Object.keys(DECKS) as DeckChoice[]).map((k) => crew(k, playerDeck, setPlayerDeck))}
        </div>
      </section>

      <section className="flex flex-col items-center gap-3">
        <p className="font-oswald uppercase tracking-[.18em] text-[11px] text-red-300/80">Adversaire (IA)</p>
        <div className="flex flex-wrap gap-4 justify-center max-w-4xl">
          {(Object.keys(DECKS) as DeckChoice[]).map((k) => crew(k, aiDeck, setAiDeck))}
        </div>
      </section>

      <section className="flex flex-col items-center gap-3">
        <p className="font-oswald uppercase tracking-[.18em] text-[11px] text-white/55">Niveau de l'IA</p>
        <div className="flex flex-wrap gap-3 justify-center">
          {LEVELS.map((l) => {
            const on = level === l.key;
            return (
              <button
                key={l.key}
                onClick={() => setLevel(l.key)}
                className="w-44 rounded-xl px-4 py-3 text-left transition-all duration-200"
                style={{ background: on ? "linear-gradient(180deg,#2a2f3a,#1a1e26)" : "rgba(255,255,255,.04)", border: "2px solid var(--ink-edge)", boxShadow: on ? "inset 0 0 0 2px var(--color-gold), 0 0 22px 1px #E8B84B33" : "inset 0 0 0 1px rgba(255,255,255,.07)" }}
              >
                <div className="font-oswald font-bold text-sm" style={{ color: on ? "var(--color-gold)" : "rgba(255,255,255,.8)" }}>{l.label}</div>
                <div className="font-spectral italic text-[11px] text-white/45 mt-0.5">{l.sub}</div>
              </button>
            );
          })}
        </div>
      </section>

      <button
        onClick={startGame}
        disabled={!ready}
        className={`btn action-btn text-lg px-10 py-3.5 mt-1 ${ready ? "btn-gold" : "btn-ghost"}`}
      >
        Commencer le combat
      </button>
    </main>
  );
}
