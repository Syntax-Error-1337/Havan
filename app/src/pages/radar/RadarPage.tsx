import { useCallback, useEffect, useRef, useState } from 'react';
import { Animated, Easing, Pressable, TextInput } from 'react-native';
import { useQuery } from '@tanstack/react-query';

import { Box, AppText, Button, Screen } from '../../shared/ui';
import { useTheme, tokens } from '../../app-shell/theme';
import { request } from '../../shared/api/client';
import { serialTailMatches, signalBars, type NearbyDevice } from '../../shared/lib/ble';
import { useHomes } from '../../entities/home';
import { useRadar } from '../../features/pairing/useRadar';
import { usePairDevice } from '../../features/pairing/usePairDevice';

type PairedDevice = { id: string; name: string; serial: string; product_key: string };

/**
 * The radar.
 *
 * Three states, in order: nothing yet, here is what is out there, and now tell
 * this one your Wi-Fi password. The device claims itself once it is on the
 * network — the phone only watches for it to appear.
 */
export function RadarPage() {
  const { color } = useTheme();
  const { data: homes } = useHomes();
  const homeId = homes?.[0]?.id;

  const [selected, setSelected] = useState<NearbyDevice | null>(null);
  const [phase, setPhase] = useState<'scan' | 'wifi' | 'waiting' | 'done'>('scan');
  const [claimError, setClaimError] = useState<string | null>(null);

  const { state, devices } = useRadar(phase === 'scan');
  const pair = usePairDevice(homeId ?? '');

  const restart = useCallback(() => {
    setSelected(null);
    setClaimError(null);
    setPhase('scan');
  }, []);

  return (
    <Screen>
      <Box flex={1} p="lg" gap="lg">
        <Box gap="xs">
          <AppText variant="display">Add a device</AppText>
          <AppText variant="caption" tone="muted">
            {phase === 'scan'
              ? 'Keep the device close and powered on. It announces itself.'
              : phase === 'wifi'
                ? 'The device joins your network from here.'
                : 'Waiting for the device to appear.'}
          </AppText>
        </Box>

        {phase === 'scan' ? (
          <ScanView
            state={state}
            devices={devices}
            accent={color.accent}
            onPick={(device) => {
              setSelected(device);
              setPhase('wifi');
            }}
          />
        ) : null}

        {phase === 'wifi' && selected && homeId ? (
          <WifiForm
            device={selected}
            busy={pair.isPending}
            error={pair.error instanceof Error ? pair.error.message : null}
            onCancel={restart}
            onSubmit={async (ssid, password) => {
              setClaimError(null);
              try {
                await pair.mutateAsync({
                  device: selected,
                  ssid,
                  password,
                  endpoint: process.env.EXPO_PUBLIC_MQTT_URL ?? 'mqtts://mqtt.haven.dev:8883',
                  timezone: homes?.[0]?.timezone ?? 'UTC',
                });
                setPhase('waiting');
              } catch {
                // The mutation's error renders in the form; staying here lets
                // the user fix the password rather than start over.
              }
            }}
          />
        ) : null}

        {phase === 'waiting' && selected ? (
          <WaitingView
            tail={selected.serialTail}
            error={claimError}
            onAppeared={() => setPhase('done')}
            onFailed={setClaimError}
          />
        ) : null}

        {phase === 'done' && selected ? (
          <Box gap="md" center p="xl">
            <AppText variant="title">Added</AppText>
            <AppText tone="muted" style={{ textAlign: 'center' }}>
              {selected.name ?? 'Your device'} is on the network and in this home.
            </AppText>
            <Button label="Add another" onPress={restart} />
          </Box>
        ) : null}
      </Box>
    </Screen>
  );
}

