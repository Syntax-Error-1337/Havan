import { useEffect, useMemo, useState } from 'react';
import { FlatList, Pressable, RefreshControl, ScrollView, StyleSheet, TextInput, View } from 'react-native';
import { useRouter } from 'expo-router';
import { useQueryClient } from '@tanstack/react-query';

import { Box, Button, Screen, AppText, HomeSkeleton, Icon, TabBar } from '../../shared/ui';
import { DeviceTile } from '../../entities/device';
import { summarise } from '../../entities/device/model/summary';
import {
  DEVICE_PAGE,
  useCreateHome,
  useCreateRoom,
  useDashboard,
  useHomes,
} from '../../entities/home';
import { qk } from '../../shared/api/keys';
import { useMe, useSession } from '../../entities/session';
import { useHomeStream } from '../../features/live-updates/useHomeStream';
import { tokens } from '../../shared/config/tokens';
import { withAlpha } from '../../shared/ui/Glass';
import { useTheme } from '../../app-shell/theme';
import { groupByRoom } from './sections';
import { buildRows } from './rows';
import { categoriesOf, filterDevices } from './search';
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

  // A page at a time. The footer offers the rest: with a thousand devices the payload is ~3 MB, and
  // that should be a decision the person makes, not a cost every cold start pays.
  const [deviceLimit, setDeviceLimit] = useState(DEVICE_PAGE);
  const { data: dashboard, isPending, refetch, isRefetching } = useDashboard(
    homeId ?? null,
    deviceLimit,
  );

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

  // Search is local: the dashboard already holds every device, so asking the network again would be
  // slower and would flicker the grid. The rail and the hero keep showing the *home* — a filter is a
  // question about the list, not a claim that the other devices stopped existing.
  const [query, setQuery] = useState('');
  const [category, setCategory] = useState<string | null>(null);
  const categories = useMemo(() => categoriesOf(devices), [devices]);
  const filtered = useMemo(() => filterDevices(devices, query, category), [devices, query, category]);
  const filtering = query.trim().length > 0 || category !== null;

  const sections = useMemo(() => groupByRoom(filtered, rooms), [filtered, rooms]);
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
  // The count is the home's, not the page's: "171 of 200 online" on a 420-device home is a lie the
  // page size told.
  const totalDevices = dashboard?.devices_total ?? devices.length;
  const onlineText =
    totalDevices === devices.length
      ? `${summary.online} of ${totalDevices} online`
      : `${summary.online} of ${totalDevices} online · showing ${devices.length}`;

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
              <HomeHeader
                name={homeName}
                person={person}
                subtitle={onlineText}
                clock={clock}
                onOpenHomes={() => router.push('/homes')}
              />

              {/* Searching is a question about the list. While it is being asked, the rail, the hero
                  and the scenes step aside: they are the home's state, not the answer, and leaving
                  them on screen pushes the result the person is looking for below the fold. */}
              {filtering ? null : (
                <>
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
                </>
              )}

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

              {/* Search first, filters under it: a house with two hundred devices is not navigated by
                  scrolling, and the words people type are the ones on the shelf — a name, a room, a
                  kind of thing. */}
              <View
                style={{
                  flexDirection: 'row',
                  alignItems: 'center',
                  gap: 8,
                  backgroundColor: color.glass,
                  borderWidth: StyleSheet.hairlineWidth,
                  borderColor: color.border,
                  borderRadius: 14,
                  paddingHorizontal: 12,
                }}
              >
                <Icon name="search" size={16} color={color.muted} />
                <TextInput
                  testID="device-search"
                  accessibilityLabel="Search devices"
                  placeholder="Search a device or room"
                  placeholderTextColor={color.muted}
                  value={query}
                  onChangeText={setQuery}
                  autoCorrect={false}
                  style={{ flex: 1, paddingVertical: 11, color: color.text, fontSize: 15 }}
                />
                {filtering ? (
                  <Pressable
                    accessibilityRole="button"
                    accessibilityLabel="Clear search"
                    onPress={() => {
                      setQuery('');
                      setCategory(null);
                    }}
                    hitSlop={10}
                  >
                    <Icon name="close" size={14} color={color.muted} />
                  </Pressable>
                ) : null}
              </View>

              {categories.length > 1 ? (
                <ScrollView horizontal showsHorizontalScrollIndicator={false} contentContainerStyle={{ gap: 8 }}>
                  <Chip label="All" selected={category === null} onPress={() => setCategory(null)} />
                  {categories.map((c) => (
                    <Chip
                      key={c}
                      label={c.charAt(0).toUpperCase() + c.slice(1)}
                      selected={category === c}
                      onPress={() => setCategory(category === c ? null : c)}
                    />
                  ))}
                </ScrollView>
              ) : null}

              {filtering ? (
                <AppText variant="tiny" tone="muted" testID="search-count">
                  {filtered.length} of {devices.length} device{devices.length === 1 ? '' : 's'}
                </AppText>
              ) : null}

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

            {filtering ? null : (
              <Box style={{ marginTop: tokens.space.lg, marginBottom: tokens.space.lg }}>
                <SceneStrip homeId={homeId} scenes={dashboard?.scenes} pending={!dashboard} />
              </Box>
            )}
          </>
        }
        ListEmptyComponent={
          isPending ? (
            <HomeSkeleton />
          ) : filtering ? (
            // "No devices" would be a lie: the house is full of them, the question was too narrow.
            <Box p="xl" gap="md" center>
              <AppText variant="title" weight="600">
                Nothing matches
              </AppText>
              <AppText tone="muted" style={{ textAlign: 'center' }}>
                {devices.length} device{devices.length === 1 ? '' : 's'} in this home. Try a room, a
                name, or a kind of device.
              </AppText>
              <Pressable
                accessibilityRole="button"
                onPress={() => {
                  setQuery('');
                  setCategory(null);
                }}
              >
                <AppText variant="body" tone="accent">
                  Clear the search
                </AppText>
              </Pressable>
            </Box>
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
        ListFooterComponent={
          dashboard?.devices_next_cursor ? (
            <View style={{ paddingHorizontal: tokens.space.lg, paddingTop: tokens.space.md }}>
              <Pressable
                accessibilityRole="button"
                testID="load-all-devices"
                onPress={() => setDeviceLimit((limit: number) => Math.min(limit * 4, 2000))}
                style={({ pressed }) => ({
                  flexDirection: 'row',
                  alignItems: 'center',
                  justifyContent: 'center',
                  gap: 8,
                  paddingVertical: 14,
                  borderRadius: 14,
                  borderWidth: 1,
                  // A muted outline read as a status line, not as the thing to press. It wears the
                  // accent now, and it says how many are behind it.
                  borderColor: color.accent,
                  backgroundColor: withAlpha(color.accent, pressed ? 0.24 : 0.14),
                })}
              >
                <AppText variant="caption" weight="600" style={{ color: color.accent }}>
                  Load{' '}
                  {(dashboard.devices_total ?? devices.length) - devices.length} more
                </AppText>
                <AppText variant="tiny" tone="muted">
                  showing {devices.length} of {dashboard.devices_total ?? devices.length}
                </AppText>
              </Pressable>
            </View>
          ) : (
            <View />
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
