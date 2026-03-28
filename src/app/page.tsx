"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";

type DeckChoice = "mugiwara" | "marines";

export default function Home() {
  const [selected, setSelected] = useState<DeckChoice | null>(null);
  const router = useRouter();

  const startGame = () => {
    if (!selected) return;
    router.push(`/game?deck=${selected}`);
  };

  return (
    <main className="flex flex-col items-center justify-center min-h-screen gap-8 p-8">
      <h1 className="text-4xl font-bold text-center">
        One Piece Grand Line TCG
      </h1>
      <p className="text-gray-400 text-lg">Choisis ton equipage</p>

      <div className="flex gap-6">
        <button
          onClick={() => setSelected("mugiwara")}
          className={`p-6 rounded-xl border-2 transition-all w-64 ${
            selected === "mugiwara"
              ? "border-red-500 bg-red-500/10"
              : "border-gray-700 hover:border-gray-500"
          }`}
        >
          <h2 className="text-xl font-bold text-red-400">Mugiwara</h2>
          <p className="text-sm text-gray-400 mt-2">
            Capitaine : Monkey D. Luffy
          </p>
          <p className="text-xs text-gray-500 mt-1">
            Diversite, synergies, sustain
          </p>
        </button>

        <button
          onClick={() => setSelected("marines")}
          className={`p-6 rounded-xl border-2 transition-all w-64 ${
            selected === "marines"
              ? "border-blue-500 bg-blue-500/10"
              : "border-gray-700 hover:border-gray-500"
          }`}
        >
          <h2 className="text-xl font-bold text-blue-400">Marines</h2>
          <p className="text-sm text-gray-400 mt-2">
            Capitaine : Akainu (Sakazuki)
          </p>
          <p className="text-xs text-gray-500 mt-1">
            Hierarchie, Logia, controle
          </p>
        </button>
      </div>

      <button
        onClick={startGame}
        disabled={!selected}
        className={`px-8 py-3 rounded-lg text-lg font-semibold transition-all ${
          selected
            ? "bg-amber-600 hover:bg-amber-500 text-white cursor-pointer"
            : "bg-gray-800 text-gray-600 cursor-not-allowed"
        }`}
      >
        Commencer le combat
      </button>
    </main>
  );
}
