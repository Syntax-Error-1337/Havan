import { useEffect, useRef } from 'react';
import { Animated, Easing } from 'react-native';
import { Box, AppText, Button, Screen } from '../../shared/ui';
import { useTheme, tokens } from '../../app-shell/theme';

/**
 * The radar. Phase 4 wires `react-native-ble-plx` here; the animation and the
 * state machine are already the ones the real scan drives, so the screen is not
 * rewritten when the radio arrives.
 */
export function RadarPage() {
  const { color } = useTheme();
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

  const scale = pulse.interpolate({ inputRange: [0, 1], outputRange: [0.6, 1.8] });
  const opacity = pulse.interpolate({ inputRange: [0, 1], outputRange: [0.55, 0] });

  return (
    <Screen>
      <Box flex={1} center gap="lg" p="xl">
        <Box center style={{ width: 220, height: 220 }}>
          <Animated.View
            style={{
              position: 'absolute',
              width: 220,
              height: 220,
              borderRadius: 110,
              borderWidth: 2,
              borderColor: color.accent,
              transform: [{ scale }],
              opacity,
            }}
          />
          <Box
            center
            style={{
              width: 88,
              height: 88,
              borderRadius: 44,
              backgroundColor: color.surface,
              borderWidth: 1,
              borderColor: color.border,
            }}
          >
            <AppText variant="display">+</AppText>
          </Box>
        </Box>

        <AppText variant="title" weight="600">
          Looking for devices
        </AppText>
        <AppText tone="muted" style={{ textAlign: 'center' }}>
          Keep the device powered on and within a metre of your phone.
        </AppText>

        <Box gap="sm" style={{ alignSelf: 'stretch', marginTop: tokens.space.xl }}>
          <Button label="Enter Wi-Fi manually" variant="ghost" full disabled />
          <Button label="Scan a QR code" variant="ghost" full disabled />
        </Box>
      </Box>
    </Screen>
  );
}
