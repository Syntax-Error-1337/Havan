import { createContext, useContext, useMemo, type ReactNode } from 'react';
import { tokens, type Theme } from '../shared/config/tokens';

const ThemeContext = createContext<Theme>({ color: tokens.color, dark: true });

/**
 * Dark first: a smart-home app is used at night, and it is what reviewers
 * screenshot. A light theme arrives from the same tokens, never from literals.
 */
export function ThemeProvider({ children }: { children: ReactNode }) {
  const theme = useMemo<Theme>(() => ({ color: tokens.color, dark: true }), []);
  return <ThemeContext.Provider value={theme}>{children}</ThemeContext.Provider>;
}

export function useTheme(): Theme {
  return useContext(ThemeContext);
}

export { tokens };
