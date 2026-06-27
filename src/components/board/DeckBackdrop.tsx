"use client";

// CSS approximation of the maquette's top-down ship deck: a rounded wooden
// "island" of planks over the sea, with hull edges + vignette. Authored so a
// real `plateau-deck.png` can replace `background` here later if exported.
export default function DeckBackdrop() {
  return (
    <div className="absolute inset-0 flex items-center justify-center pointer-events-none select-none" aria-hidden>
      <div
        className="relative"
        style={{
          width: "96%",
          height: "97%",
          borderRadius: "44% / 12%",
          border: "3px solid #29190d",
          background: [
            // central warm highlight
            "radial-gradient(120% 80% at 50% 38%, rgba(150,110,66,.35), transparent 60%)",
            // plank seams (run along the deck length)
            "repeating-linear-gradient(0deg, rgba(0,0,0,.16) 0 2px, transparent 2px 44px)",
            // faint cross grain
            "repeating-linear-gradient(90deg, rgba(255,255,255,.025) 0 1px, transparent 1px 130px)",
            // base wood
            "linear-gradient(180deg, #6c4a2b 0%, #5a3c22 48%, #46301a 100%)",
          ].join(","),
          boxShadow:
            "inset 0 0 0 5px #3a2817, inset 0 0 90px rgba(0,0,0,.55), inset 0 0 22px rgba(0,0,0,.5), 0 24px 70px rgba(0,0,0,.62)",
        }}
      >
        {/* edge vignette so the centre reads brighter than the rails */}
        <div
          className="absolute inset-0"
          style={{ borderRadius: "inherit", background: "radial-gradient(120% 90% at 50% 50%, transparent 52%, rgba(6,9,14,.55) 100%)" }}
        />
      </div>
    </div>
  );
}
