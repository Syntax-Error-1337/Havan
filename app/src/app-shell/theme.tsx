import { createContext, useContext, useMemo, type ReactNode } from 'react';
import { useColorScheme } from 'react-native';
import { dark, light, tokens, type Palette, type Theme } from '../shared/config/tokens';

const ThemeContext = createContext<Theme>({ color: dark, dark: true });

/**
 * The app follows the system appearance.
 *
 * That is an HIG rule, not a preference: someone who set their phone to light
 * because of the sun outside should not get a black screen from us. `mode` exists
 * for the rare case that must force one (a viewfinder, a splash screen) and for
 * tests — never as a way to opt out of the system.
 */
export function ThemeProvider({
  children,
  mode,
}: {
  children: ReactNode;
  mode?: 'light' | 'dark';
}) {
  const system = useColorScheme();
  // When the platform reports no preference — which is what a browser often does — the app's own
  // look is the dark one. It is not a violation of "follow the system": there is no system answer to
  // follow, and defaulting a night-sky interface to white is the one choice nobody asked for.
  const isDark = (mode ?? system ?? 'dark') === 'dark';
  const theme = useMemo<Theme>(() => ({ color: isDark ? dark : light, dark: isDark }), [isDark]);

  return <ThemeContext.Provider value={theme}>{children}</ThemeContext.Provider>;
}

export function useTheme(): Theme {
  return useContext(ThemeContext);
}

/** A palette by name, for the few places that need both (a chart, a snapshot). */
export function palette(mode: 'light' | 'dark'): Palette {
  return mode === 'dark' ? dark : light;
}

export { tokens };
