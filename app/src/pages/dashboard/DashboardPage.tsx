import { useEffect, useMemo, useState } from 'react';
import { FlatList, Pressable, RefreshControl, ScrollView, StyleSheet, TextInput, View } from 'react-native';
import { useRouter } from 'expo-router';
import { useQueryClient } from '@tanstack/react-query';

import {
  Box,
  Button,
  Screen,
  AppText,
  HomeSkeleton,
  Icon,
  TabBar,
  QueryError,
  useActionSheet,
} from '../../shared/ui';
import { DeviceTile } from '../../entities/device';
import { summarise } from '../../entities/device/model/summary';
import {
  DEVICE_PAGE_MAX,
  useCreateHome,
  useCreateRoom,
  useDashboard,
  useDashboardTail,
  canManage,
  useHomes,
} from '../../entities/home';
import type { Dashboard } from '../../entities/home';
import { qk } from '../../shared/api/keys';
import { useMe, useSession } from '../../entities/session';
import { useHomeStream } from '../../features/live-updates/useHomeStream';
import { tokens } from '../../shared/config/tokens';
import { withAlpha } from '../../shared/ui/Glass';
import { ApiError } from '../../shared/api/client';
import { useTheme } from '../../app-shell/theme';
import { groupByRoom } from './sections';
import { buildRows } from './rows';
import { categoriesOf, filterDevices } from './search';
import { SceneStrip } from '../../widgets/scene-strip/SceneStrip';
import { HomeHeader, StatRail } from '../../widgets/home/HomeHeader';
import { HeroCard, pickHero } from '../../widgets/home/HeroCard';

