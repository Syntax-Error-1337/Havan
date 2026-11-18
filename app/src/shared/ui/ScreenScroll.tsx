import type { ReactNode } from 'react';
import { KeyboardAvoidingView, Platform, ScrollView, type ScrollViewProps } from 'react-native';

import { tokens } from '../../app-shell/theme';

/**
 * A scrolling screen body that behaves correctly on a phone.
 *
 * Three behaviours were being remembered per screen, and only the login screen remembered the third:
 *
 * 1. **Bottom clearance.** The tab bar floats over the content, so every scrollable body has to end above it.
 *    Seven screens each kept their own copy of `paddingBottom: tabBarClearance`, and the one screen that
 *    forgot (the device detail) hid its destructive action below the viewport.
 * 2. **`keyboardShouldPersistTaps`.** Without it, iOS treats a tap while the keyboard is up as "dismiss the
 *    keyboard" and swallows the press: on the scene editor, naming a scene and then tapping a device chip did
 *    nothing until you tapped again. `handled` lets the press through and still dismisses on empty space.
 * 3. **Keyboard avoidance.** A form at the bottom of the screen, on a phone, ends up behind the keyboard.
 *
 * The defaults here are the mobile-correct ones; a screen passes `padding`, `gap` or `style` when it wants to
 * differ, and nothing has to remember the rest.
 */
export function ScreenScroll({
  children,
  padded = true,
  gap,
  paddingTop,
  style,
  avoidKeyboard = true,
  ...rest
}: ScrollViewProps & {
  children: ReactNode;
  /** Whether the body gets the standard 16-point gutter. Off for a screen that draws edge-to-edge sections. */
  padded?: boolean;
  gap?: number;
  /** Overrides the top inset — a body that draws its own header inside the scroll needs one set explicitly. */
  paddingTop?: number;
  /** Turn off for a body with no inputs: the wrapper is not free, and a static screen never needs it. */
  avoidKeyboard?: boolean;
}) {
  const scroll = (
    <ScrollView
      // `handled`, not `always`: a tap on a button must land, and a tap on empty space should still dismiss.
      keyboardShouldPersistTaps="handled"
      keyboardDismissMode="interactive"
      showsVerticalScrollIndicator={false}
      contentContainerStyle={{
        paddingTop: paddingTop ?? (padded ? tokens.space.lg : 0),
        paddingHorizontal: padded ? tokens.space.lg : 0,
        paddingBottom: tokens.layout.tabBarClearance,
        gap,
      }}
      style={style}
      {...rest}
    >
      {children}
    </ScrollView>
  );

  if (!avoidKeyboard) return scroll;

  return (
    <KeyboardAvoidingView
      // iOS needs `padding` to push the body up; Android already resizes the window, and applying padding there
      // too double-counts the keyboard and leaves a gap the size of the keyboard behind the content.
      behavior={Platform.OS === 'ios' ? 'padding' : undefined}
      style={{ flex: 1 }}
    >
      {scroll}
    </KeyboardAvoidingView>
  );
}
