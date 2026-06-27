"use client";

// Volonté (energy) shown as a row of gem pips, n filled out of max — matches
// the maquette's "VOLONTÉ · 6/10" HUD.
export default function VolonteGauge({
  value, max = 10, label, align = "left",
}: { value: number; max?: number; label: string; align?: "left" | "right" }) {
  const filled = Math.max(0, Math.min(max, value));
  return (
    <div className={`flex flex-col gap-1 ${align === "right" ? "items-end" : "items-start"}`}>
      <span className="font-oswald text-[9px] uppercase tracking-[.18em] text-white/55">
        {label} · <span className="text-white/80 font-bold">{filled}/{max}</span>
      </span>
      <div className="flex gap-[3px]">
        {Array.from({ length: max }).map((_, i) => (
          <span
            key={i}
            className="rounded-full"
            style={{
              width: 9, height: 9,
              background: i < filled ? "radial-gradient(circle at 32% 28%, #cfeaff, #3f86c9 70%)" : "rgba(255,255,255,.10)",
              boxShadow: i < filled
                ? "0 0 6px rgba(96,156,224,.85), inset 0 0 0 1px rgba(255,255,255,.45)"
                : "inset 0 0 0 1px rgba(255,255,255,.16)",
            }}
          />
        ))}
      </div>
    </div>
  );
}
