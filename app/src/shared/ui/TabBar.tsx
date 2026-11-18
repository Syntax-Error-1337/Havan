import { Pressable, StyleSheet, View } from 'react-native';
import { LinearGradient } from 'expo-linear-gradient';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { useTheme, tokens } from '../../app-shell/theme';
// Direct imports, not the barrel: `index.ts` exports this file, so importing the barrel here closes
// a require cycle (Metro warns about it, and a cycle can hand back an uninitialised binding).
import { AppText } from './Box';
import { Icon, type IconName } from './Icon';
import { withAlpha } from './Glass';

export type TabKey = 'home' | 'automations' | 'add' | 'settings';

/**
 * The one piece of chrome that stays.
 *
 * Floated above the content rather than docked to it: a docked bar on a gradient canvas cuts the
 * screen into two rectangles, and the card language above it stops reading as one surface. The
 * centre action is raised because adding a device is the only thing a person opens this app to do
 * on day one; the other three are places to go afterwards.
 */
export function TabBar({
  active,
  onSelect,
}: {
  active: TabKey;
  onSelect: (key: TabKey) => void;
}) {
  const { color, dark } = useTheme();
  const insets = useSafeAreaInsets();

  const items: { key: TabKey; label: string; icon: IconName }[] = [
    { key: 'home', label: 'Home', icon: 'home' },
    { key: 'automations', label: 'Automations', icon: 'sparkle' },
    { key: 'add', label: 'Add', icon: 'plus' },
    { key: 'settings', label: 'Settings', icon: 'gear' },
  ];

  return (
    <>
    {/* A short fade above the bar: the content does scroll under it, and without this the eye reads
        a half-visible room header as a layout bug rather than as something behind glass. */}
    <LinearGradient
      pointerEvents="none"
      // Derived from the canvas, not from a literal: the scrim has to match whatever the mesh behind
      // it is doing, in whichever appearance the operator chose.
      colors={[
        withAlpha(color.bgBottom, 0),
        withAlpha(color.bgBottom, 0.55),
        withAlpha(color.bgBottom, 0.96),
      ]}
      locations={[0, 0.45, 1]}
      style={{ position: 'absolute', left: 0, right: 0, bottom: 0, height: 156 }}
    />
    <View
      style={{
        position: 'absolute',
        // On a phone this is inset by 16 and reads as a capsule. On a desktop it would stretch into a dock across
        // 1 000 points, which is a phone pattern wearing a desktop's width — so it keeps a capsule's width and
        // centres instead.
        alignSelf: 'center',
        width: '100%',
        maxWidth: 520,
        bottom: Math.max(insets.bottom, tokens.space.md),
        borderRadius: 28,
        paddingVertical: 10,
        paddingHorizontal: 8,
        flexDirection: 'row',
        alignItems: 'center',
        justifyContent: 'space-between',
        backgroundColor: color.chrome,
        borderWidth: StyleSheet.hairlineWidth,
        borderColor: color.border,
        // The one shadow in the app: the bar has to separate from the content scrolling under it,
        // and a border alone disappears over a bright card.
        shadowColor: color.shadow,
        // Light needs more shadow than 0.12 to read as floating: on near-white, a 12 % shadow is a rumour.
        shadowOpacity: dark ? 0.45 : 0.18,
        shadowRadius: 18,
        shadowOffset: { width: 0, height: 8 },
        elevation: 12,
      }}
    >
      {items.map((item) => {
        const selected = active === item.key;
        const isAdd = item.key === 'add';

        return (
          <Pressable
            key={item.key}
            accessibilityRole="button"
            accessibilityLabel={item.label}
            accessibilityState={{ selected }}
            onPress={() => onSelect(item.key)}
            style={({ pressed }) => ({
              flex: 1,
              alignItems: 'center',
              justifyContent: 'center',
              gap: 4,
              paddingVertical: 4,
              opacity: pressed ? 0.7 : 1,
            })}
          >
            {isAdd ? (
              <View
                style={{
                  width: 42,
                  height: 42,
                  borderRadius: 21,
                  alignItems: 'center',
                  justifyContent: 'center',
                  backgroundColor: color.accent,
                }}
              >
                <Icon name="plus" size={22} color={color.onAccent} />
              </View>
            ) : (
              <>
                {/* 0.55 alpha on the bar measured 3.0:1 in daylight — an icon you can see but not
                    identify. 0.64 clears 4.5:1 and still reads as inactive. */}
                <Icon
                  name={item.icon}
                  size={21}
                  color={selected ? color.accent : withAlpha(color.text, 0.64)}
                />
                <AppText
                  variant="tiny"
                  style={{ color: selected ? color.accent : withAlpha(color.text, 0.64) }}
                >
                  {item.label}
                </AppText>
              </>
            )}
          </Pressable>
        );
      })}
    </View>
    </>
  );
}
