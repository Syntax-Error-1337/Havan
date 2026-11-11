import { Pressable, ScrollView } from 'react-native';

import { Box, AppText, Button, Screen } from '../../shared/ui';
import { tokens } from '../../app-shell/theme';
import {
  ago,
  useMarkAllRead,
  useMarkRead,
  useNotifications,
  type Notification,
} from '../../entities/notification';

/**
 * Alerts.
 *
 * What the automations have been telling a table nobody read. Unread alerts are
 * the only thing that looks different, because that is the only thing that
 * matters: a list where everything is styled equally is a list nobody scans.
 */
export function AlertsPage() {
  const { data: alerts, isPending, isError } = useNotifications();
  const markRead = useMarkRead();
  const markAll = useMarkAllRead();

  const unread = (alerts ?? []).filter((a) => !a.read_at).length;

  return (
    <Screen>
      <ScrollView contentContainerStyle={{ padding: tokens.space.lg, gap: tokens.space.md }}>
        <Box row style={{ justifyContent: 'space-between', alignItems: 'flex-end' }}>
          <Box gap="xs">
            <AppText variant="display">Alerts</AppText>
            <AppText variant="caption" tone="muted">
              {unread === 0 ? 'Nothing new' : `${unread} unread`}
            </AppText>
          </Box>
          {unread > 0 ? (
            <Button
              label="Mark all read"
              variant="ghost"
              loading={markAll.isPending}
              onPress={() => markAll.mutate()}
            />
          ) : null}
        </Box>

        {isPending ? <AppText tone="muted">Loading…</AppText> : null}

        {isError ? (
          <AppText tone="muted">Alerts are unavailable right now.</AppText>
        ) : null}

        {!isPending && !isError && (alerts ?? []).length === 0 ? (
          <Box gap="sm" p="lg" radius="md" bg="surface" border>
            <AppText variant="title">Nothing yet</AppText>
            <AppText tone="muted">
              Automations write here when they have something to say. Give one a
              notify action and it will show up.
            </AppText>
          </Box>
        ) : null}

        {(alerts ?? []).map((alert) => (
          <Alert key={alert.id} alert={alert} onRead={() => markRead.mutate(alert.id)} />
        ))}
      </ScrollView>
    </Screen>
  );
}

function Alert({ alert, onRead }: { alert: Notification; onRead: () => void }) {
  const unread = !alert.read_at;

  return (
    <Pressable
      onPress={unread ? onRead : undefined}
      accessibilityRole="button"
      accessibilityState={{ selected: unread }}
      disabled={!unread}
    >
      <Box row gap="md" p="md" radius="md" bg={unread ? 'elevated' : 'surface'} border>
        {/* A dot, not a colour change alone: colour is the first thing a
            colour-blind user loses. */}
        <Box style={{ width: 8, paddingTop: 6 }}>
          {unread ? (
            <Box
              style={{
                width: 8,
                height: 8,
                borderRadius: 4,
                backgroundColor: tokens.color.accent,
              }}
            />
          ) : null}
        </Box>

        <Box flex={1} gap="xs">
          <Box row style={{ justifyContent: 'space-between' }}>
            <AppText weight={unread ? '600' : '400'}>{alert.title}</AppText>
            <AppText variant="caption" tone="muted">
              {ago(alert.created_at)}
            </AppText>
          </Box>
          {alert.body ? (
            <AppText variant="caption" tone="muted">
              {alert.body}
            </AppText>
          ) : null}
        </Box>
      </Box>
    </Pressable>
  );
}
