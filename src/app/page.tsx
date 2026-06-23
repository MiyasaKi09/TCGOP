"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { Hat, Anchor, SkullCross } from "@/components/icons";
import { DECK_VISUAL } from "@/lib/theme";

type DeckChoice = "mugiwara" | "marines" | "baroque" | "redhair";
type Level = "beginner" | "intermediate" | "expert";

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
    const m = DECK_VISUAL[key];
    const Crest = CREST[m.crest];
    return (
      <button
        key={key}
        onClick={() => onPick(key)}
        className="relative w-52 rounded-2xl overflow-hidden text-left transition-all duration-200 hover:-translate-y-1"
        style={{ background: m.frame, border: "2px solid var(--ink-edge)", boxShadow: on ? `0 0 0 2px ${m.accent}, 0 12px 28px rgba(0,0,0,.55)` : "0 10px 26px rgba(0,0,0,.5)" }}
      >
        {/* faction ribbon */}
        <div className="relative flex items-center gap-2 px-3 py-2" style={{ background: m.accent, color: "#0a0d12", boxShadow: "inset 0 -2px 0 rgba(0,0,0,.25)" }}>
          <Crest size={18} color="#0a0d12" />
          <span className="font-cinzel text-[15px] font-extrabold tracking-wide truncate">{m.label}</span>
          {on && <span className="ml-auto font-oswald text-[9px] uppercase tracking-widest px-2 py-0.5 rounded-full" style={{ background: "rgba(0,0,0,.28)", color: "#fff" }}>Choisi</span>}
        </div>
        {/* body */}
        <div className="relative p-4">
          <div className="halftone absolute inset-0 opacity-30 pointer-events-none" />
          <div className="absolute inset-0 flex items-center justify-center opacity-[0.08] pointer-events-none">
            <Crest size={120} color="#fff" />
          </div>
          <p className="relative font-oswald text-xs text-white/80">Capitaine : {m.captain}</p>
          <p className="relative font-spectral italic text-[11px] text-white/50 mt-1">{m.tagline}</p>
        </div>
      </button>
    );
  };

  return (
    <main className="sea-bg relative min-h-screen overflow-hidden">
      <div className="manga-atmos" />
      <div className="relative z-10 min-h-screen flex flex-col items-center gap-7 p-8 py-12">
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
          {(Object.keys(DECK_VISUAL) as DeckChoice[]).map((k) => crew(k, playerDeck, setPlayerDeck))}
        </div>
      </section>

      <section className="flex flex-col items-center gap-3">
        <p className="font-oswald uppercase tracking-[.18em] text-[11px] text-red-300/80">Adversaire (IA)</p>
        <div className="flex flex-wrap gap-4 justify-center max-w-4xl">
          {(Object.keys(DECK_VISUAL) as DeckChoice[]).map((k) => crew(k, aiDeck, setAiDeck))}
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
      </div>
    </main>
  );
}
