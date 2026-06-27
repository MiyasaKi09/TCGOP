"use client";

// A small framed card-pile (Deck / Pioche / Défausse) with a count.
export default function Pile({ label, count }: { label: string; count: number }) {
  return (
    <div className="flex flex-col items-center gap-0.5">
      <div
        className="relative w-9 h-12 rounded-md"
        style={{ background: "linear-gradient(160deg,#1b2330,#0b1018)", boxShadow: "inset 0 0 0 1px rgba(232,184,75,.32), 0 3px 9px rgba(0,0,0,.5)" }}
      >
        <span className="absolute inset-0 flex items-center justify-center font-oswald font-bold text-[13px]" style={{ color: "#E8B84B" }}>{count}</span>
      </div>
      <span className="font-oswald text-[7px] uppercase tracking-[.15em] text-white/40">{label}</span>
    </div>
  );
}
