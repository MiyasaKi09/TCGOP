"use client";

import { useEffect, useRef } from "react";

function webglAvailable(): boolean {
  try {
    const c = document.createElement("canvas");
    return !!(c.getContext("webgl2") || c.getContext("webgl"));
  } catch { return false; }
}

/** True only on capable, non-reduced-motion, larger displays. */
function highTier(): boolean {
  if (typeof window === "undefined") return false;
  if (window.matchMedia?.("(prefers-reduced-motion: reduce)").matches) return false;
  if (!webglAvailable()) return false;
  const cores = navigator.hardwareConcurrency ?? 8;
  const coarse = window.matchMedia?.("(pointer: coarse)").matches;
  if (cores <= 4 || coarse || window.innerWidth < 900) return false;
  return true;
}

/**
 * Background animated sea (separate Pixi app). Quality-gated: only mounts on
 * capable desktops. On anything else the static `.sea-bg` image remains.
 */
export default function AmbientStage() {
  const hostRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!highTier() || !hostRef.current) return;
    let handle: { destroy(): void } | null = null;
    let cancelled = false;

    (async () => {
      try {
        const { createAmbientStage } = await import("@/lib/vfx/ambient");
        if (cancelled || !hostRef.current) return;
        handle = await createAmbientStage(hostRef.current);
        if (cancelled) { handle?.destroy(); handle = null; }
      } catch (err) {
        console.error("AmbientStage init failed; keeping static sea", err);
      }
    })();

    return () => { cancelled = true; handle?.destroy(); };
  }, []);

  return <div ref={hostRef} className="absolute inset-0 pointer-events-none" style={{ zIndex: 0 }} aria-hidden />;
}