function ScanView({
  state,
  devices,
  accent,
  onPick,
}: {
  state: 'idle' | 'scanning' | 'denied' | 'unsupported';
  devices: (NearbyDevice & { name?: string; category?: string })[];
  accent: string;
  onPick: (device: NearbyDevice) => void;
}) {
  const pulse = useRef(new Animated.Value(0)).current;

  useEffect(() => {
    const loop = Animated.loop(
      Animated.timing(pulse, {
        toValue: 1,
        duration: 1800,
        easing: Easing.out(Easing.ease),
        useNativeDriver: true,
      }),
    );
    loop.start();
    return () => loop.stop();
  }, [pulse]);

  if (state === 'unsupported') {
    return (
      <Box gap="sm" p="lg">
        <AppText variant="title">No radio here</AppText>
        <AppText tone="muted">
          This build cannot scan for Bluetooth devices. Pairing needs the phone app.
        </AppText>
      </Box>
    );
  }

  return (
    <Box flex={1} gap="md">
      <Box center style={{ height: 190 }}>
        <Animated.View
          style={{
            position: 'absolute',
            width: 190,
            height: 190,
            borderRadius: 95,
            borderWidth: 2,
            borderColor: accent,
            opacity: pulse.interpolate({ inputRange: [0, 1], outputRange: [0.5, 0] }),
            transform: [
              { scale: pulse.interpolate({ inputRange: [0, 1], outputRange: [0.5, 1.6] }) },
            ],
          }}
        />
        <Box
          center
          style={{
            width: 76,
            height: 76,
            borderRadius: 38,
            backgroundColor: tokens.color.surface,
            borderWidth: 1,
            borderColor: tokens.color.border,
          }}
        >
          <AppText variant="display">
            {devices.length > 0 ? String(devices.length) : '·'}
          </AppText>
        </Box>
      </Box>

      {devices.length === 0 ? (
        <AppText tone="muted" style={{ textAlign: 'center' }}>
          Nothing yet. Devices show up within a few seconds.
        </AppText>
      ) : null}

      <Box gap="sm">
        {devices.map((device) => (
          <Pressable
            key={`${device.productHash}:${device.serialTail}`}
            onPress={() => onPick(device)}
            accessibilityRole="button"
          >
            <Box
              row
              gap="md"
              p="md"
              radius="md"
              bg="surface"
              border
              style={{ alignItems: 'center' }}
            >
              <Box flex={1} gap="xs">
                <AppText>{device.name ?? 'Unrecognised device'}</AppText>
                <AppText variant="caption" tone="muted">
                  {device.name
                    ? `${device.serialTail.toUpperCase()} · ${device.state.replace('_', ' ')}`
                    : `unknown product ${device.productHash}`}
                </AppText>
              </Box>
              <AppText variant="caption" tone="muted">
                {'▮'.repeat(signalBars(device.rssi)) || '·'}
              </AppText>
            </Box>
          </Pressable>
        ))}
      </Box>
    </Box>
  );
}

function WifiForm({
  device,
  busy,
  error,
  onSubmit,
  onCancel,
}: {
  device: NearbyDevice;
  busy: boolean;
  error: string | null;
  onSubmit: (ssid: string, password: string) => Promise<void>;
  onCancel: () => void;
}) {
  const [ssid, setSsid] = useState('');
  const [password, setPassword] = useState('');

  const field = {
    backgroundColor: tokens.color.surface,
    borderWidth: 1,
    borderColor: tokens.color.border,
    borderRadius: tokens.radius.md,
    padding: tokens.space.lg,
    color: tokens.color.text,
    fontSize: tokens.font.body,
  } as const;

  return (
    <Box gap="md">
      <Box gap="xs">
        <AppText variant="title">{device.name ?? 'This device'}</AppText>
        <AppText variant="caption" tone="muted">
          Which network should it join?
        </AppText>
      </Box>

      <TextInput
        placeholder="Network name"
        placeholderTextColor={tokens.color.muted}
        autoCapitalize="none"
        autoCorrect={false}
        value={ssid}
        onChangeText={setSsid}
        style={field}
      />
      <TextInput
        placeholder="Password"
        placeholderTextColor={tokens.color.muted}
        secureTextEntry
        value={password}
        onChangeText={setPassword}
        style={field}
      />

      {error ? <AppText tone="danger" variant="caption">{error}</AppText> : null}

      <Button
        label={busy ? 'Sending…' : 'Send to device'}
        disabled={busy || ssid.length === 0}
        onPress={() => void onSubmit(ssid, password)}
      />
      <Button label="Back" variant="ghost" disabled={busy} onPress={onCancel} />
    </Box>
  );
}

/**
 * The device claims itself; the phone only watches for the result. It polls for
 * a minute and then says plainly that it did not arrive, rather than spinning
 * forever.
 */
function WaitingView({
  tail,
  error,
  onAppeared,
  onFailed,
}: {
  tail: string;
  error: string | null;
  onAppeared: () => void;
  onFailed: (message: string) => void;
}) {
  const { data: homes } = useHomes();
  const homeId = homes?.[0]?.id;
  const [seconds, setSeconds] = useState(0);

  const devices = useQuery({
    queryKey: ['pairing', 'wait', homeId, seconds],
    queryFn: () => request<PairedDevice[]>(`/devices?home_id=${homeId}`),
    enabled: Boolean(homeId) && seconds < 60,
    refetchInterval: 2000,
  });

  useEffect(() => {
    const timer = setInterval(() => setSeconds((n) => n + 1), 1000);
    return () => clearInterval(timer);
  }, []);

  useEffect(() => {
    if (devices.data?.some((d) => serialTailMatches(d.serial, tail))) {
      onAppeared();
    }
  }, [devices.data, tail, onAppeared]);

  useEffect(() => {
    if (seconds >= 60) {
      onFailed('The device did not come online. Check the password and try again.');
    }
  }, [seconds, onFailed]);

  return (
    <Box gap="md" center p="xl">
      <AppText variant="title">Joining the network</AppText>
      <AppText tone="muted" style={{ textAlign: 'center' }}>
        This usually takes under a minute. Leave the app open.
      </AppText>
      <AppText variant="caption" tone="muted">
        {Math.min(seconds, 60)}s
      </AppText>
      {error ? (
        <AppText tone="danger" variant="caption" style={{ textAlign: 'center' }}>
          {error}
        </AppText>
      ) : null}
    </Box>
  );
}
