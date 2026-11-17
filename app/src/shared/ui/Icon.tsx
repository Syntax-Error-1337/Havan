import Svg, { Circle, Path, Rect } from 'react-native-svg';

/**
 * The icon set, drawn rather than shipped.
 *
 * Two reasons it is not an icon font or a raster sprite. First, a device's icon has to come from its
 * *data* — a product declares `category`, a thing model declares capabilities — so the app needs
 * shapes it can pick at runtime by name, and a font would mean a network dependency and a licence
 * question for a white-label build. Second, stroke paths scale to any size with one colour, which is
 * what the category accent needs.
 *
 * Every glyph is drawn on a 24×24 grid with 1.8 pt strokes so they read at 20 pt in a tile and at
 * 44 pt in a detail hero without a second set.
 */
export type IconName =
  | 'bulb'
  | 'plug'
  | 'sensor'
  | 'climate'
  | 'cover'
  | 'lock'
  | 'camera'
  | 'speaker'
  | 'vacuum'
  | 'switch'
  | 'energy'
  | 'home'
  | 'plus'
  | 'minus'
  | 'sparkle'
  | 'drop'
  | 'radar'
  | 'alert'
  | 'gear'
  | 'chevron-right'
  | 'lightning';

const STROKE = 1.8;

export function Icon({
  name,
  size = 20,
  color,
  filled = false,
}: {
  name: IconName;
  size?: number;
  color: string;
  /** Filled reads as "on"; the same path, a different conviction. */
  filled?: boolean;
}) {
  const common = {
    stroke: color,
    strokeWidth: STROKE,
    strokeLinecap: 'round' as const,
    strokeLinejoin: 'round' as const,
    fill: filled ? color : 'none',
  };

  return (
    <Svg width={size} height={size} viewBox="0 0 24 24" accessibilityRole="image">
      {name === 'bulb' && (
        <>
          <Path d="M9 17h6" {...common} fill="none" />
          <Path d="M10 21h4" {...common} fill="none" />
          <Path
            d="M12 3a6 6 0 0 0-3.5 10.9V17h7v-3.1A6 6 0 0 0 12 3Z"
            {...common}
          />
        </>
      )}
      {name === 'plug' && (
        <>
          <Path d="M9 3v5M15 3v5" {...common} fill="none" />
          <Path d="M6 8h12v3a6 6 0 0 1-6 6 6 6 0 0 1-6-6V8Z" {...common} />
          <Path d="M12 17v4" {...common} fill="none" />
        </>
      )}
      {name === 'sensor' && (
        <>
          <Circle cx="12" cy="12" r="2.2" {...common} />
          <Path d="M7.5 7.5a6.4 6.4 0 0 0 0 9M16.5 16.5a6.4 6.4 0 0 0 0-9" {...common} fill="none" />
          <Path d="M4.6 4.6a10.5 10.5 0 0 0 0 14.8M19.4 19.4a10.5 10.5 0 0 0 0-14.8" {...common} fill="none" />
        </>
      )}
      {name === 'climate' && (
        <>
          <Path d="M10 14.5V5a2 2 0 1 1 4 0v9.5a4 4 0 1 1-4 0Z" {...common} />
          <Circle cx="12" cy="17.5" r="1.4" {...common} />
        </>
      )}
      {name === 'cover' && (
        <>
          <Rect x="4" y="4" width="16" height="16" rx="2" {...common} fill="none" />
          <Path d="M4 9.5h16M4 15h16" {...common} fill="none" />
        </>
      )}
      {name === 'lock' && (
        <>
          <Rect x="5" y="10.5" width="14" height="9.5" rx="2" {...common} />
          <Path d="M8.5 10.5V8a3.5 3.5 0 0 1 7 0v2.5" {...common} fill="none" />
        </>
      )}
      {name === 'camera' && (
        <>
          <Rect x="3" y="7" width="13" height="10" rx="2" {...common} />
          <Path d="M16 11l5-3v8l-5-3" {...common} />
        </>
      )}
      {name === 'speaker' && (
        <>
          <Rect x="6" y="3" width="12" height="18" rx="2.5" {...common} fill="none" />
          <Circle cx="12" cy="15" r="3" {...common} />
          <Circle cx="12" cy="7.5" r="1.1" {...common} />
        </>
      )}
      {name === 'vacuum' && (
        <>
          <Circle cx="12" cy="12" r="8" {...common} fill="none" />
          <Circle cx="12" cy="12" r="2.6" {...common} />
          <Path d="M12 4v3.4M12 16.6V20" {...common} fill="none" />
        </>
      )}
      {name === 'switch' && (
        <>
          <Rect x="3" y="7" width="18" height="10" rx="5" {...common} fill="none" />
          <Circle cx={filled ? 16 : 8} cy="12" r="3.2" {...common} />
        </>
      )}
      {name === 'energy' && (
        <Path d="M13 2 5 13h5l-1 9 8-11h-5l1-9Z" {...common} />
      )}
      {name === 'lightning' && <Path d="M13 2 5 13h5l-1 9 8-11h-5l1-9Z" {...common} fill="none" />}
      {name === 'home' && (
        <>
          <Path d="M4 10.5 12 4l8 6.5V20a1 1 0 0 1-1 1h-4v-6H9v6H5a1 1 0 0 1-1-1v-9.5Z" {...common} />
        </>
      )}
      {name === 'plus' && <Path d="M12 5v14M5 12h14" {...common} fill="none" />}
      {name === 'minus' && <Path d="M5 12h14" {...common} fill="none" />}
      {name === 'sparkle' && (
        <>
          <Path d="M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8L12 3Z" {...common} />
          <Path d="M18.5 15.5l.8 2.2 2.2.8-2.2.8-.8 2.2-.8-2.2-2.2-.8 2.2-.8.8-2.2Z" {...common} fill="none" />
        </>
      )}
      {name === 'drop' && (
        <Path d="M12 3.5c3.2 3.9 5.5 6.9 5.5 9.6a5.5 5.5 0 1 1-11 0c0-2.7 2.3-5.7 5.5-9.6Z" {...common} />
      )}
      {name === 'radar' && (
        <>
          <Circle cx="12" cy="12" r="9" {...common} fill="none" />
          <Circle cx="12" cy="12" r="5" {...common} fill="none" />
          <Circle cx="12" cy="12" r="1.6" {...common} />
          <Path d="M12 3v4" {...common} fill="none" />
        </>
      )}
      {name === 'alert' && (
        <>
          <Path d="M12 4 2.8 20h18.4L12 4Z" {...common} fill="none" />
          <Path d="M12 10v4.5M12 17.2v.1" {...common} fill="none" />
        </>
      )}
      {name === 'gear' && (
        <>
          <Circle cx="12" cy="12" r="3.2" {...common} fill="none" />
          <Path
            d="M12 3.5v2M12 18.5v2M3.5 12h2M18.5 12h2M6 6l1.4 1.4M16.6 16.6 18 18M18 6l-1.4 1.4M7.4 16.6 6 18"
            {...common}
            fill="none"
          />
        </>
      )}
      {name === 'chevron-right' && <Path d="M9.5 6l6 6-6 6" {...common} fill="none" />}
    </Svg>
  );
}

