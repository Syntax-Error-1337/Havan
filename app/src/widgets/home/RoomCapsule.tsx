import { useState } from 'react';
import { Pressable, ScrollView, StyleSheet, View } from 'react-native';
import { LinearGradient } from 'expo-linear-gradient';

import { useTheme, tokens } from '../../app-shell/theme';
import { AppText } from '../../shared/ui';

/**
 * The room switcher: a floating glass capsule that says which part of the home you are looking at.
 *
 * Two reasons this replaced the category chips. Rooms are the hierarchy a person thinks in — the kitchen, the hall,
 * the landing — while "Sensor" and "Plug" are how the *platform* files things. And one switcher is enough: a screen
 * with a category rail and a room list is two answers to "where am I".
 *
 * Glass is correct here and wrong on the cards: this is the functional layer, it floats above content that scrolls
 * beneath it, and Apple's rule is exactly that — "use Liquid Glass for controls and navigation that float above the
 * content", never in the content layer (`materials.md › Liquid Glass`).
 */
export function RoomCapsule({
  rooms,
  value,
  onChange,
  total,
  onMore,
}: {
  rooms: { id: string; name: string }[];
  /** `null` = all rooms. */
  value: string | null;
  onChange: (id: string | null) => void;
  /** Unused by the pill labels; kept so a caller can pass the home's count without changing the call site later. */
  total?: number;
  /**
   * Hands the rest of the rooms to a picker.
   *
   * A home with twelve lines of production is not navigated by a horizontal scroller — a reviewer watching a
   * 1,200-device home called it "a cramped, endless scroller" and was right. Past six rooms the capsule keeps the
   * first five and offers the rest as a sheet: the cost of a picker is one tap, and the cost of a scroller is hunting.
   */
  onMore?: (rooms: { id: string; name: string }[]) => void;
}) {
  const { color, dark } = useTheme();
  // A capsule that is cut off with no cue reads as broken. Measured rather than guessed: five rooms at 390 pt do not
  // fit, and the first screenshot showed "Kitch…" with nothing to say there was more.
  const [overflowing, setOverflowing] = useState(false);
  const [scrolledToEnd, setScrolledToEnd] = useState(false);
  if (rooms.length < 2) return null;

  // Six fits a phone with the fade as the cue; more than that needs a picker, not a longer scroll.
  const shown = rooms.length > 6 && onMore ? rooms.slice(0, 5) : rooms;
  const hidden = rooms.length - shown.length;

  const pill = (label: string, selected: boolean, onPress: () => void, key: string) => (
    <Pressable
      key={key}
      onPress={onPress}
      accessibilityRole="tab"
      accessibilityState={{ selected }}
      accessibilityLabel={label}
      style={{
        minHeight: tokens.touch.min - 8,
        justifyContent: 'center',
        paddingHorizontal: tokens.space.md - 2,
        borderRadius: tokens.radius.pill,
        // The selected pill is a filled surface, not just a colour: colour alone is not a state.
        backgroundColor: selected ? (dark ? color.elevated : color.surface) : 'transparent',
        borderWidth: selected ? 0.5 : 0,
        borderColor: color.border,
      }}
    >
      <AppText
        variant="caption"
        weight={selected ? '600' : '400'}
        style={{ color: selected ? color.text : color.muted }}
      >
        {label}
      </AppText>
    </Pressable>
  );

  return (
    <View
      style={{
        flexDirection: 'row',
        alignItems: 'center',
        gap: 2,
        marginHorizontal: tokens.space.lg,
        padding: 4,
        borderRadius: tokens.radius.pill,
        backgroundColor: color.chrome,
        borderWidth: 0.5,
        borderColor: color.border,
      }}
    >
      <ScrollView
        horizontal
        showsHorizontalScrollIndicator={false}
        contentContainerStyle={{ gap: 2, alignItems: 'center', paddingRight: overflowing ? 18 : 0 }}
        onContentSizeChange={(w) => setOverflowing(w > 320)}
        onScroll={(e) => {
          const { contentOffset, contentSize, layoutMeasurement } = e.nativeEvent;
          setScrolledToEnd(contentOffset.x + layoutMeasurement.width > contentSize.width - 8);
        }}
        scrollEventThrottle={32}
      >
        {/* "All" rather than a count: the header already carries the home's totals, and a tab that reads
            "6 devices" beside "Kitchen" is a number in a list of places. */}
        {pill('All', value === null, () => onChange(null), 'all')}
        {shown.map((room) => pill(room.name, value === room.id, () => onChange(room.id), room.id))}
        {hidden > 0 && onMore ? pill(`+${hidden}`, false, () => onMore(rooms), 'more') : null}
      </ScrollView>

      {/* The cue, in the palette's own surface so it works in both appearances: content fades out under the edge
          that has more behind it, and the fade goes away once you have scrolled to the end. */}
      {overflowing && !scrolledToEnd ? (
        <LinearGradient
          pointerEvents="none"
          colors={['transparent', color.chrome]}
          start={{ x: 0, y: 0.5 }}
          end={{ x: 1, y: 0.5 }}
          style={{
            position: 'absolute',
            right: 0,
            top: 0,
            bottom: 0,
            width: 34,
            borderTopRightRadius: tokens.radius.pill,
            borderBottomRightRadius: tokens.radius.pill,
          }}
        />
      ) : null}
    </View>
  );
}
