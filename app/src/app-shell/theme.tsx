import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from 'react';
import { useColorScheme } from 'react-native';

import { applyBrand, dark, light, tokens, type Palette, type Theme } from '../shared/config/tokens';
import { DEFAULT_THEME, cachedTheme, hydrateThemeCache, usePlatformTheme } from '../entities/theme/api';
import { usePreferences } from '../entities/preferences';

const ThemeContext = createContext<Theme>({
  color: dark,
  dark: true,
  brand: {
    productName: DEFAULT_THEME.product_name,
    logoText: DEFAULT_THEME.logo_text,
    supportEmail: null,
    statusUrl: null,
  },
});

/**
 * The app wears the platform's colours.
 *
 * The operator sets an accent, a second accent and an appearance in the console; this turns those
 * into the palette every screen already reads, so no screen had to change to become white-label.
 *
 * `mode` still exists and still wins, because a test and a splash screen need a fixed answer. With
 * no operator theme the app's own night look is the default — a browser reports "light" whether or
 * not anybody chose it, and a home interface defaulting to white is the one choice nobody asked for.
 */
export function ThemeProvider({
  children,
  mode,
  accent,
  accentOn,
  accentSecondary,
  productName,
  logoText,
  supportEmail,
  statusUrl,
}: {
  children: ReactNode;
  mode?: 'light' | 'dark';
  accent?: string;
  accentOn?: string;
  accentSecondary?: string;
  productName?: string;
  logoText?: string;
  /** The operator's own contact details — platform data, like the colours, so one white-label deployment is one place to change them. */
  supportEmail?: string | null;
  statusUrl?: string | null;
}) {
  const system = useColorScheme();
  const isDark = (mode ?? system ?? 'dark') === 'dark';

  const theme = useMemo<Theme>(() => {
    const base = isDark ? dark : light;
    const brand = {
      productName: productName ?? DEFAULT_THEME.product_name,
      logoText: logoText ?? DEFAULT_THEME.logo_text,
      supportEmail: supportEmail ?? null,
      statusUrl: statusUrl ?? null,
    };

    if (!accent) return { color: base, dark: isDark, brand };

    return {
      color: applyBrand(base, { accent, accentOn, accentSecondary }),
      dark: isDark,
      brand,
    };
  }, [isDark, accent, accentOn, accentSecondary, productName, logoText, supportEmail, statusUrl]);

  return <ThemeContext.Provider value={theme}>{children}</ThemeContext.Provider>;
}

/**
 * The provider plus the platform's own answer.
 *
 * Split from `ThemeProvider` so the provider stays a pure function of its props: a test supplies a
 * palette without a network, and the app supplies the network's palette without a test.
 */
export function PlatformThemeProvider({ children }: { children: ReactNode }) {
  // The customer's own choice, if they made one. The operator sets the default for everybody; a
  // person who prefers light at 6 p.m. is not disagreeing with the brand, only with the clock.
  const appearance = usePreferences((s) => s.appearance);
  const hydratePreferences = usePreferences((s) => s.hydrate);
  useEffect(() => {
    void hydratePreferences();
  }, [hydratePreferences]);

  // The cached answer first, so a cold start offline opens in the right brand rather than flashing
  // the built-in one and correcting itself a second later.
  const [remembered, setRemembered] = useState(cachedTheme);
  useEffect(() => {
    hydrateThemeCache().then((theme) => {
      if (theme) setRemembered(theme);
    });
  }, []);

  const { data } = usePlatformTheme();
  const theme = data ?? remembered ?? DEFAULT_THEME;

  return (
    <ThemeProvider
      mode={appearance === 'system' ? theme.mode : appearance}
      accent={theme.accent}
      accentOn={theme.accent_on}
      accentSecondary={theme.accent_secondary}
      productName={theme.product_name}
      logoText={theme.logo_text}
      supportEmail={theme.support_email ?? null}
      statusUrl={theme.status_url ?? null}
    >
      {children}
    </ThemeProvider>
  );
}

export function useTheme(): Theme {
  return useContext(ThemeContext);
}

/** A palette by name, for the few places that need both (a chart, a snapshot). */
export function palette(mode: 'light' | 'dark'): Palette {
  return mode === 'dark' ? dark : light;
}

export { tokens };
