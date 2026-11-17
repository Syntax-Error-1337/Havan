import { useEffect, useMemo, useState } from 'react';
import { FlatList, Pressable, RefreshControl, StyleSheet, TextInput, View } from 'react-native';
import { useRouter } from 'expo-router';
import { useQueryClient } from '@tanstack/react-query';

import { Box, Button, Screen, AppText, HomeSkeleton, TabBar } from '../../shared/ui';
import { DeviceTile } from '../../entities/device';
import { summarise } from '../../entities/device/model/summary';
import { useCreateHome, useCreateRoom, useDashboard, useHomes } from '../../entities/home';
import { qk } from '../../shared/api/keys';
import { useMe, useSession } from '../../entities/session';
import { useHomeStream } from '../../features/live-updates/useHomeStream';
import { tokens } from '../../shared/config/tokens';
import { useTheme } from '../../app-shell/theme';
import { groupByRoom } from './sections';
import { buildRows } from './rows';
import { SceneStrip } from '../../widgets/scene-strip/SceneStrip';
import { HomeHeader, StatRail } from '../../widgets/home/HomeHeader';
import { HeroCard, pickHero } from '../../widgets/home/HeroCard';

/** The tab bar floats over the list, so the list has to end above it. */
const TAB_BAR_CLEARANCE = 148;

/**
 * One row of the list.
 *
 * The screen is a virtualised list rather than a ScrollView full of tiles. A home with two hundred
 * devices is rare and a home with two thousand is not impossible, and the old shape mounted every
 * tile at once — a dashboard that is fine in a demo and a jank source for the customer who has been
 * with us for three years. Virtualising is invisible until it matters, which is the point.
 *
 * The pairing rule lives in `rows.ts` and is unit-tested: it is data, and it is the part that breaks
 * on an odd number of devices.
 */
/**
 * Home screen.
 *
 * One request paints it: `/homes/{id}/dashboard` returns the home, its rooms, its devices with their
 * points, its scenes and the unread count. The payload is then written into the caches the widgets
 * already read, so `DeviceTile` and `SceneStrip` keep working without a second round trip — and the
 * socket keeps patching them.
 *
 * The order on screen is the order of the questions a person asks walking in: what time is it and
 * whose home is this, what is the house doing (the rail), what is the one reading that matters (the
 * hero), and only then the rooms.
 */
