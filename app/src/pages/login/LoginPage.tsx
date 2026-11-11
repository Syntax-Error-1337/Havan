import { useState } from 'react';
import { KeyboardAvoidingView, Platform, Pressable, TextInput } from 'react-native';
import { Box, Button, Screen, AppText } from '../../shared/ui';
import { useTheme, tokens } from '../../app-shell/theme';
import { useLogin, useRegister, useSession } from '../../entities/session';
import { ApiError } from '../../shared/api/client';

/**
 * One screen, two modes. An account is created with the same two fields, so a
 * separate signup flow would be the same form with more copy.
 */
export function LoginPage() {
  const { color } = useTheme();
  const [mode, setMode] = useState<'signin' | 'signup'>('signin');
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [error, setError] = useState<string | null>(null);

  const signIn = useSession((s) => s.signIn);
  const login = useLogin();
  const register = useRegister();
  const pending = login.isPending || register.isPending;

  const submit = async () => {
    setError(null);
    try {
      const pair =
        mode === 'signin'
          ? await login.mutateAsync({ email: email.trim(), password })
          : await register.mutateAsync({ email: email.trim(), password });
      await signIn({ access: pair.access, refresh: pair.refresh });
    } catch (e) {
      setError(
        e instanceof ApiError
          ? e.message
          : 'Something went wrong. Check your connection and try again.',
      );
    }
  };

  const canSubmit = email.includes('@') && password.length >= 10 && !pending;

  return (
    <Screen>
      <KeyboardAvoidingView
        behavior={Platform.OS === 'ios' ? 'padding' : undefined}
        style={{ flex: 1 }}
      >
        <Box flex={1} p="xl" gap="lg" style={{ justifyContent: 'center' }}>
          <Box gap="xs">
            <AppText variant="display" weight="700">
              Haven
            </AppText>
            <AppText tone="muted">
              {mode === 'signin'
                ? 'Sign in to control your home.'
                : 'Create an account to get started.'}
            </AppText>
          </Box>

          <Box gap="md">
            <TextInput
              value={email}
              onChangeText={setEmail}
              placeholder="Email"
              placeholderTextColor={color.muted}
              autoCapitalize="none"
              autoComplete="email"
              keyboardType="email-address"
              accessibilityLabel="Email"
              style={inputStyle(color)}
            />
            <TextInput
              value={password}
              onChangeText={setPassword}
              placeholder={mode === 'signup' ? 'Password (10+ characters)' : 'Password'}
              placeholderTextColor={color.muted}
              secureTextEntry
              autoComplete="password"
              accessibilityLabel="Password"
              style={inputStyle(color)}
            />
          </Box>

          {error ? (
            <AppText tone="danger" variant="caption">
              {error}
            </AppText>
          ) : null}

          <Button
            label={mode === 'signin' ? 'Sign in' : 'Create account'}
            onPress={submit}
            loading={pending}
            disabled={!canSubmit}
            full
          />

          <Pressable
            onPress={() => {
              setMode(mode === 'signin' ? 'signup' : 'signin');
              setError(null);
            }}
            accessibilityRole="button"
            hitSlop={12}
          >
            <AppText tone="muted" style={{ textAlign: 'center' }}>
              {mode === 'signin'
                ? 'No account yet? Create one'
                : 'Already have an account? Sign in'}
            </AppText>
          </Pressable>
        </Box>
      </KeyboardAvoidingView>
    </Screen>
  );
}

function inputStyle(color: ReturnType<typeof useTheme>['color']) {
  return {
    minHeight: tokens.touch.min,
    paddingHorizontal: tokens.space.lg,
    borderRadius: tokens.radius.md,
    backgroundColor: color.surface,
    borderWidth: 1,
    borderColor: color.border,
    color: color.text,
    fontSize: tokens.font.body,
  };
}
