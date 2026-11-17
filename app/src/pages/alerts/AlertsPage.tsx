import { ScrollView, View } from 'react-native';
import { useRouter } from 'expo-router';

import { Box, AppText, Button, Glass, Icon, Screen } from '../../shared/ui';
import { withAlpha } from '../../shared/ui/Glass';
import { useTheme, tokens } from '../../app-shell/theme';
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
  const router = useRouter();
  const { color, dark } = useTheme();
  const unread = !alert.read_at;

  // An alert that names a device can be opened on that device. The automation's trigger writes the
  // id into `data`; older rows carry `{}` and simply do not offer the jump.
  const deviceId = typeof alert.data?.device_id === 'string' ? alert.data.device_id : null;
  const deviceName = typeof alert.data?.device_name === 'string' ? alert.data.device_name : null;

  const open = () => {
    if (unread) onRead();
    if (deviceId) router.push(`/device/${deviceId}`);
  };

  const kind = KIND[alert.kind] ?? KIND.alert!;

  return (
    <Glass
      onPress={open}
      accent={unread ? color.accent : undefined}
      // Read alerts stay pressable: "read" is not "done", and the device is still there.
      disabled={!deviceId && !unread}
      testID={`alert-${alert.id}`}
    >
      <Box row gap="md" style={{ alignItems: 'flex-start' }}>
        <View
          style={{
            width: 34,
            height: 34,
            borderRadius: 17,
            alignItems: 'center',
            justifyContent: 'center',
            backgroundColor: withAlpha(kind.accent, dark ? 0.22 : 0.14),
          }}
        >
          <Icon name={kind.icon} size={17} color={kind.accent} />
        </View>

        <Box flex={1} gap="xs">
          <Box row style={{ justifyContent: 'space-between', alignItems: 'center', gap: 8 }}>
            <AppText weight={unread ? '600' : '400'} style={{ flexShrink: 1 }}>
              {alert.title}
            </AppText>
            <AppText variant="caption" tone="muted">
              {ago(alert.created_at)}
            </AppText>
          </Box>
          {alert.body ? (
            <AppText variant="caption" tone="muted">
              {alert.body}
            </AppText>
          ) : null}
          {deviceId ? (
            <AppText variant="tiny" style={{ color: color.accent }} numberOfLines={1}>
              Open {deviceName ?? 'the device'}
            </AppText>
          ) : null}
        </Box>

        {unread ? (
          // A dot, not a colour change alone: colour is the first thing a colour-blind user loses.
          <View style={{ width: 8, height: 8, borderRadius: 4, backgroundColor: color.accent, marginTop: 6 }} />
        ) : null}
      </Box>
    </Glass>
  );
}

/** What kind of thing interrupted you, said in a colour and a glyph rather than in a word. */
const KIND: Record<string, { icon: 'sparkle' | 'lightning' | 'lock' | 'alert'; accent: string }> = {
  automation: { icon: 'sparkle', accent: '#8B5CF6' },
  ota: { icon: 'lightning', accent: '#0EA5E9' },
  security: { icon: 'lock', accent: '#F472B6' },
  alert: { icon: 'alert', accent: '#FB923C' },
};
