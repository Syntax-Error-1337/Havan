import { useCallback, useState, type ReactNode } from 'react';
import { Modal, Pressable, StyleSheet, View } from 'react-native';

import { useTheme, tokens } from '../../app-shell/theme';
import { AppText } from './Box';
import { withAlpha } from '../config/tokens';

/**
 * "Are you sure" as a surface the app draws, not a dialog the platform draws.
 *
 * `Alert.alert` is the obvious call and it is the wrong one here: `react-native-web` implements it as a
 * no-op, so every destructive confirmation in the web build silently did nothing — pressing "Delete
 * room" closed nothing and deleted nothing, and pressing "Remove this device" was equally inert. The
 * native dialog also cannot wear the operator's brand, and it cannot be tested.
 *
 * This is a small sheet over a translucent scrim: it works on all three platforms, it uses the palette,
 * and a test can press its buttons.
 */
export type ConfirmOptions = {
  title: string;
  message?: string;
  /** The verb, not "OK": "Remove", "Delete". The person should be able to read the button and know. */
  confirmLabel: string;
  cancelLabel?: string;
  destructive?: boolean;
  onConfirm: () => void;
};

/** Local state for one sheet. */
export function useConfirm() {
  const [options, setOptions] = useState<ConfirmOptions | null>(null);

  const ask = useCallback((next: ConfirmOptions) => setOptions(next), []);
  const close = useCallback(() => setOptions(null), []);

  const sheet: ReactNode = options ? (
    <Confirm
      {...options}
      onCancel={close}
      onConfirm={() => {
        close();
        options.onConfirm();
      }}
    />
  ) : null;

  return { ask, sheet };
}

export function Confirm({
  title,
  message,
  confirmLabel,
  cancelLabel = 'Cancel',
  destructive = false,
  onConfirm,
  onCancel,
}: ConfirmOptions & { onCancel: () => void }) {
  const { color } = useTheme();

  return (
    <Modal transparent animationType="fade" visible onRequestClose={onCancel}>
      <View
        style={{
          flex: 1,
          justifyContent: 'center',
          padding: tokens.space.xl,
          // A wash, not a blackout: the app behind it should still be recognisable, so the person
          // knows what they are changing rather than feeling they have left the screen.
          backgroundColor: withAlpha(color.shadow, 0.62),
        }}
      >
        {/* Opaque, unlike every other surface in this app: a dialog you can read the dashboard through is
            a dialog that is harder to answer than the question deserves. */}
        <View
          style={{
            padding: 20,
            borderRadius: tokens.radius.xl,
            backgroundColor: color.elevated,
            borderWidth: StyleSheet.hairlineWidth,
            borderColor: color.border,
          }}
        >
          <View style={{ gap: tokens.space.sm }}>
            <AppText variant="title" weight="700">
              {title}
            </AppText>
            {message ? (
              <AppText variant="caption" tone="muted">
                {message}
              </AppText>
            ) : null}
          </View>

          <View
            style={{
              flexDirection: 'row',
              justifyContent: 'flex-end',
              gap: tokens.space.lg,
              marginTop: tokens.space.lg,
            }}
          >
            <Pressable
              onPress={onCancel}
              accessibilityRole="button"
              testID="confirm-cancel"
              hitSlop={12}
            >
              <AppText tone="muted">{cancelLabel}</AppText>
            </Pressable>
            <Pressable
              onPress={onConfirm}
              accessibilityRole="button"
              testID="confirm-accept"
              hitSlop={12}
            >
              <AppText weight="600" style={{ color: destructive ? color.danger : color.accent }}>
                {confirmLabel}
              </AppText>
            </Pressable>
          </View>
        </View>
      </View>
    </Modal>
  );
}
