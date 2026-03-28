/** Shuffle an array in place using Fisher-Yates */
export function shuffle<T>(arr: T[]): T[] {
  const result = [...arr];
  for (let i = result.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [result[i], result[j]] = [result[j], result[i]];
  }
  return result;
}

/** Generate a unique instance ID */
let instanceCounter = 0;
export function generateInstanceId(defId: string): string {
  return `${defId}_${++instanceCounter}_${Date.now().toString(36)}`;
}

/** Reset instance counter (for testing) */
export function resetInstanceCounter(): void {
  instanceCounter = 0;
}

/** Deep clone a plain object */
export function deepClone<T>(obj: T): T {
  return JSON.parse(JSON.stringify(obj));
}

/** Volonte cap */
export const VOLONTE_CAP = 10;

/** Starting hand size */
export const STARTING_HAND_SIZE = 6;

/** Bonus Vol on ally KO */
export const ALLY_KO_BONUS_VOL = 2;

/** All slots */
export const ALL_SLOTS = ["V1", "V2", "V3", "A1", "A2", "A3"] as const;

/** Front slots */
export const FRONT_SLOTS = ["V1", "V2", "V3"] as const;

/** Back slots */
export const BACK_SLOTS = ["A1", "A2", "A3"] as const;

/** Adjacency map */
export const ADJACENCY: Record<string, string[]> = {
  V1: ["V2", "A1"],
  V2: ["V1", "V3", "A2"],
  V3: ["V2", "A3"],
  A1: ["A2", "V1"],
  A2: ["A1", "A3", "V2"],
  A3: ["A2", "V3"],
};
