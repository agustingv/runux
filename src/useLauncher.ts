import { useCallback, useEffect, useRef, useState } from "react";
import { api, errorMessage, events, type Task, type EaGame, type ExternalGame, type Game, type LogLine, type Progress, type Status } from "./api";
import { eaKey, externalKey } from "./components/Sidebar";

const POLL_MS = 3000;
const MAX_LOGS = 1000;
/** How long the "Iniciando…" state lasts if the game never shows up as running. */
const LAUNCH_TIMEOUT_MS = 45_000;

/**
 * What the launcher's backend reports, kept up to date: the clients' status
 * and the games, refreshed every few seconds and after each task, plus the
 * tasks in progress, their download progress and the log.
 */
export function useLauncher({ onTaskError }: { onTaskError: () => void }) {
  const [status, setStatus] = useState<Status | null>(null);
  const [games, setGames] = useState<Game[]>([]);
  const [external, setExternal] = useState<ExternalGame[]>([]);
  const [eaGames, setEaGames] = useState<EaGame[]>([]);
  const [busy, setBusy] = useState<Set<string>>(new Set());
  const [launching, setLaunching] = useState<Record<string, number>>({});
  const [progress, setProgress] = useState<Record<string, Progress>>({});
  const [logs, setLogs] = useState<LogLine[]>([]);
  const [error, setError] = useState<string | null>(null);

  const markBusy = useCallback((task: string, on: boolean) => {
    setBusy((prev) => {
      const next = new Set(prev);
      if (on) next.add(task);
      else next.delete(task);
      return next;
    });
  }, []);

  /** Tasks the user cancelled, whose failure isn't an error to show. */
  const cancelled = useRef(new Set<string>());

  // One refresh at a time: asking while one runs queues another after it,
  // so slow ones don't pile up and the latest change is always picked up.
  const refreshing = useRef(false);
  const refreshAgain = useRef(false);

  const refresh = useCallback(async () => {
    if (refreshing.current) {
      refreshAgain.current = true;
      return;
    }
    refreshing.current = true;
    try {
      do {
        refreshAgain.current = false;
        const s = await api.snapshot();
        setStatus(s.status);
        setGames(s.games);
        setExternal(s.external);
        setEaGames(s.eaGames);
      } while (refreshAgain.current);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      refreshing.current = false;
    }
  }, []);

  useEffect(() => {
    refresh();
    const id = setInterval(refresh, POLL_MS);
    return () => clearInterval(id);
  }, [refresh]);

  useEffect(() => {
    const subs = [
      events.onLog((l) => setLogs((prev) => [...prev.slice(-MAX_LOGS + 1), l])),
      events.onProgress((p) => setProgress((prev) => ({ ...prev, [p.task]: p }))),
      events.onTaskFinished((t) => {
        markBusy(t.task, false);
        if (!t.success) setError(`La tarea "${t.task}" terminó con error. Revisa el registro.`);
        refresh();
      }),
    ];
    return () => subs.forEach((p) => p.then((unlisten) => unlisten()));
  }, [markBusy, refresh]);

  // Drop the "launching" flag once the game runs or the timeout expires.
  useEffect(() => {
    const now = Date.now();
    const stale = Object.entries(launching).filter(
      ([key, t]) =>
        now - t > LAUNCH_TIMEOUT_MS ||
        games.find((g) => g.uid === key)?.running ||
        external.find((g) => externalKey(g.id) === key)?.running ||
        eaGames.find((g) => eaKey(g.id) === key)?.running,
    );
    if (stale.length) {
      setLaunching((prev) => {
        const next = { ...prev };
        stale.forEach(([key]) => delete next[key]);
        return next;
      });
    }
  }, [games, external, eaGames, launching]);

  /** Runs a command that finishes when the work is done; its result, or `undefined` if it failed. */
  const runTask = async <T,>(task: string, fn: () => Promise<T>): Promise<T | undefined> => {
    markBusy(task, true);
    setError(null);
    try {
      return await fn();
    } catch (e) {
      if (!cancelled.current.delete(task)) {
        setError(errorMessage(e));
        onTaskError();
      }
    } finally {
      markBusy(task, false);
      refresh();
    }
  };

  /** Runs a command that spawns a background process; `task-finished` clears it. */
  const startTask = async (task: string, fn: () => Promise<unknown>) => {
    markBusy(task, true);
    setError(null);
    try {
      await fn();
    } catch (e) {
      markBusy(task, false);
      setError(errorMessage(e));
    }
    refresh();
  };

  /** Launches the game with sidebar key `key`, showing "Iniciando…" until it runs. */
  const play = (key: string, launch: () => Promise<void>) => {
    setLaunching((prev) => ({ ...prev, [key]: Date.now() }));
    startTask(key, launch);
  };

  /** Stops a task's download; the task then ends quietly. */
  const cancel = (task: Task) => {
    cancelled.current.add(task);
    api.cancelDownload(task).catch((e) => setError(errorMessage(e)));
  };

  const clearProgress = (task: string) =>
    setProgress((prev) => {
      const { [task]: _, ...rest } = prev;
      return rest;
    });

  /** Wraps an action so that its failure shows as the error banner. */
  const guard = (fn: () => Promise<unknown>) => () => fn().catch((e) => setError(errorMessage(e)));

  return {
    status, games, external, eaGames, busy, launching, progress, logs, error,
    setError, clearLogs: () => setLogs([]), refresh, runTask, startTask, play, cancel, clearProgress, guard,
  };
}
