import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { Animated, Platform, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { useTheme, tokens } from '../../app-shell/theme';
import { withAlpha } from '../config/tokens';
import { AppText } from './Box';
import { useReducedMotion } from '../lib/useReducedMotion';

type Tone = 'done' | 'failed';

type ToastState = { id: number; message: string; tone: Tone } | null;

const ToastContext = createContext<(message: string, tone?: Tone) => void>(() => undefined);

/** Long enough to read a short sentence at a glance, short enough not to sit over the next thing you do. */
const VISIBLE_MS = 2600;

/**
 * One line, above the tab bar, that says what just happened.
 *
 * Every mutation in this app was silent. A room renamed, a location saved, a member made an admin, an alert list
 * cleared — the row changed and nothing else did. On a slow connection that is indistinguishable from a tap that
 * missed, which is the same complaint the command-outcome work answered for devices: *the app has to say when it did
 * what you asked, and say when it did not.*
 *
 * It is deliberately not a modal: `pointerEvents="none"` and an absolute position mean the customer can keep working
 * while it is on screen, and a second action replaces the first message rather than queueing behind it — the last
 * thing you did is the thing you want confirmation of.
 */
export function ToastProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<ToastState>(null);
  const next = useRef(0);

  const show = useCallback((message: string, tone: Tone = 'done') => {
    next.current += 1;
    setState({ id: next.current, message, tone });
  }, []);

  const value = useMemo(() => show, [show]);

  return (
    <ToastContext.Provider value={value}>
      {children}
      <Toast key={state?.id ?? 'none'} state={state} onDone={() => setState(null)} />
    </ToastContext.Provider>
  );
}

export function useToast() {
  return useContext(ToastContext);
}

function Toast({ state, onDone }: { state: ToastState; onDone: () => void }) {
  const { color } = useTheme();
  const insets = useSafeAreaInsets();
  const reduced = useReducedMotion();
  const fade = useRef(new Animated.Value(0)).current;
  const lift = useRef(new Animated.Value(8)).current;

  useEffect(() => {
    if (!state) return;
    const timer = setTimeout(onDone, VISIBLE_MS);
    if (reduced) {
      fade.setValue(1);
      lift.setValue(0);
      return () => clearTimeout(timer);
    }
    Animated.parallel([
      Animated.timing(fade, { toValue: 1, duration: 160, useNativeDriver: true }),
      Animated.timing(lift, { toValue: 0, duration: 160, useNativeDriver: true }),
    ]).start();
    return () => clearTimeout(timer);
  }, [state, reduced, fade, lift, onDone]);

  if (!state) return null;

  const failure = state.tone === 'failed';

  return (
    <View
      pointerEvents="none"
      // Above the tab bar, not over it: a message that covers the control you are about to press is a message in the
      // way. `bottom` includes the home-indicator inset so the pill never sits under the bar.
      style={{
        position: 'absolute',
        left: 0,
        right: 0,
        bottom: tokens.layout.tabBarHeight + insets.bottom + tokens.space.md,
        alignItems: 'center',
      }}
    >
      <Animated.View
        accessibilityRole="alert"
        // Screen readers announce a change to a live region; without this the confirmation is visual only, which is
        // the failure mode every silent mutation already had.
        accessibilityLiveRegion="polite"
        style={{
          maxWidth: 420,
          marginHorizontal: tokens.space.lg,
          paddingVertical: tokens.space.md,
          paddingHorizontal: tokens.space.lg,
          borderRadius: tokens.radius.pill,
          backgroundColor: color.card,
          borderWidth: 0.5,
          borderColor: failure ? color.danger : color.cardEdge,
          opacity: fade,
          transform: [{ translateY: lift }],
          ...Platform.select({
            // From the palette, not a literal: the shadow is a theme colour like any other, and the operator owns it.
            web: { boxShadow: `0 10px 30px ${withAlpha(color.shadow, 0.34)}` } as object,
            default: {
              shadowColor: color.shadow,
              shadowOpacity: 0.34,
              shadowRadius: 18,
              shadowOffset: { width: 0, height: 8 },
              elevation: 8,
            },
          }),
        }}
      >
        <View style={{ flexDirection: 'row', alignItems: 'center', gap: tokens.space.sm }}>
          {/* A dot, not a checkmark: the check would be a second icon language next to the tab bar's glyphs, and it
              claims "done" for things that will still fail (an optimistic rename). The dot is the accent, which is
              the app's own way of saying "this one is live". */}
          <View
            style={{
              width: 6,
              height: 6,
              borderRadius: 3,
              backgroundColor: failure ? color.danger : color.accent,
            }}
          />
          <AppText variant="caption" tone={failure ? 'danger' : undefined} weight="600">
            {state.message}
          </AppText>
        </View>
      </Animated.View>
    </View>
  );
}
