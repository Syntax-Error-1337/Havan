import { Pressable } from 'react-native';

import { AppText } from './Box';
import { Box } from './Box';
import { useTheme } from '../../app-shell/theme';
import { describeFailure } from '../api/errors';

/**
 * "We could not ask" is not "there is nothing".
 *
 * Every screen in this app renders an empty state, and before this component each of those empty states
 * doubled as the failure state — measured by blocking `/dashboard`: a home with six devices told the
 * customer **"No devices yet — Tap Add and we will look for devices nearby"**, which invites re-pairing
 * hardware that is already on the wall. On `/scenes` the same failure says "No scenes yet. Build one
 * below." and then a Save that cannot work.
 *
 * Three sentences do the work: what could not be loaded, that nothing has changed, and a way to ask
 * again. The last one matters most — an error screen without a retry is a dead end with an explanation.
 */
export function QueryError({
  title = 'We could not load this',
  body,
  error,
  onRetry,
  testID = 'query-error',
}: {
  title?: string;
  /** Overrides the derived sentence. A screen that knows something more specific than the status should say it. */
  body?: string;
  /**
   * The failure itself. Given one, the sentence is derived from it — the platform's own words when it answered, and the
   * connection when it did not — instead of every screen blaming the network for a 403.
   */
  error?: unknown;
  onRetry?: () => void;
  testID?: string;
}) {
  const { color } = useTheme();
  const failure = error === undefined ? null : describeFailure(error);
  const sentence =
    body ??
    failure?.body ??
    'Nothing has changed — the app could not reach the hub.';
  // A retry offered for a refusal the platform will repeat is a button that teaches the customer to press it forever.
  const retry = failure?.permanent ? undefined : onRetry;

  return (
    <Box p="xl" gap="md" center testID={testID}>
      <AppText variant="title" weight="600">
        {title}
      </AppText>
      <AppText tone="muted" style={{ textAlign: 'center' }}>
        {sentence}
      </AppText>
      {retry ? (
        <Pressable onPress={retry} accessibilityRole="button" hitSlop={10}>
          <AppText variant="body" style={{ color: color.accentInk }}>
            Try again
          </AppText>
        </Pressable>
      ) : null}
    </Box>
  );
}
