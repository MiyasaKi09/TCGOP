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
    onPick: (k: DeckChoice) => void,
    idx: number
  ) => {
    const on = selected === key;
    const m = DECK_VISUAL[key];
    const Crest = CREST[m.crest];
    return (
      <button
        key={key}
        onClick={() => onPick(key)}
        className="surface lift sheen enter relative w-56 rounded-2xl overflow-hidden text-left"
        style={{ ["--d" as string]: `${120 + idx * 70}ms`, boxShadow: on ? `0 0 0 2px ${m.accent}, 0 16px 36px rgba(0,0,0,.55)` : undefined }}
      >
        {/* thin faction accent strip — minimalist colour cue */}
        <div className="h-1.5 w-full" style={{ background: m.accent }} />
        <div className="relative p-4">
          <div className="absolute -right-2 -top-1 opacity-[0.07] pointer-events-none">
            <Crest size={84} color="#fff" />
          </div>
          <div className="flex items-center gap-2">
            <Crest size={18} color={m.accent} />
            <h2 className="font-cinzel text-lg font-bold truncate" style={{ color: m.accent }}>{m.label}</h2>
            {on && (
              <span className="ml-auto flex items-center justify-center w-5 h-5 rounded-full text-[11px] font-bold" style={{ background: m.accent, color: "#0a0d12" }}>✓</span>
            )}
          </div>
          <p className="font-oswald text-xs text-white/80 mt-2">Capitaine · {m.captain}</p>
          <p className="font-spectral italic text-[11px] text-white/50 mt-0.5">{m.tagline}</p>
        </div>
      </button>
    );
  };

  return (
    <main className="sea-bg relative min-h-screen overflow-hidden">
      <div className="manga-atmos" />
      <div className="relative z-10 min-h-screen flex flex-col items-center gap-7 p-8 py-12">
      <div className="speed-lines enter flex flex-col items-center gap-2 px-10 py-4">
        <div className="flex items-center gap-3">
          <SkullCross size={26} color="var(--color-gold)" />
          <h1 className="font-cinzel text-3xl sm:text-4xl font-extrabold tracking-wide text-center text-gold" style={{ textShadow: "0 2px 0 var(--ink-edge), 0 4px 24px rgba(0,0,0,.7)" }}>
            One Piece Grand Line TCG
          </h1>
        </div>
        <p className="font-oswald uppercase tracking-[.2em] text-xs text-white/50">Prépare le duel</p>
      </div>

      <section className="enter flex flex-col items-center gap-3" style={{ ["--d" as string]: "80ms" }}>
        <p className="font-oswald uppercase tracking-[.18em] text-[11px] text-emerald-300/80">Votre équipage</p>
        <div className="flex flex-wrap gap-4 justify-center max-w-4xl">
          {(Object.keys(DECK_VISUAL) as DeckChoice[]).map((k, i) => crew(k, playerDeck, setPlayerDeck, i))}
        </div>
      </section>

      <section className="enter flex flex-col items-center gap-3" style={{ ["--d" as string]: "160ms" }}>
        <p className="font-oswald uppercase tracking-[.18em] text-[11px] text-red-300/80">Adversaire (IA)</p>
        <div className="flex flex-wrap gap-4 justify-center max-w-4xl">
          {(Object.keys(DECK_VISUAL) as DeckChoice[]).map((k, i) => crew(k, aiDeck, setAiDeck, i))}
        </div>
      </section>

      <section className="enter flex flex-col items-center gap-3" style={{ ["--d" as string]: "240ms" }}>
        <p className="font-oswald uppercase tracking-[.18em] text-[11px] text-white/55">Niveau de l'IA</p>
        <div className="flex flex-wrap gap-3 justify-center">
          {LEVELS.map((l) => {
            const on = level === l.key;
            return (
              <button
                key={l.key}
                onClick={() => setLevel(l.key)}
                className="lift w-44 rounded-xl px-4 py-3 text-left"
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
        className={`btn action-btn enter text-lg px-10 py-3.5 mt-1 ${ready ? "btn-gold" : "btn-ghost"}`}
        style={{ ["--d" as string]: "320ms" }}
      >
        Commencer le combat
      </button>
      </div>
    </main>
  );
}
