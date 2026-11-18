import { useState } from 'react';
import { KeyboardAvoidingView, Platform, Pressable, TextInput, View } from 'react-native';
import { Box, BrandMark, Button, Screen, AppText } from '../../shared/ui';
import { useTheme, tokens } from '../../app-shell/theme';
import { useLogin, useRegister, useSession } from '../../entities/session';
import { ApiError } from '../../shared/api/client';

/**
 * One screen, two modes. An account is created with the same two fields, so a
 * separate signup flow would be the same form with more copy.
 */
export function LoginPage() {
  const { color, brand } = useTheme();
  const [mode, setMode] = useState<'signin' | 'signup'>('signin');
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [reveal, setReveal] = useState(false);

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
        {/* A sign-in form is a column of four elements. At 1024 points it stretched edge to edge and the fields
            became 900-point boxes — the same fault as a stretched dashboard, on the first screen anyone sees. */}
        <Box
          flex={1}
          p="xl"
          gap="lg"
          style={{ justifyContent: 'center', width: '100%', maxWidth: 420, alignSelf: 'center' }}
        >
          <Box gap="xs">
            {/* The sign-in screen is the first frame a customer sees, so it wears the operator's
                name and initials rather than ours — and so does the first-run screen, from the same component. */}
            <BrandMark />
            <AppText tone="muted">
              {mode === 'signin'
                ? 'Sign in to control your home.'
                : 'Create an account to get started.'}
            </AppText>
          </Box>

          <Box gap="md">
            <Box gap="xs">
              {/* A placeholder is not a label: it disappears on the first keystroke, and by then the person
                  is typing into a box whose name they have to remember. */}
              <AppText variant="tiny" tone="muted" style={{ letterSpacing: 1.1 }}>
                EMAIL
              </AppText>
              <TextInput
                value={email}
                onChangeText={setEmail}
                placeholder="you@example.com"
                placeholderTextColor={color.muted}
                autoCapitalize="none"
                autoComplete="email"
                textContentType="username"
                keyboardType="email-address"
                accessibilityLabel="Email"
                style={inputStyle(color)}
              />
            </Box>

            <Box gap="xs">
              <AppText variant="tiny" tone="muted" style={{ letterSpacing: 1.1 }}>
                PASSWORD
              </AppText>
              <View style={{ justifyContent: 'center' }}>
                <TextInput
                  value={password}
                  onChangeText={setPassword}
                  placeholder={mode === 'signup' ? 'At least 10 characters' : 'Your password'}
                  placeholderTextColor={color.muted}
                  // `current-password` is what tells a password manager to offer the saved one; `password`
                  // made both modes look like a new-password field.
                  secureTextEntry={!reveal}
                  autoComplete={mode === 'signup' ? 'new-password' : 'current-password'}
                  textContentType={mode === 'signup' ? 'newPassword' : 'password'}
                  accessibilityLabel="Password"
                  style={inputStyle(color)}
                />
                {/* Typing a password blind on a phone keyboard is the most common reason a sign-in fails
                    and the customer blames the app. */}
                <Pressable
                  onPress={() => setReveal((v) => !v)}
                  accessibilityRole="button"
                  accessibilityLabel={reveal ? 'Hide password' : 'Show password'}
                  hitSlop={10}
                  style={{ position: 'absolute', right: 14 }}
                >
                  <AppText variant="tiny" tone="muted">
                    {reveal ? 'HIDE' : 'SHOW'}
                  </AppText>
                </Pressable>
              </View>
            </Box>
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
                ? `New to ${brand.productName}? Create an account`
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