/** The tab bar floats over the list, so the list has to end above it. */

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

  // Live state for whichever home is open. The status is shown, not swallowed: a screen that has
  // stopped hearing from the house must say so rather than keep showing what it last heard.
  const stream = useHomeStream(homeId);

  // A page at a time. The footer offers the rest: with a thousand devices the payload is ~3 MB, and
  // that should be a decision the person makes, not a cost every cold start pays.
  //
  // "More" is a list of cursors, not a growing `device_limit`. The server clamps a page at 500, so a
  // limit that keeps doubling stops moving after the second press — on a 1,200-device home the first
  // version re-fetched the same 500 devices and left the other 700 unreachable.
  const [tailCursors, setTailCursors] = useState<string[]>([]);
  const {
    data: dashboard,
    isPending,
    refetch,
    isRefetching,
    isError: dashboardFailed,
  } = useDashboard(homeId ?? null);
  const tails = useDashboardTail(homeId ?? null, tailCursors);

  // Seed the per-entity caches from the aggregate: the widgets below were written
  // against those keys, and this keeps one request for the whole screen.
  useEffect(() => {
    if (!dashboard || !homeId) return;
    qc.setQueryData(qk.devices(homeId), dashboard.devices);
    qc.setQueryData(qk.scenes(homeId), dashboard.scenes);
    qc.setQueryData(qk.unread, { unread: dashboard.unread_count });
    qc.setQueryData([...qk.home(homeId), 'rooms'], dashboard.rooms);
  }, [dashboard, homeId, qc]);

  // The first page, then every page the person asked for, deduped by id: a device that moves between
  // pages between two requests must not appear twice.
  const devices = useMemo(() => {
    const seen = new Set<string>();
    const all: NonNullable<typeof dashboard>['devices'] = [];
    for (const device of [
      ...(dashboard?.devices ?? []),
      ...tails.flatMap((tail: { data?: Dashboard }) => tail.data?.devices ?? []),
    ]) {
      if (seen.has(device.id)) continue;
      seen.add(device.id);
      all.push(device);
    }
    return all;
  }, [dashboard, tails]);

  // Where the next press starts, and how many it will bring. The last loaded page owns the answer:
  // the first page's cursor points at devices the tail has already fetched.
  const nextCursor = useMemo(() => {
    const last = tails.length > 0 ? tails[tails.length - 1] : undefined;
    return (last ? last.data?.devices_next_cursor : dashboard?.devices_next_cursor) ?? undefined;
  }, [tails, dashboard]);
  const rooms = dashboard?.rooms;
  const unread = dashboard?.unread_count ?? 0;
  const createRoom = useCreateRoom(homeId ?? undefined);
  const [addingRoom, setAddingRoom] = useState(false);
  const addSheet = useActionSheet();
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
  const home = homes?.find((h) => h.id === homeId);
  const homeName = dashboard?.home.name ?? home?.name ?? 'Home';
  // What this person is allowed to change. A member who is offered "Add a room" gets a 403 from an API
  // they cannot see and a sheet that did nothing — the permission is known before the button is drawn.
  const mayManage = Boolean(home && canManage(home.role));
  const person = me?.display_name || me?.email || 'You';
  // The count is the home's, not the page's: "171 of 200 online" on a 420-device home is a lie the
  // page size told.
  const totalDevices = dashboard?.devices_total ?? devices.length;
  // Presence moved into the header pill, so the line under the title describes the *house* rather than
  // repeating the count — and says when the list is a page of it, because a grid that stops at 200 of 1,200
  // with no word about it looks like the other 1,000 were deleted.
  const roomCount = (dashboard?.rooms ?? []).length;
  // A failed request has no counts, and "0 devices · 0 rooms" is the same lie as "No devices yet": the
  // em dash is this codebase's way of saying "not known yet", and it is also the honest answer here.
  const subtitleText = dashboardFailed
    ? '— devices · — rooms'
    : devices.length === totalDevices
      ? `${totalDevices.toLocaleString()} ${totalDevices === 1 ? 'device' : 'devices'} · ${roomCount} ${
          roomCount === 1 ? 'room' : 'rooms'
        }`
      : `${totalDevices.toLocaleString()} devices · showing ${devices.length.toLocaleString()}`;

  return (
    <Screen>
      <FlatList
        data={rows}
        keyExtractor={(row) => row.key}
        contentContainerStyle={{ paddingBottom: tokens.layout.tabBarClearance + tokens.space.xl }}
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
                subtitle={subtitleText}
                online={dashboard?.presence?.online ?? summary.online}
                total={dashboard?.presence?.total ?? totalDevices}
                // The pill's number is the home's, counted by the server: on a paged home the page's
                // own summary is a fact about our pagination.
                silent={dashboard?.presence?.silent ?? summary.silent}
                unknown={dashboardFailed}
                unread={unread}
                onOpenAlerts={() => router.push('/alerts')}
                onOpenHomes={() => router.push('/homes')}
              />

              {stream === 'reconnecting' ? (
                <Box row gap="xs" style={{ alignItems: 'center' }} testID="stream-reconnecting">
                  <Box
                    style={{
                      width: 6,
                      height: 6,
                      borderRadius: 3,
                      backgroundColor: color.warning,
                    }}
                  />
                  <AppText variant="tiny" tone="muted">
                    Reconnecting — showing what was last reported
                  </AppText>
                </Box>
              ) : null}

              {/* Searching is a question about the list. While it is being asked, the rail, the hero
                  and the scenes step aside: they are the home's state, not the answer, and leaving
                  them on screen pushes the result the person is looking for below the fold. */}
              {filtering ? null : (
                <>
                  {/* One sentence, once, above everything a stale reading could mislead: the rail's counts,
                      the hero's number and every tile below are the last thing the house said. Per-tile
                      qualifiers are noise at this size; the banner is what makes "0 on" and "Offline"
                      legible as facts about the network rather than about the bulbs. */}
                  {summary.silent > 0 ? (
                    <Box
                      row
                      gap="xs"
                      testID="silence-banner"
                      style={{ alignItems: 'center', paddingHorizontal: tokens.space.lg }}
                    >
                      <Box
                        style={{
                          width: 6,
                          height: 6,
                          borderRadius: 3,
                          backgroundColor: color.warning,
                        }}
                      />
                      <AppText variant="tiny" tone="muted">
                        {summary.silent === 1
                          ? 'This device is not reporting — showing what it last sent'
                          : 'Some devices are not reporting — showing what they last sent'}
                      </AppText>
                    </Box>
                  ) : null}

                  <StatRail
                    summary={summary}
                    rooms={sections.filter((s) => s.title !== 'Unassigned').length}
                    showTemperature={Boolean(
                      hero?.points.some(
                        (p) => p.capability === 'temperature' && typeof p.value === 'number',
                      ),
                    )}
                  />

                  {hero ? (
                    <HeroCard device={hero} onOpen={() => router.push(`/device/${hero.id}`)} />
                  ) : null}
                </>
              )}

              {/* The alerts pill moved into the header as a bell, because a status is not a peer of a
                  create action; and "New room" moved down into the list's own toolbar, where the rooms it
                  makes are. Three different jobs had been stacked as three identical pills. */}

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

              <ScrollView horizontal showsHorizontalScrollIndicator={false} contentContainerStyle={{ gap: 8 }}>
                {categories.length > 1 ? (
                  <>
                    <Chip label="All" selected={category === null} onPress={() => setCategory(null)} />
                    {categories.map((c) => (
                      <Chip
                        key={c}
                        label={c.charAt(0).toUpperCase() + c.slice(1)}
                        selected={category === c}
                        onPress={() => setCategory(category === c ? null : c)}
                      />
                    ))}
                  </>
                ) : null}
              </ScrollView>

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
                  {/* The chip that used to open this form also said "Cancel". Now that the form is reached
                      from the add sheet, it has to carry its own way out or the only exit is a saved room. */}
                  <Pressable
                    accessibilityRole="button"
                    accessibilityLabel="Cancel adding a room"
                    onPress={() => {
                      setRoomName('');
                      setAddingRoom(false);
                    }}
                    hitSlop={8}
                    style={{ paddingHorizontal: 8 }}
                  >
                    <AppText tone="muted">Cancel</AppText>
                  </Pressable>
                {createRoom.isError ? (
                  <AppText variant="tiny" style={{ color: color.danger }} testID="room-error">
                    {roomError(createRoom.error)}
                  </AppText>
                ) : null}
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
          ) : dashboardFailed ? (
            /* The same list is rendered for "this home is empty" and for "we could not ask", which is the
               one thing a home screen must never confuse: a customer with six devices was told "No devices
               yet — Tap Add and we will look for devices nearby" because a request failed on a train. The
               empty state invites re-pairing hardware that is already on the wall. */
            <QueryError
              title="We could not load this home"
              body="Your devices are still there — the app could not reach the hub. Nothing has changed."
              onRetry={() => refetch()}
              testID="home-unreachable"
            />
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
          nextCursor ? (
            <View style={{ paddingHorizontal: tokens.space.lg, paddingTop: tokens.space.md }}>
              <Pressable
                accessibilityRole="button"
                testID="load-all-devices"
                disabled={!nextCursor}
                onPress={() => {
                  if (!nextCursor) return;
                  setTailCursors((cursors) => [...cursors, nextCursor]);
                }}
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
                <AppText variant="caption" weight="600" style={{ color: color.accentInk }}>
                  Load {Math.min(DEVICE_PAGE_MAX, totalDevices - devices.length)} more
                </AppText>
                <AppText variant="tiny" tone="muted">
                  showing {devices.length} of {totalDevices}
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

      {addSheet.sheet}

      <TabBar
        active="home"
        onSelect={(key) => {
          if (key === 'automations') router.push('/automations');
          if (key === 'add') {
            addSheet.open({
              // The home's name, not "add something": a sheet that says where the thing will land is a
              // sheet you do not have to think about.
              title: `Add to ${homeName}`,
              items: [
                {
                  key: 'device',
                  label: 'Add a device',
                  hint: 'Scan for a nearby bulb, plug or sensor',
                  icon: 'radar',
                  onPress: () => router.push('/add-device'),
                },
                ...(mayManage
                  ? [
                      {
                        key: 'room',
                        label: 'Add a room',
                        hint: 'Group devices by where they are',
                        icon: 'home' as const,
                        onPress: () => setAddingRoom(true),
                      },
                    ]
                  : []),
                {
                  key: 'scene',
                  label: 'Add a scene',
                  hint: 'One tap that sets several devices',
                  icon: 'sparkle',
                  onPress: () => router.push('/scenes'),
                },
              ],
            });
          }
          if (key === 'settings') router.push('/settings');
        }}
      />
    </Screen>
  );
}

/**
 * The server's refusal, in the words of the thing that refused.
 *
 * `useCreateRoom` posts to an endpoint that answers 403 for a member; before this line the form simply
 * stopped working, which reads as a bug in the app rather than a rule of the house.
 */
function roomError(error: unknown): string {
  if (!(error instanceof ApiError)) return 'Could not reach the hub. Try again.';
  if (error.code === 'forbidden') return 'Only an owner or admin can add a room to this home.';
  if (error.code === 'conflict') return 'There is already a room with that name.';
  if (error.code === 'validation_failed') return error.message;
  return 'That room could not be created. Try again.';
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
        style={{ color: selected ? color.onAccent : urgent ? color.accentInk : color.muted }}
      >
        {label}
      </AppText>
    </Pressable>
  );
}
