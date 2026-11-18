import { ScrollView, View } from 'react-native';
import { useRouter } from 'expo-router';

import {
  Box,
  AppText,
  Button,
  Glass,
  Icon,
  ListSkeleton,
  QueryError,
  Screen,
  ScreenHeader,
  ScreenScroll,
} from '../../shared/ui';
import { withAlpha } from '../../shared/ui/Glass';
import { signalAccent } from '../../shared/config/tokens';
import { useTheme, tokens } from '../../app-shell/theme';
import {
  clockTime,
  dayBucket,
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
  const { data: alerts, isPending, isError, isFetching, refetch } = useNotifications();
  const markRead = useMarkRead();
  const markAll = useMarkAllRead();

  const unread = (alerts ?? []).filter((a) => !a.read_at).length;

  return (
    <Screen>
      {/* The screen had no way back: it is pushed from the bell, and the only exit was the browser's or the
          platform's own gesture. `ScreenHeader` is the one place that knows what "back" means when a screen
          was opened by a link and there is nothing behind it. */}
      <ScreenHeader
        title="Alerts"
        right={
          unread > 0 ? (
            <Button
              label="Mark all read"
              variant="ghost"
              loading={markAll.isPending}
              onPress={() => markAll.mutate()}
            />
          ) : undefined
        }
      />

      {/* Pull to refresh: the one gesture an iOS customer tries first, and it did nothing on this screen. */}
      <ScreenScroll
        padded={false}
        gap={tokens.space.md}
        onRefresh={() => void refetch()}
        refreshing={isFetching && !isPending}
        contentContainerStyle={{ paddingHorizontal: tokens.space.lg }}
      >
        {/* "Nothing new" is a claim about the alerts, and it can only be made once they have been counted.
            It also appeared *while they were still loading* — the feed said "Nothing new" above three
            skeletons, which is the screen contradicting itself in its first second. */}
        {isPending ? null : (
          <AppText variant="caption" tone="muted">
            {isError ? '— unread' : unread === 0 ? 'Nothing new' : `${unread} unread`}
          </AppText>
        )}

        {isPending ? <ListSkeleton rows={3} rowHeight={104} showHeader={false} /> : null}

        {isError ? (
          <QueryError
            title="We could not load your alerts"
            body="Nothing has changed — the app could not reach the hub."
            onRetry={() => void refetch()}
            testID="alerts-unreachable"
          />
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

        {/* Grouped by day. A feed that runs Today → yesterday → last Tuesday in one flat column is a
            column you scroll without reading; a heading costs a line and makes it a record. */}
        {(alerts ?? []).map((alert, index, all) => {
          const bucket = dayBucket(alert.created_at);
          const first = index === 0 || dayBucket(all[index - 1]!.created_at) !== bucket;
          return (
            <Box key={alert.id} gap="sm">
              {first ? (
                <AppText
                  variant="tiny"
                  tone="muted"
                  style={{ letterSpacing: 1.1, marginTop: index === 0 ? 0 : tokens.space.sm }}
                >
                  {bucket.toUpperCase()}
                </AppText>
              ) : null}
              <Alert alert={alert} onRead={() => markRead.mutate(alert.id)} />
            </Box>
          );
        })}
      </ScreenScroll>
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
      // Read alerts stay pressable: "read" is not "done", and the device is still there. But they step
      // back: a feed where read and unread look the same is a feed you cannot triage, and a 3-pixel dot
      // is not enough to carry that on its own.
      style={unread ? undefined : { opacity: 0.72 }}
      disabled={!deviceId && !unread}
      // Read state said only by a dot and a dimmer card, which a screen reader never hears.
      accessibilityLabel={`${alert.title}, ${unread ? 'unread' : 'read'}${
        deviceId ? `, opens ${deviceName ?? 'the device'}` : ''
      }`}
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
              {clockTime(alert.created_at)}
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
          ) : unread ? (
            // An alert with no device behind it still does something when tapped. Saying so is the
            // difference between a card that looks interactive and a card that is.
            <AppText variant="tiny" tone="muted" numberOfLines={1}>
              Mark as read
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
  automation: { icon: 'sparkle', accent: signalAccent.automation },
  ota: { icon: 'lightning', accent: signalAccent.ota },
  security: { icon: 'lock', accent: signalAccent.security },
  alert: { icon: 'alert', accent: signalAccent.alert },
};
