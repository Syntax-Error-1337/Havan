import { useEffect, useMemo, useState } from 'react';
import { Pressable, ScrollView, TextInput } from 'react-native';
import { useRouter } from 'expo-router';
import { useQueryClient } from '@tanstack/react-query';

import { Box, Button, Screen, AppText, Section } from '../../shared/ui';
import { DeviceCard } from '../../entities/device';
import { useCreateHome, useCreateRoom, useDashboard, useHomes } from '../../entities/home';
import { qk } from '../../shared/api/keys';
import { useSession } from '../../entities/session';
import { useHomeStream } from '../../features/live-updates/useHomeStream';
import { tokens } from '../../shared/config/tokens';
import { useTheme } from '../../app-shell/theme';
import { groupByRoom } from './sections';
import { SceneStrip } from '../../widgets/scene-strip/SceneStrip';

/**
 * Home screen.
 *
 * One request paints it: `/homes/{id}/dashboard` returns the home, its rooms, its
 * devices with their points, its scenes and the unread count. The payload is then
 * written into the caches the widgets already read, so `DeviceCard` and `SceneStrip`
 * keep working without a second round trip — and the socket keeps patching them.
 *
 * Four honest states: no home yet, loading, empty, list. Errors are the empty state
 * with a different sentence — a user cannot act on a stack trace.
 */
export function DashboardPage() {
  const router = useRouter();
  const { color } = useTheme();
  const qc = useQueryClient();

  const homeId = useSession((s) => s.homeId);
  const setHome = useSession((s) => s.setHome);
  const { data: homes, isPending: homesPending } = useHomes(Boolean(homeId));
  const createHome = useCreateHome();

  // First run on a new account: adopt the only home there is.
  useEffect(() => {
    if (!homeId && homes && homes.length > 0) setHome(homes[0]!.id);
  }, [homeId, homes, setHome]);

  // Live state for whichever home is open.
  useHomeStream(homeId);

  const { data: dashboard, isPending, refetch } = useDashboard(homeId ?? null);

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

  return (
    <Screen>
      <Box px="lg" py="md" row style={{ justifyContent: 'space-between', alignItems: 'center' }}>
        <Box gap="xs">
          <AppText variant="display" weight="700">
            {homeName}
          </AppText>
          <AppText variant="caption" tone="muted">
            {devices.length} device{devices.length === 1 ? '' : 's'}
          </AppText>
        </Box>
        <Box row gap="sm" style={{ alignItems: 'center' }}>
          <Button
            label={unread > 0 ? `Alerts (${unread})` : 'Alerts'}
            variant="ghost"
            onPress={() => router.push('/alerts')}
          />
          <Button label="Room" variant="ghost" onPress={() => setAddingRoom((v) => !v)} />
          <Button
            label="Automatic"
            variant="ghost"
            onPress={() => router.push('/automations')}
          />
          <Button label="Settings" variant="ghost" onPress={() => router.push('/settings')} />
          <Button label="Add" onPress={() => router.push('/add-device')} />
        </Box>
      </Box>

      {homeId ? (
        <SceneStrip homeId={homeId} scenes={dashboard?.scenes} pending={!dashboard} />
      ) : null}

      {addingRoom ? (
        <Box row gap="sm" px="lg" style={{ alignItems: 'center' }}>
          <TextInput
            placeholder="Room name"
            placeholderTextColor={color.muted}
            value={roomName}
            onChangeText={setRoomName}
            autoFocus
            style={{
              flex: 1,
              backgroundColor: color.surface,
              borderWidth: 1,
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

      {isPending ? (
        <Box p="lg" gap="md">
          {[0, 1, 2].map((i) => (
            <Box
              key={i}
              style={{ height: 96, borderRadius: tokens.radius.lg, backgroundColor: color.surface }}
            />
          ))}
        </Box>
      ) : devices.length === 0 ? (
        <Box p="xl" gap="md" center style={{ flex: 1 }}>
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
      ) : (
        <ScrollView
          contentContainerStyle={{ paddingTop: tokens.space.sm, paddingBottom: tokens.space.xxl }}
        >
          {sections.map((section) => (
            <Section key={section.title} title={section.title}>
              {section.data.map((device) => (
                <DeviceCard
                  key={device.id}
                  device={device}
                  onPress={() => router.push(`/device/${device.id}`)}
                />
              ))}
            </Section>
          ))}
        </ScrollView>
      )}
    </Screen>
  );
}
