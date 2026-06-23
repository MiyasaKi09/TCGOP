"use client";

import { useEffect, useRef, useState, useCallback } from "react";
import type { GameState, PlayerId, PendingAttack } from "@/types";
import { getCardDef, getCaptainDef } from "@/engine/cardRegistry";
import { styleFor, type VfxElement } from "./vfx";
import { vfxBus } from "./vfxBus";

export type VfxKind = "attack" | "impact" | "heal" | "ko" | "banner";

export interface VfxEvent {
  id: number;
  kind: VfxKind;
  element: VfxElement;
  fromX?: number; fromY?: number;
  toX?: number; toY?: number;
  value?: number;
  isSpecial?: boolean;
  zone?: boolean;
  impact?: boolean;
  big?: boolean;     // captain / special — grander
  label?: string;    // banner text
  sub?: string;      // banner subtitle
}

const REDUCED = typeof window !== "undefined" &&
  window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;

/** Screen-centre of a unit by its engine id ("instanceId" or "captain_playerX"). */
function centerOf(id: string): { x: number; y: number } | null {
  const el = document.querySelector(`[data-inst="${id}"]`) as HTMLElement | null;
  if (!el) return null;
  const r = el.getBoundingClientRect();
  return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
}

/** Briefly toggle a class on a unit's token. */
function flash(id: string, cls: string, ms: number) {
  const el = document.querySelector(`[data-inst="${id}"]`) as HTMLElement | null;
  if (!el) return;
  el.classList.add(cls);
  window.setTimeout(() => el.classList.remove(cls), ms);
}

interface Snapshot {
  pv: Record<string, number>;
  capPv: Record<string, number>;
  board: Set<string>;
  pendingSig: string | null;
}

function snapshot(state: GameState): Snapshot {
  const pv: Record<string, number> = {};
  const board = new Set<string>();
  for (const id in state.cards) {
    const c = state.cards[id];
    if (c.zone === "board") { pv[id] = c.currentPv; board.add(id); }
  }
  const capPv: Record<string, number> = {
    player1: state.players.player1.captain.currentPv,
    player2: state.players.player2.captain.currentPv,
  };
  const pa = state.pendingAttack;
  const pendingSig = pa ? `${pa.attackerId}>${pa.targetId}:${pa.rawDamage}:${pa.isSpecial}` : null;
  return { pv, capPv, board, pendingSig };
}

/** Resolve the on-screen target id of a pending attack. */
function targetId(pa: PendingAttack, state: GameState): string {
  if (pa.targetIsCaptain) {
    // attackerId is "<inst>" or "captain_pX"; the captain target belongs to the opponent.
    const owner = pa.attackerId.startsWith("captain_")
      ? (pa.attackerId.replace("captain_", "") as PlayerId)
      : state.cards[pa.attackerId]?.owner;
    const opp: PlayerId = owner === "player1" ? "player2" : "player1";
    return `captain_${opp}`;
  }
  return pa.targetId;
}

function attackLabel(state: GameState, pa: PendingAttack): { label?: string; sub?: string } {
  if (!pa.isSpecial) return {};
  try {
    if (pa.attackerId.startsWith("captain_")) {
      const pid = pa.attackerId.replace("captain_", "") as PlayerId;
      const cap = getCaptainDef(state.players[pid].captain.defId);
      return { label: cap.verso.specialAttack?.name ?? cap.name, sub: cap.name };
    }
    const def = getCardDef(state.cards[pa.attackerId].defId);
    return { label: def.specialAttack?.name ?? def.name, sub: def.name };
  } catch { return {}; }
}

