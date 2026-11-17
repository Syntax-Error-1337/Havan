import { Pressable } from 'react-native';

import { AppText } from './Box';
import { Box } from './Box';
import { useTheme } from '../../app-shell/theme';

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
  body = 'Nothing has changed — the app could not reach the hub.',
  onRetry,
  testID = 'query-error',
}: {
  title?: string;
  body?: string;
  onRetry?: () => void;
  testID?: string;
}) {
  const { color } = useTheme();

  return (
    <Box p="xl" gap="md" center testID={testID}>
      <AppText variant="title" weight="600">
        {title}
      </AppText>
      <AppText tone="muted" style={{ textAlign: 'center' }}>
        {body}
      </AppText>
      {onRetry ? (
        <Pressable onPress={onRetry} accessibilityRole="button" hitSlop={10}>
          <AppText variant="body" style={{ color: color.accentInk }}>
            Try again
          </AppText>
        </Pressable>
      ) : null}
    </Box>
  );
}
