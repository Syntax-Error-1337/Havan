import { useState } from 'react';
import { Modal, Pressable, StyleSheet, TextInput, View } from 'react-native';

import { useTheme, tokens } from '../../app-shell/theme';
import { withAlpha } from '../../shared/config/tokens';
import { AppText, useToast } from '../../shared/ui';
import { useChangePassword } from '../../entities/session';

/**
 * Changing a password, which the app could not do at all.
 *
 * Two fields, because the old password *is* the proof: an app that let a borrowed phone set a new password without it
 * would be an app where a locked screen is the only security. The old one is checked on the server, and every other
 * session is ended by the same request — that is the point of changing it when you think somebody else has it.
 */
export function ChangePasswordSheet({ onClose }: { onClose: () => void }) {
  const { color, dark } = useTheme();
  const toast = useToast();
  const change = useChangePassword();
  const [current, setCurrent] = useState('');
  const [next, setNext] = useState('');

  const ready = current.length > 0 && next.length >= 8 && next !== current;
  const failure = change.error instanceof Error ? change.error.message : null;

  const field = {
    paddingVertical: tokens.space.sm,
    paddingHorizontal: tokens.space.md,
    borderRadius: tokens.radius.md,
    backgroundColor: color.card,
    borderWidth: StyleSheet.hairlineWidth,
    borderColor: color.border,
    color: color.text,
  } as const;

  const save = () => {
    if (!ready) return;
    change.mutate(
      { current, next },
      {
        onSuccess: (result) => {
          toast(
            result.signed_out > 0
              ? `Password changed — ${result.signed_out} other ${result.signed_out === 1 ? 'device' : 'devices'} signed out`
              : 'Password changed',
          );
          onClose();
        },
      },
    );
  };

  return (
    <Modal transparent animationType="fade" visible onRequestClose={onClose}>
      <View
        style={{
          flex: 1,
          justifyContent: 'center',
          padding: tokens.space.xl,
          backgroundColor: withAlpha(color.shadow, 0.62),
        }}
      >
        <View
          style={{
            padding: 20,
            borderRadius: tokens.radius.xl,
            backgroundColor: color.elevated,
            borderWidth: StyleSheet.hairlineWidth,
            borderColor: color.border,
            shadowColor: color.shadow,
            shadowOpacity: dark ? 0.5 : 0.22,
            shadowRadius: 24,
            shadowOffset: { width: 0, height: 12 },
            elevation: 12,
            gap: tokens.space.md,
          }}
        >
          <View style={{ gap: tokens.space.xs }}>
            <AppText variant="title" weight="700">
              Change password
            </AppText>
            <AppText variant="caption" tone="muted">
              Every other device is signed out. This one stays signed in.
            </AppText>
          </View>

          <View style={{ gap: tokens.space.xs }}>
            <AppText variant="tiny" tone="muted">
              Current password
            </AppText>
            <TextInput
              value={current}
              onChangeText={setCurrent}
              secureTextEntry
              autoFocus
              autoCapitalize="none"
              autoCorrect={false}
              accessibilityLabel="Current password"
              placeholderTextColor={color.placeholder}
              style={field}
            />
          </View>

          <View style={{ gap: tokens.space.xs }}>
            <AppText variant="tiny" tone="muted">
              New password
            </AppText>
            <TextInput
              value={next}
              onChangeText={setNext}
              secureTextEntry
              autoCapitalize="none"
              autoCorrect={false}
              accessibilityLabel="New password"
              placeholder="At least 8 characters"
              placeholderTextColor={color.placeholder}
              style={field}
            />
            {/* The rule is stated before it is broken, not after: a refusal that arrives on Save is a refusal that
                costs a round trip to learn something the screen could have said. */}
            {next.length > 0 && next.length < 8 ? (
              <AppText variant="tiny" tone="muted">
                {`${8 - next.length} more character${8 - next.length === 1 ? '' : 's'}`}
              </AppText>
            ) : null}
            {next.length > 0 && next === current ? (
              <AppText variant="tiny" tone="danger">
                That is the password you already have.
              </AppText>
            ) : null}
          </View>

          {failure ? (
            <AppText variant="tiny" tone="danger">
              {failure}
            </AppText>
          ) : null}

          <View style={{ flexDirection: 'row', alignItems: 'center', justifyContent: 'flex-end', gap: tokens.space.xl }}>
            <Pressable
              onPress={onClose}
              accessibilityRole="button"
              hitSlop={12}
              style={{ minHeight: tokens.touch.min, justifyContent: 'center' }}
            >
              <AppText tone="muted">Cancel</AppText>
            </Pressable>
            <Pressable
              onPress={save}
              accessibilityRole="button"
              accessibilityLabel="Save the new password"
              accessibilityState={{ disabled: !ready || change.isPending }}
              disabled={!ready || change.isPending}
              hitSlop={12}
              style={{ minHeight: tokens.touch.min, justifyContent: 'center' }}
            >
              <AppText weight="700" style={{ color: ready ? color.accent : color.muted }}>
                {change.isPending ? 'Saving…' : 'Save'}
              </AppText>
            </Pressable>
          </View>
        </View>
      </View>
    </Modal>
  );
}
