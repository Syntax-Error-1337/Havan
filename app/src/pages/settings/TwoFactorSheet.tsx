import { useState } from 'react';
import { Modal, Pressable, StyleSheet, TextInput, View } from 'react-native';
import * as Clipboard from 'expo-clipboard';

import { useTheme, tokens } from '../../app-shell/theme';
import { withAlpha } from '../../shared/config/tokens';
import { AppText, Box, Button, useToast } from '../../shared/ui';
import { useConfirm } from '../../shared/ui';
import {
  useConfirmTotp,
  useDisableTotp,
  useStartTotp,
  useTotp,
} from '../../entities/session';

/**
 * The customer's second factor, end to end.
 *
 * Three states, because there genuinely are three: off, half-set-up (a seed exists and nobody proved their app works),
 * and on. Collapsing "pending" into "off" would throw away the seed somebody may already have typed into their phone;
 * collapsing it into "on" is the bug that locks people out of their own house.
 *
 * There is no QR code. A QR needs a renderer this app does not carry, and the two things an authenticator app actually
 * needs — the seed and the `otpauth://` link — are both here as text that can be copied. Saying "scan this" over a picture
 * that does not exist would be the worse trade.
 */
export function TwoFactorSheet({ onClose }: { onClose: () => void }) {
  const { color, dark } = useTheme();
  const toast = useToast();
  const { ask, sheet } = useConfirm();

  const state = useTotp();
  const start = useStartTotp();
  const confirm = useConfirmTotp();
  const disable = useDisableTotp();

  const [code, setCode] = useState('');
  const [copied, setCopied] = useState<'secret' | 'link' | null>(null);

  const current = state.data?.state ?? 'off';
  const started = start.data;
  const failure =
    (confirm.error ?? start.error ?? disable.error) instanceof Error
      ? ((confirm.error ?? start.error ?? disable.error) as Error).message
      : null;

  const field = {
    paddingVertical: tokens.space.sm,
    paddingHorizontal: tokens.space.md,
    borderRadius: tokens.radius.md,
    backgroundColor: color.card,
    borderWidth: StyleSheet.hairlineWidth,
    borderColor: color.border,
    color: color.text,
  } as const;

  const copy = async (what: 'secret' | 'link', value: string) => {
    await Clipboard.setStringAsync(value);
    setCopied(what);
    // A copy that says nothing leaves the person wondering whether the press landed.
    toast(what === 'secret' ? 'Secret copied' : 'Link copied');
  };

  const finish = () => {
    if (code.trim().length !== 6) return;
    confirm.mutate(code.trim(), {
      onSuccess: () => {
        toast('Two-factor authentication is on');
        setCode('');
        onClose();
      },
    });
  };

  const turnOff = () => {
    ask({
      title: 'Turn off two-factor authentication?',
      message:
        'Your password alone will sign you in again. If you are turning it off because you lost your phone, change your password afterwards.',
      confirmLabel: 'Turn off',
      destructive: true,
      onConfirm: () =>
        disable.mutate(undefined, {
          onSuccess: () => {
            toast('Two-factor authentication is off');
            onClose();
          },
        }),
    });
  };

  return (
    <Modal transparent animationType="fade" visible onRequestClose={onClose}>
      {sheet}
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
              Two-factor authentication
            </AppText>
            <AppText variant="caption" tone="muted">
              {current === 'on'
                ? 'A code from your authenticator app is asked for at every sign-in.'
                : 'A code from an authenticator app, on top of your password.'}
            </AppText>
          </View>

          {/* Off, or half-set-up and the seed is in this session only. Either way the next step is a fresh seed. */}
          {current !== 'on' && !started ? (
            <Box gap="sm">
              <AppText variant="caption" tone="muted">
                {state.data?.state === 'pending'
                  ? 'An earlier attempt was never finished, so it does not protect anything yet. Setting it up again replaces that secret.'
                  : 'Your account can unlock a door. One password is one thing to lose.'}
              </AppText>
              <Button
                label={state.data?.state === 'pending' ? 'Start again' : 'Set up'}
                onPress={() => start.mutate()}
                loading={start.isPending}
                testID="totp-start"
              />
            </Box>
          ) : null}

          {started ? (
            <Box gap="sm">
              <AppText variant="caption" tone="muted">
                Add this to your authenticator app, then type the six digits it shows.
              </AppText>

              <Pressable
                onPress={() => copy('secret', started.secret)}
                accessibilityRole="button"
                accessibilityLabel="Copy the secret"
                accessibilityHint="Copies the setup key to the clipboard"
                style={field}
              >
                <AppText
                  style={{ fontFamily: 'Courier', letterSpacing: 1.2 }}
                  testID="totp-secret"
                >
                  {started.secret}
                </AppText>
              </Pressable>

              <Pressable
                onPress={() => copy('link', started.uri)}
                accessibilityRole="button"
                accessibilityLabel="Copy the setup link"
                style={{ minHeight: tokens.touch.min, justifyContent: 'center' }}
              >
                <AppText variant="tiny" style={{ color: color.accent }}>
                  {copied === 'link' ? 'Link copied' : 'Copy the setup link instead'}
                </AppText>
              </Pressable>

              <View style={{ gap: tokens.space.xs }}>
                <AppText variant="tiny" tone="muted">
                  Code
                </AppText>
                <TextInput
                  value={code}
                  onChangeText={setCode}
                  keyboardType="number-pad"
                  maxLength={6}
                  autoFocus
                  accessibilityLabel="Six-digit code"
                  placeholder="123456"
                  placeholderTextColor={color.placeholder}
                  style={field}
                />
              </View>

              <Button
                label="Turn on"
                onPress={finish}
                loading={confirm.isPending}
                disabled={code.trim().length !== 6}
                testID="totp-confirm"
              />
            </Box>
          ) : null}

          {current === 'on' ? (
            <Box gap="sm">
              <AppText variant="caption" tone="muted">
                Codes come from the app you set up, not from Haven. Losing that app means losing the second factor — turn
                it off here while you still have your password.
              </AppText>
              <Button
                label="Turn off"
                variant="ghost"
                onPress={turnOff}
                loading={disable.isPending}
                testID="totp-disable"
              />
            </Box>
          ) : null}

          {failure ? (
            <AppText variant="tiny" tone="danger">
              {failure}
            </AppText>
          ) : null}

          <Pressable
            onPress={onClose}
            accessibilityRole="button"
            hitSlop={12}
            style={{ minHeight: tokens.touch.min, justifyContent: 'flex-end' }}
          >
            <AppText tone="muted">Close</AppText>
          </Pressable>
        </View>
      </View>
    </Modal>
  );
}
