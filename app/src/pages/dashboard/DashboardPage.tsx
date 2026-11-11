import { useEffect, useMemo, useState } from 'react';
import { Pressable, SectionList, TextInput } from 'react-native';
import { useRouter } from 'expo-router';
import { Box, Button, Screen, AppText } from '../../shared/ui';
import { DeviceCard, useDevices } from '../../entities/device';
import { useCreateHome, useCreateRoom, useHomes, useRooms } from '../../entities/home';
import { useSession } from '../../entities/session';
import { useHomeStream } from '../../features/live-updates/useHomeStream';
import { tokens } from '../../shared/config/tokens';
import { useTheme } from '../../app-shell/theme';
import { groupByRoom } from './sections';

/**
 * Home screen. Four honest states: no home yet, loading, empty, list. Errors are
 * the empty state with a different sentence — a user cannot act on a stack trace.
 */
export function DashboardPage() {
  const router = useRouter();
  const { color } = useTheme();

  const homeId = useSession((s) => s.homeId);
  const setHome = useSession((s) => s.setHome);
  const { data: homes, isPending: homesPending } = useHomes();
  const createHome = useCreateHome();

  // First run on a new account: adopt the only home there is.
  useEffect(() => {
    if (!homeId && homes && homes.length > 0) setHome(homes[0]!.id);
  }, [homeId, homes, setHome]);

  // Live state for whichever home is open.
  useHomeStream(homeId);

  const { data, isPending, refetch } = useDevices(homeId ?? undefined);
  const devices = useMemo(() => data ?? [], [data]);

  const { data: rooms } = useRooms(homeId ?? undefined);
  const createRoom = useCreateRoom(homeId ?? undefined);
  const [addingRoom, setAddingRoom] = useState(false);
  const [roomName, setRoomName] = useState('');

  const sections = useMemo(() => groupByRoom(devices, rooms), [devices, rooms]);

  if (homesPending) return <Screen>{null}</Screen>;

  if (!homes || homes.length === 0) {
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

  const home = homes.find((h) => h.id === homeId) ?? homes[0]!;

  return (
    <Screen>
      <Box px="lg" py="md" row style={{ justifyContent: 'space-between', alignItems: 'center' }}>
        <Box gap="xs">
          <AppText variant="display" weight="700">
            {home.name}
          </AppText>
          <AppText variant="caption" tone="muted">
            {home.device_count} device{home.device_count === 1 ? '' : 's'}
          </AppText>
        </Box>
        <Box row gap="sm" style={{ alignItems: 'center' }}>
          <Button
            label="Room"
            variant="ghost"
            onPress={() => setAddingRoom((v) => !v)}
          />
          <Button label="Automatic" variant="ghost" onPress={() => router.push('/automations')} />
          <Button label="Add" onPress={() => router.push('/add-device')} />
        </Box>
      </Box>

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
          <Button label="Try again" variant="ghost" onPress={() => refetch()} />
        </Box>
      ) : (
        <SectionList
          sections={sections}
          keyExtractor={(d) => d.id}
          contentContainerStyle={{
            gap: tokens.space.sm,
            paddingBottom: tokens.space.xxl,
            paddingTop: tokens.space.sm,
          }}
          stickySectionHeadersEnabled={false}
          renderSectionHeader={({ section }) => (
            <AppText
              variant="caption"
              tone="muted"
              style={{
                paddingHorizontal: tokens.space.lg,
                paddingTop: tokens.space.md,
                paddingBottom: tokens.space.xs,
                textTransform: 'uppercase',
                letterSpacing: 0.6,
              }}
            >
              {section.title}
            </AppText>
          )}
          renderItem={({ item }) => (
            <Box style={{ paddingHorizontal: tokens.space.lg }}>
              <DeviceCard device={item} onPress={() => router.push(`/device/${item.id}`)} />
            </Box>
          )}
        />
      )}
    </Screen>
  );
}
