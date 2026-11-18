import { useState } from 'react';
import { useLocalSearchParams, useRouter } from 'expo-router';

import { AppText, Box, Button, Card, Screen, ScreenHeader, ScreenScroll } from '../src/shared/ui';
import { useAcceptInvite } from '../src/entities/home';
import { useSession } from '../src/entities/session';
import { useTheme, tokens } from '../src/app-shell/theme';

/**
 * `haven://join?token=…` — the screen an invitation link opens.
 *
 * Deliberately a screen and not an automatic action: joining somebody's home is a thing to agree to, and a link that
 * silently adds you to a house you were only shown is a link you cannot undo from the app. One button, and it says which
 * home and as what role before it is pressed.
 *
 * The token is checked by the server against the signed-in address, so a forwarded link fails with an explanation
 * rather than joining the wrong person.
 */
export default function JoinScreen() {
  const router = useRouter();
  const { color } = useTheme();
  const { token } = useLocalSearchParams<{ token?: string }>();
  const access = useSession((s) => s.access);
  const accept = useAcceptInvite();
  const [joined, setJoined] = useState<string | null>(null);

  const message = accept.error instanceof Error ? accept.error.message : null;

  return (
    <Screen>
      <ScreenHeader title="Join a home" />
      <ScreenScroll gap={tokens.space.lg}>
        <Card accent={color.accent}>
          <Box gap="sm">
            <AppText variant="title" weight="700">
              You have been invited
            </AppText>
            <AppText variant="caption" tone="muted">
              Accepting adds you to this home and to its devices. You can leave it again from Members.
            </AppText>
          </Box>
        </Card>

        {!token ? (
          <AppText variant="caption" tone="danger">
            This link is missing its invitation code. Ask for a new one.
          </AppText>
        ) : null}

        {!access ? (
          // Signing in first, on purpose: the invitation is bound to an address, and accepting it anonymously would
          // have to guess who is asking.
          <Box gap="sm">
            <AppText variant="caption" tone="muted">
              Sign in as the person who was invited, then come back to this link.
            </AppText>
            <Button label="Sign in" onPress={() => router.replace('/login')} />
          </Box>
        ) : joined ? (
          <Box gap="sm">
            <AppText weight="600">You are in.</AppText>
            <Button label="Open the home" onPress={() => router.replace('/')} />
          </Box>
        ) : (
          <Box gap="sm">
            <Button
              label={accept.isPending ? 'Joining…' : 'Accept invitation'}
              disabled={!token || accept.isPending}
              onPress={() =>
                accept.mutate(token ?? '', {
                  onSuccess: (result) => setJoined(result.home_id),
                })
              }
            />
            {message ? (
              <AppText variant="caption" tone="danger">
                {message}
              </AppText>
            ) : null}
          </Box>
        )}
      </ScreenScroll>
    </Screen>
  );
}
