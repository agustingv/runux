import { useCallback, useEffect, useState } from "react";

/**
 * A value kept in the webview's storage, for small conveniences only: it
 * falls back to `initial` when storage is unavailable or holds something else.
 */
export function useStored<T>(key: string, initial: T): [T, (update: (prev: T) => T) => void] {
  const [value, setValue] = useState<T>(() => {
    try {
      const raw = localStorage.getItem(key);
      return raw === null ? initial : (JSON.parse(raw) as T);
    } catch {
      return initial;
    }
  });
  const update = useCallback((fn: (prev: T) => T) => {
    setValue((prev) => {
      const next = fn(prev);
      try {
        localStorage.setItem(key, JSON.stringify(next));
      } catch {
        // Not saved: it still holds until Runux closes.
      }
      return next;
    });
  }, [key]);
  return [value, update];
}

/**
 * When each game was last seen running, by its key. Watching the running
 * state catches every way of starting a game, from Runux or from a client.
 */
export function useRecentlyPlayed(running: string[]): Record<string, number> {
  const [played, setPlayed] = useStored<Record<string, number>>("recently-played", {});
  const runningKey = running.join("\n");
  useEffect(() => {
    if (!runningKey) return;
    const now = Date.now();
    setPlayed((prev) => {
      const next = { ...prev };
      for (const key of runningKey.split("\n")) next[key] = now;
      return next;
    });
  }, [runningKey, setPlayed]);
  return played;
}