export function useCombatVfx(
  state: GameState,
  boardRef: React.RefObject<HTMLElement | null>
): { events: VfxEvent[]; remove: (id: number) => void } {
  const [events, setEvents] = useState<VfxEvent[]>([]);
  const prev = useRef<Snapshot | null>(null);
  const lastPending = useRef<PendingAttack | null>(null);
  const idc = useRef(1);

  const remove = useCallback((id: number) => {
    setEvents((es) => es.filter((e) => e.id !== id));
  }, []);

  const shake = useCallback((big: boolean) => {
    const el = boardRef.current;
    if (!el || REDUCED) return;
    const cls = big ? "vfx-shake-hard" : "vfx-shake";
    el.classList.remove("vfx-shake", "vfx-shake-hard");
    // reflow to restart the animation
    void el.offsetWidth;
    el.classList.add(cls);
    window.setTimeout(() => el.classList.remove(cls), big ? 520 : 360);
  }, [boardRef]);

  useEffect(() => {
    const snap = snapshot(state);
    const p = prev.current;
    if (state.pendingAttack) lastPending.current = state.pendingAttack;

    if (!p) { prev.current = snap; return; }

    const out: VfxEvent[] = [];

    // --- 1. Attack declared (pending newly set / changed) → projectile + banner ---
    const pa = state.pendingAttack;
    if (pa && snap.pendingSig !== p.pendingSig && snap.pendingSig !== null) {
      const from = centerOf(pa.attackerId);
      const to = centerOf(targetId(pa, state));
      const st = styleFor(pa.element, pa.hasHaki);
      const big = pa.isSpecial || pa.attackerId.startsWith("captain_");
      const { label, sub } = attackLabel(state, pa);
      if (from && to) {
        out.push({
          id: idc.current++, kind: "attack", element: st.key,
          fromX: from.x, fromY: from.y, toX: to.x, toY: to.y,
          isSpecial: pa.isSpecial, zone: pa.attackTraits.includes("zone") || pa.attackTraits.includes("total"),
          impact: pa.attackTraits.includes("impact") || pa.pushback, big,
        });
      }
      // wind-up lunge on attacker
      flash(pa.attackerId, "vfx-lunge", 380);
      if (label) {
        out.push({ id: idc.current++, kind: "banner", element: st.key, label, sub, big });
      }
    }

    // --- 2. PV deltas → impacts (damage) / heals ---
    const lp = lastPending.current;
    const elementForTarget = (id: string): { el: VfxElement; impact: boolean; big: boolean } => {
      if (lp) {
        const tId = targetId(lp, state);
        if (tId === id || !lp.targetIsCaptain) {
          const st = styleFor(lp.element, lp.hasHaki);
          return { el: st.key, impact: lp.attackTraits.includes("impact") || !!lp.pushback, big: lp.isSpecial || lp.attackerId.startsWith("captain_") };
        }
      }
      return { el: "physical", impact: false, big: false };
    };

    // board characters
    for (const id of Object.keys({ ...p.pv, ...snap.pv })) {
      const before = p.pv[id];
      const after = snap.pv[id];
      if (before === undefined || after === undefined) continue;
      const d = after - before;
      if (d < 0) {
        const c = centerOf(id);
        const meta = elementForTarget(id);
        if (c) out.push({ id: idc.current++, kind: "impact", element: meta.el, toX: c.x, toY: c.y, value: -d, impact: meta.impact, big: meta.big });
        flash(id, meta.impact ? "vfx-knockback" : "vfx-hit", 520);
        shake(meta.big || -d >= 7);
      } else if (d > 0) {
        const c = centerOf(id);
        if (c) out.push({ id: idc.current++, kind: "heal", element: "physical", toX: c.x, toY: c.y, value: d });
        flash(id, "vfx-heal", 700);
      }
    }

    // captains
    for (const pid of ["player1", "player2"] as PlayerId[]) {
      const d = snap.capPv[pid] - p.capPv[pid];
      const id = `captain_${pid}`;
      if (d < 0) {
        const c = centerOf(id);
        const meta = elementForTarget(id);
        if (c) out.push({ id: idc.current++, kind: "impact", element: meta.el, toX: c.x, toY: c.y, value: -d, impact: meta.impact, big: true });
        flash(id, "vfx-hit", 520);
        shake(true);
      } else if (d > 0) {
        const c = centerOf(id);
        if (c) out.push({ id: idc.current++, kind: "heal", element: "physical", toX: c.x, toY: c.y, value: d });
        flash(id, "vfx-heal", 700);
      }
    }

    // --- 3. KO: a unit left the board ---
    for (const id of p.board) {
      if (!snap.board.has(id)) {
        const c = centerOf(id); // token may still be in the DOM this frame
        if (c) out.push({ id: idc.current++, kind: "ko", element: "physical", toX: c.x, toY: c.y });
      }
    }

    if (out.length) {
      // Feed the imperative WebGL layer (no reconciliation), then React state
      // for the DOM fallback / number+banner layer.
      for (const ev of out) vfxBus.emit(ev);
      setEvents((es) => [...es, ...out]);
    }
    prev.current = snap;
  }, [state, shake]);

  return { events, remove };
}