/** A category's glyph. Unknown categories get a neutral tile rather than a wrong picture. */
export function iconForCategory(category: string | undefined): IconName {
  const known: Record<string, IconName> = {
    light: 'bulb',
    plug: 'plug',
    switch: 'switch',
    sensor: 'sensor',
    climate: 'climate',
    thermostat: 'climate',
    cover: 'cover',
    blind: 'cover',
    lock: 'lock',
    camera: 'camera',
    speaker: 'speaker',
    vacuum: 'vacuum',
    energy: 'energy',
    meter: 'energy',
  };
  return known[(category ?? '').toLowerCase()] ?? 'sensor';
}

/** A capability's glyph, so a tile can show what the device actually does. */
export function iconForCapability(capability: string | undefined): IconName {
  const known: Record<string, IconName> = {
    switch: 'switch',
    brightness: 'bulb',
    color: 'sparkle',
    color_temp: 'sparkle',
    temperature: 'climate',
    target_temperature: 'climate',
    humidity: 'sensor',
    motion: 'sensor',
    contact: 'sensor',
    illuminance: 'sensor',
    lock: 'lock',
    cover: 'cover',
    position: 'cover',
    battery: 'energy',
    power: 'energy',
    energy: 'energy',
    volume: 'speaker',
    playback: 'speaker',
    mode: 'gear',
  };
  return known[(capability ?? '').toLowerCase()] ?? 'sensor';
}
