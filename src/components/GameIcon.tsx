import { useEffect, useState, type CSSProperties } from "react";
import { api, initials } from "../api";

/** Icons by game key, asked for once per session. */
const cache = new Map<string, Promise<string | null>>();

function iconOf(gameKey: string): Promise<string | null> {
  let icon = cache.get(gameKey);
  if (!icon) {
    icon = api.gameIcon(gameKey).catch(() => null);
    cache.set(gameKey, icon);
  }
  return icon;
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

/** The icon in the game's executable, or its initials without one. */
export function GameIcon({ gameKey, name, className, ready, style }: Props) {
  const [icon, setIcon] = useState<string | null>(null);
  // Old games only carry 32 or 48 px icons, which blur when scaled up.
  const [small, setSmall] = useState(false);

  useEffect(() => {
    setIcon(null);
    if (!ready) return;
    let current = true;
    iconOf(gameKey).then((i) => current && setIcon(i));
    return () => { current = false; };
  }, [gameKey, ready]);

  if (!icon) return <span className={className} style={style}>{initials(name)}</span>;
  return (
    <span className={`${className} has-icon ${small ? "small" : ""}`}>
      <img src={icon} alt="" onLoad={(e) => setSmall(e.currentTarget.naturalWidth < 128)} />
    </span>
  );
}