export function DashboardPage() {
  const router = useRouter();
  const { color } = useTheme();
  const qc = useQueryClient();

  const homeId = useSession((s) => s.homeId);
  const setHome = useSession((s) => s.setHome);
  const { data: me } = useMe();
  const { data: homes, isPending: homesPending } = useHomes(Boolean(homeId));
  const createHome = useCreateHome();

  // First run on a new account: adopt the only home there is.
  useEffect(() => {
    if (!homeId && homes && homes.length > 0) setHome(homes[0]!.id);
  }, [homeId, homes, setHome]);

  // Live state for whichever home is open.
  useHomeStream(homeId);

  const { data: dashboard, isPending, refetch, isRefetching } = useDashboard(homeId ?? null);

  // Seed the per-entity caches from the aggregate: the widgets below were written
  // against those keys, and this keeps one request for the whole screen.
  useEffect(() => {
    if (!dashboard || !homeId) return;
    qc.setQueryData(qk.devices(homeId), dashboard.devices);
    qc.setQueryData(qk.scenes(homeId), dashboard.scenes);
    qc.setQueryData(qk.unread, { unread: dashboard.unread_count });
    qc.setQueryData([...qk.home(homeId), 'rooms'], dashboard.rooms);
  }, [dashboard, homeId, qc]);

  const devices = useMemo(() => dashboard?.devices ?? [], [dashboard]);
  const rooms = dashboard?.rooms;
  const unread = dashboard?.unread_count ?? 0;
  const createRoom = useCreateRoom(homeId ?? undefined);
  const [addingRoom, setAddingRoom] = useState(false);
  const [roomName, setRoomName] = useState('');

  const sections = useMemo(() => groupByRoom(devices, rooms), [devices, rooms]);
  const summary = useMemo(() => summarise(devices), [devices]);
  const hero = useMemo(() => pickHero(devices), [devices]);

  const rows = useMemo(() => buildRows(sections), [sections]);

  // A disabled query reports `isPending` forever, so the gate is "no home yet", not
  // "the homes list has not arrived" — with a stored home we never ask for it.
  if (!homeId && homesPending) return <Screen>{null}</Screen>;

  if (!homeId) {
    return (
      <Screen>
        <Box p="xl" gap="md" center flex={1}>
          <AppText variant="title" weight="600">
            Create your first home
          </AppText>
          <AppText tone="muted" style={{ textAlign: 'center' }}>
            A home holds your rooms and devices. You can add more later.
          </AppText>
          <Button
            label="Create home"
            loading={createHome.isPending}
            onPress={() =>
              createHome.mutate(
                { name: 'My home', timezone: 'UTC' },
                { onSuccess: (home) => setHome(home.id) },
              )
            }
          />
          {createHome.error ? (
            <AppText tone="danger" variant="caption">
              {createHome.error.message}
            </AppText>
          ) : null}
        </Box>
      </Screen>
    );
  }

  // The dashboard already carries the home; the list is only a fallback for the
  // moment before it lands.
  const homeName = dashboard?.home.name ?? homes?.find((h) => h.id === homeId)?.name ?? 'Home';
  const person = me?.display_name || me?.email || 'You';
  const clock = new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  const onlineText = `${summary.online} of ${summary.devices} online`;

  return (
    <Screen>
      <FlatList
        data={rows}
        keyExtractor={(row) => row.key}
        contentContainerStyle={{ paddingBottom: TAB_BAR_CLEARANCE + tokens.space.xl }}
        showsVerticalScrollIndicator={false}
        // Pull to refresh: the live socket keeps values current, but a person who has just plugged
        // something in wants to ask again by hand, and a screen with no gesture for that feels stuck.
        refreshControl={
          <RefreshControl
            refreshing={isRefetching && !isPending}
            onRefresh={() => void refetch()}
            tintColor={color.muted}
            colors={[color.accent]}
            progressBackgroundColor={color.surface}
          />
        }
        ListHeaderComponent={
          <>
            <Box px="lg" py="md" gap="lg">
              <HomeHeader name={homeName} person={person} subtitle={onlineText} clock={clock} />

              <StatRail
                summary={summary}
                rooms={sections.filter((s) => s.title !== 'Unassigned').length}
                showTemperature={Boolean(
                  hero?.points.some(
                    (p) => p.capability === 'temperature' && typeof p.value === 'number',
                  ),
                )}
              />

              {hero ? <HeroCard device={hero} /> : null}

              <View style={{ flexDirection: 'row', gap: 8 }}>
                <Chip
                  label={unread > 0 ? `Alerts · ${unread}` : 'Alerts'}
                  onPress={() => router.push('/alerts')}
                  urgent={unread > 0}
                />
                <Chip
                  label={addingRoom ? 'Cancel' : 'New room'}
                  onPress={() => setAddingRoom((v) => !v)}
                  selected={addingRoom}
                />
                <Chip label="Automations" onPress={() => router.push('/automations')} />
              </View>

              {addingRoom ? (
                <Box row gap="sm" style={{ alignItems: 'center' }}>
                  <TextInput
                    placeholder="Room name"
                    placeholderTextColor={color.muted}
                    value={roomName}
                    onChangeText={setRoomName}
                    autoFocus
                    style={{
                      flex: 1,
                      backgroundColor: color.glass,
                      borderWidth: StyleSheet.hairlineWidth,
                      borderColor: color.border,
                      borderRadius: tokens.radius.md,
                      padding: tokens.space.md,
                      color: color.text,
                    }}
                  />
                  <Button
                    label={createRoom.isPending ? 'Saving…' : 'Create'}
                    disabled={roomName.trim().length === 0 || createRoom.isPending}
                    onPress={() =>
                      createRoom.mutate(
                        { name: roomName.trim() },
                        {
                          onSuccess: () => {
                            setRoomName('');
                            setAddingRoom(false);
                          },
                        },
                      )
                    }
                  />
                </Box>
              ) : null}
            </Box>

            <Box style={{ marginTop: tokens.space.lg, marginBottom: tokens.space.lg }}>
              <SceneStrip homeId={homeId} scenes={dashboard?.scenes} pending={!dashboard} />
            </Box>
          </>
        }
        ListEmptyComponent={
          isPending ? (
            <HomeSkeleton />
          ) : (
            <Box p="xl" gap="md" center>
              <AppText variant="title" weight="600">
                No devices yet
              </AppText>
              <AppText tone="muted" style={{ textAlign: 'center' }}>
                Tap Add and we will look for devices nearby.
              </AppText>
              <Pressable onPress={() => refetch()} accessibilityRole="button">
                <AppText variant="body" tone="accent">
                  Try again
                </AppText>
              </Pressable>
            </Box>
          )
        }
        renderItem={({ item }) =>
          item.kind === 'room' ? (
            <View
              style={{
                flexDirection: 'row',
                alignItems: 'center',
                justifyContent: 'space-between',
                paddingHorizontal: tokens.space.lg,
                paddingTop: tokens.space.md,
                paddingBottom: tokens.space.sm,
              }}
            >
              <AppText variant="subhead" weight="600">
                {item.title}
              </AppText>
              <AppText variant="tiny" tone="muted">
                {item.online}/{item.total} online
              </AppText>
            </View>
          ) : (
            <View
              style={{
                flexDirection: 'row',
                gap: tokens.space.md,
                paddingHorizontal: tokens.space.lg,
                paddingBottom: tokens.space.md,
              }}
            >
              {item.devices.map((device) => (
                <View key={device.id} style={{ width: item.wide ? '100%' : '47%' }}>
                  <DeviceTile
                    device={device}
                    onPress={() => router.push(`/device/${device.id}`)}
                    wide={item.wide}
                  />
                </View>
              ))}
            </View>
          )
        }
        // A handful of rows fills the screen; the rest wait their turn instead of mounting.
        initialNumToRender={8}
        windowSize={7}
        removeClippedSubviews
      />

      <TabBar
        active="home"
        onSelect={(key) => {
          if (key === 'automations') router.push('/automations');
          if (key === 'add') router.push('/add-device');
          if (key === 'settings') router.push('/settings');
        }}
      />
    </Screen>
  );
}

/** One pill. Three of them, not five: a rail of chips is navigation, not a settings panel. */
function Chip({
  label,
  onPress,
  selected = false,
  urgent = false,
}: {
  label: string;
  onPress: () => void;
  selected?: boolean;
  urgent?: boolean;
}) {
  const { color } = useTheme();
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityState={{ selected }}
      onPress={onPress}
      style={{
        paddingHorizontal: 14,
        paddingVertical: 9,
        borderRadius: 999,
        backgroundColor: selected ? color.accent : color.glass,
        borderWidth: StyleSheet.hairlineWidth,
        borderColor: selected ? color.accent : urgent ? color.accent : color.border,
      }}
    >
      <AppText
        variant="caption"
        style={{ color: selected ? color.onAccent : urgent ? color.accent : color.muted }}
      >
        {label}
      </AppText>
    </Pressable>
  );
}
