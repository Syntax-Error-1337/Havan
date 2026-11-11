import { useMemo } from 'react';
import { FlatList } from 'react-native';
import { useRouter } from 'expo-router';
import { Box, Button, Screen, AppText } from '../../shared/ui';
import { DeviceCard, useDevices } from '../../entities/device';
import { tokens } from '../../shared/config/tokens';
import { useTheme } from '../../app/theme';

/**
 * Home screen. Three honest states: loading, empty, list. An error is the
 * empty state with a different sentence — the user cannot act on a stack trace.
 */
export function DashboardPage() {
  const router = useRouter();
  const { color } = useTheme();
  const homeId = process.env.EXPO_PUBLIC_HOME_ID ?? 'demo';
  const { data, isPending, refetch } = useDevices(homeId);

  const devices = useMemo(() => data ?? [], [data]);

  return (
    <Screen>
      <Box px="lg" py="md" row style={{ justifyContent: 'space-between', alignItems: 'center' }}>
        <AppText variant="display" weight="700">
          Home
        </AppText>
        <Button label="Add" onPress={() => router.push('/add-device')} />
      </Box>

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
        <FlatList
          data={devices}
          keyExtractor={(d) => d.id}
          numColumns={2}
          columnWrapperStyle={{ gap: tokens.space.md, paddingHorizontal: tokens.space.lg }}
          contentContainerStyle={{ gap: tokens.space.md, paddingBottom: tokens.space.xxl }}
          renderItem={({ item }) => (
            <DeviceCard device={item} onPress={() => router.push(`/device/${item.id}`)} />
          )}
        />
      )}
    </Screen>
  );
}
