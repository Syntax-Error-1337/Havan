import { useCallback, useEffect, useRef, useState } from 'react';
import { Animated, Easing, Platform, Pressable, ScrollView, TextInput, View } from 'react-native';
import { useQuery } from '@tanstack/react-query';
import { useRouter } from 'expo-router';

import { Box, AppText, Button, Glass, Icon, Screen } from '../../shared/ui';
import { useTheme, tokens } from '../../app-shell/theme';
import { withAlpha } from '../../shared/config/tokens';
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

        {phase === 'scan' && state === 'unsupported' ? (
          <PhoneHandoff />
        ) : phase === 'scan' ? (
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

/**
 * The browser cannot scan for Bluetooth, so this screen is a handoff rather than a failure.
 *
 * It used to say "No radio here — this build cannot scan for Bluetooth devices", which is a developer's
 * sentence about a limitation the customer cannot act on, followed by empty space. A first-run screen that
 * stops dead is the worst screen in the app, because it is the one the customer sees first.
 *
 * So: what is happening, the three things to do about it, the link to carry over, and a way out. The primary
 * action copies `haven://add-device` — a deep link the phone app already answers — because the honest next
 * step from a laptop is "open this on your phone", not "try again".
 */
export function PhoneHandoff() {
  const { color } = useTheme();
  const router = useRouter();
  const [copied, setCopied] = useState(false);
  const canCopy = Platform.OS === 'web' && typeof navigator !== 'undefined' && Boolean(navigator.clipboard);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText('haven://add-device');
      setCopied(true);
    } catch {
      // A browser that refuses the clipboard still shows the link below, so the person can type it.
      setCopied(false);
    }
  };

  const steps = [
    'Open Haven on the phone that will pair the device.',
    'Keep the device powered on and close to that phone.',
    'Tap Add a device there — it will appear within a few seconds.',
  ];

  // Scrolls, because a handoff is four blocks of text on a screen that might be 667 points tall with the
  // customer's text size turned up. A dead end that is also cut off is worse than the dead end it replaced.
  return (
    <ScrollView contentContainerStyle={{ gap: 20, paddingBottom: 24 }} showsVerticalScrollIndicator={false}>
      <Glass style={{ padding: 16, alignItems: 'center', gap: 8 }} accent={color.accent}>
        <Box
          center
          style={{
            width: 54,
            height: 54,
            borderRadius: 27,
            backgroundColor: withAlpha(color.accent, 0.16),
          }}
        >
          <Icon name="phone" size={28} color={color.accent} />
        </Box>
        <AppText variant="title" weight="700" style={{ textAlign: 'center' }}>
          Pair from your phone
        </AppText>
        <AppText variant="caption" tone="muted" style={{ textAlign: 'center' }}>
          A browser cannot scan for Bluetooth, so pairing happens in the app. Everything else works here.
        </AppText>
      </Glass>

      <Box gap="sm">
        {steps.map((step, index) => (
          <Box key={step} row gap="md" style={{ alignItems: 'flex-start' }}>
            <Box
              center
              style={{
                width: 22,
                height: 22,
                borderRadius: 11,
                backgroundColor: withAlpha(color.accent, 0.2),
                marginTop: 1,
              }}
            >
              <AppText variant="tiny" weight="700" style={{ color: color.accent }}>
                {index + 1}
              </AppText>
            </Box>
            <AppText variant="caption" style={{ flex: 1, color: color.text }}>
              {step}
            </AppText>
          </Box>
        ))}
      </Box>

      <Box gap="xs">
        {canCopy ? (
          <Button label={copied ? 'Link copied' : 'Copy the phone link'} onPress={copy} />
        ) : null}
        <AppText variant="tiny" tone="muted" style={{ textAlign: 'center' }}>
          {copied
            ? 'Open it on your phone to finish pairing.'
            : 'Copies a link you can send to yourself and open on the phone.'}
        </AppText>
      </Box>

      <Pressable
        accessibilityRole="button"
        accessibilityLabel="Back to your home"
        onPress={() => router.replace('/')}
        style={{ alignItems: 'center', paddingVertical: 10 }}
      >
        <AppText weight="600" tone="muted">
          Back to your home
        </AppText>
      </Pressable>
    </ScrollView>
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
  const [reveal, setReveal] = useState(false);

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

      <Box gap="xs">
        <AppText variant="tiny" tone="muted" style={{ letterSpacing: 1.1 }}>
          NETWORK NAME
        </AppText>
        <TextInput
          placeholder="The name your phone shows"
          placeholderTextColor={tokens.color.muted}
          autoCapitalize="none"
          autoCorrect={false}
          autoComplete="off"
          accessibilityLabel="Network name"
          value={ssid}
          onChangeText={setSsid}
          style={field}
        />
        {/* The most common reason a first pairing fails, said before it happens: these devices are 2.4 GHz
            only, and a house with "Home" and "Home_5G" looks like one network until the device never
            appears. */}
        <AppText variant="tiny" tone="muted">
          Most smart devices join 2.4 GHz only. If you can see two names, pick the one without “5G”.
        </AppText>
      </Box>

      <Box gap="xs">
        <AppText variant="tiny" tone="muted" style={{ letterSpacing: 1.1 }}>
          WI-FI PASSWORD
        </AppText>
        <View style={{ justifyContent: 'center' }}>
          <TextInput
            placeholder="The password for that network"
            placeholderTextColor={tokens.color.muted}
            secureTextEntry={!reveal}
            autoCapitalize="none"
            autoCorrect={false}
            autoComplete="off"
            accessibilityLabel="Wi-Fi password"
            value={password}
            onChangeText={setPassword}
            style={field}
          />
          {/* A wrong Wi-Fi password is the other half of "the device never appeared", and it is typed on a
              phone keyboard with no other way to check it. */}
          <Pressable
            onPress={() => setReveal((v) => !v)}
            accessibilityRole="button"
            accessibilityLabel={reveal ? 'Hide Wi-Fi password' : 'Show Wi-Fi password'}
            hitSlop={10}
            style={{ position: 'absolute', right: 14 }}
          >
            <AppText variant="tiny" tone="muted">
              {reveal ? 'HIDE' : 'SHOW'}
            </AppText>
          </Pressable>
        </View>
      </Box>

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
