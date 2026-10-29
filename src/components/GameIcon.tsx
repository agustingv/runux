import { useEffect, useState, type CSSProperties } from "react";
import { api, initials } from "../api";

/** Pictures by game key, asked for once per session. */
const icons = new Map<string, Promise<string | null>>();
const backgrounds = new Map<string, Promise<string | null>>();

function cached(cache: Map<string, Promise<string | null>>, gameKey: string, get: (key: string) => Promise<string | null>) {
  let picture = cache.get(gameKey);
  if (!picture) {
    picture = get(gameKey).catch(() => null);
    cache.set(gameKey, picture);
  }
  return picture;
}

/** Shown icons, re-asked for when a game's icon changes (a cover downloaded or removed). */
const shown = new Map<string, Set<() => void>>();

/** Forgets the game's icon, so that it's asked for again everywhere it's shown. */
export function refreshIcon(gameKey: string) {
  icons.delete(gameKey);
  shown.get(gameKey)?.forEach((reload) => reload());
}

/**
 * The picture an emulator game's disc shows behind it in the console's menu
 * (PS3 and PSP ones have it), or `null`.
 */
export function useGameBackground(gameKey: string): string | null {
  const [background, setBackground] = useState<string | null>(null);
  useEffect(() => {
    setBackground(null);
    // Only games added by hand can be emulator games.
    if (!gameKey.startsWith("ext:")) return;
    let current = true;
    cached(backgrounds, gameKey, api.gameBackground).then((b) => current && setBackground(b));
    return () => { current = false; };
  }, [gameKey]);
  return background;
}

interface Props {
  /** The game's key in the sidebar. */
  gameKey: string;
  name: string;
  className: string;
  /** False while there is no executable to read it from. */
  ready: boolean;
  style?: CSSProperties;
}

/** The icon in the game's executable or on its disc, or its initials without one. */
export function GameIcon({ gameKey, name, className, ready, style }: Props) {
  const [icon, setIcon] = useState<string | null>(null);
  // Old games only carry 32 or 48 px icons, which blur when scaled up.
  const [small, setSmall] = useState(false);
  // PS3 and PSP icons are logos, about twice as wide as tall.
  const [wide, setWide] = useState(false);
  const [version, setVersion] = useState(0);

  useEffect(() => {
    const reload = () => setVersion((v) => v + 1);
    const set = shown.get(gameKey) ?? new Set();
    shown.set(gameKey, set.add(reload));
    return () => { set.delete(reload); };
  }, [gameKey]);

  useEffect(() => {
    setIcon(null);
    if (!ready) return;
    let current = true;
    cached(icons, gameKey, api.gameIcon).then((i) => current && setIcon(i));
    return () => { current = false; };
  }, [gameKey, ready, version]);

  if (!icon) return <span className={className} style={style}>{initials(name)}</span>;
  return (
    <span className={`${className} has-icon ${small ? "small" : ""} ${wide ? "wide" : ""}`}>
      <img
        src={icon}
        alt=""
        onLoad={(e) => {
          const { naturalWidth: w, naturalHeight: h } = e.currentTarget;
          setSmall(w < 128);
          setWide(w > h * 1.3);
        }}
      />
    </span>
  );
}
