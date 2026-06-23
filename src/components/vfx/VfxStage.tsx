"use client";

import { useEffect, useRef } from "react";

function webglAvailable(): boolean {
  try {
    const c = document.createElement("canvas");
    return !!(c.getContext("webgl2") || c.getContext("webgl"));
  } catch { return false; }
}

/**
 * Mounts a full-screen PixiJS overlay (particles/projectiles/bursts) and reports
 * whether WebGL is live via `onActiveChange`. Pixi is dynamically imported inside
 * the effect, so it never enters the static graph / SSR / initial bundle. Falls
 * back (active=false) on reduced-motion, missing WebGL, init failure, or context loss.
 */
export default function VfxStage({ onActiveChange }: { onActiveChange: (active: boolean) => void }) {
  const hostRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (typeof window === "undefined") return;
    const reduced = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
    if (reduced || !webglAvailable() || !hostRef.current) {
      onActiveChange(false);
      return;
    }

    let handle: { destroy(): void } | null = null;
    let cancelled = false;

    (async () => {
      try {
        const { createVfxStage } = await import("@/lib/vfx/stage");
        if (cancelled || !hostRef.current) return;
        handle = await createVfxStage(hostRef.current, {
          onContextLost: () => onActiveChange(false),
        });
        if (cancelled) { handle?.destroy(); handle = null; return; }
        onActiveChange(true);
      } catch (err) {
        console.error("VfxStage init failed; falling back to CSS VFX", err);
        onActiveChange(false);
      }
    })();

    return () => {
      cancelled = true;
      onActiveChange(false);
      handle?.destroy();
    };
  }, [onActiveChange]);

  return <div ref={hostRef} className="fixed inset-0 pointer-events-none" style={{ zIndex: 20 }} aria-hidden />;
}
