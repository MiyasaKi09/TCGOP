"use client";

import { useLayoutEffect, useRef } from "react";

/**
 * Zoom-from-origin transition (FLIP). Pass the bounding rect of the element the
 * user clicked; the returned ref's element animates from that position/size up
 * to its natural (enlarged) position. No-op if `origin` is null.
 */
export function useFlipZoom<T extends HTMLElement = HTMLDivElement>(origin: DOMRect | null | undefined) {
  const ref = useRef<T>(null);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el || !origin) return;
    const final = el.getBoundingClientRect();
    if (!final.width || !final.height) return;

    const scale = origin.width / final.width;
    const dx = origin.left + origin.width / 2 - (final.left + final.width / 2);
    const dy = origin.top + origin.height / 2 - (final.top + final.height / 2);

    // Start at the clicked card's place/size…
    el.style.transformOrigin = "center center";
    el.style.transition = "none";
    el.style.transform = `translate(${dx}px, ${dy}px) scale(${scale})`;
    el.style.opacity = "0.5";
    void el.offsetWidth; // force reflow so the next change animates

    // …then animate up to the enlarged view.
    el.style.transition = "transform .3s cubic-bezier(.2,.75,.25,1), opacity .22s ease-out";
    el.style.transform = "translate(0px, 0px) scale(1)";
    el.style.opacity = "1";

    const cleanup = () => {
      el.style.transition = "";
      el.style.transform = "";
      el.style.transformOrigin = "";
      el.style.opacity = "";
    };
    el.addEventListener("transitionend", cleanup, { once: true });
    return () => el.removeEventListener("transitionend", cleanup);
  }, [origin]);
  return ref;
}
