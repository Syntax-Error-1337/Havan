import { Pressable } from 'react-native';

import { useTheme } from '../../app-shell/theme';
import { Box, AppText } from './Box';

/**
 * A small chip picker.
 *
 * Extracted from the automations screen when scenes needed the same thing: two
 * copies of a control is how two slightly different controls happen.
 */
export function ChipPicker({
  label,
  value,
  options,
  onChange,
  emptyLabel = 'Nothing to choose from',
  columns,
}: {
  /** Visible heading for the row. Optional: a caller that already draws its own heading passes nothing. */
  label?: string;
  value: string | null;
  options: { id: string; label: string }[];
  onChange: (id: string) => void;
  emptyLabel?: string;
  /**
   * How many equal-width columns to lay the chips out in.
   *
   * Left unset, chips are as wide as their labels and wrap — right for `On | Off` and the appearance control,
   * where three short words should not be stretched. A picker of device *names* is different: ragged widths in
   * two columns read as an unfinished layout, which is exactly what a reviewer said about the scene editor.
   */
  columns?: number;
}) {
  const { color } = useTheme();
  // `flexBasis`, and less than an exact share: two 50 % cells plus an 8-point gap is more than 100 %, so the
  // first attempt stacked every chip in a single column and looked like a list with the wrong width.
  const grid =
    columns && columns > 1 ? { flexBasis: `${100 / columns - 2}%` as const, flexGrow: 0, flexShrink: 1 } : undefined;

  return (
    <Box gap="xs">
      {label ? (
        <AppText variant="caption" tone="muted">
          {label}
        </AppText>
      ) : null}
      <Box row gap="sm" style={{ flexWrap: 'wrap' }}>
        {options.length === 0 ? (
          <AppText variant="caption" tone="muted">
            {emptyLabel}
          </AppText>
        ) : null}
        {options.map((option) => {
          const selected = value === option.id;
          return (
            <Pressable
              key={option.id}
              onPress={() => onChange(option.id)}
              accessibilityRole="button"
              accessibilityState={{ selected }}
              accessibilityLabel={label ? `${label}: ${option.label}` : option.label}
              // In a grid the cell carries the width; the chip fills it.
              style={grid}
            >
              {/* An unselected chip said its label in muted grey on a surface that is nearly the same grey: the
                  choice was there and unreadable. `elevated` is a step up from the card, and the label keeps the
                  text tone — selection is carried by the fill, not by hiding the other options. */}
              {/* 44 tall, not 36: a chip is a button, and a row of 36-point buttons is a row of near-misses for
                  anybody with a thumb rather than a mouse. The text stays 13-point; the padding grows. */}
              <Box
                px="md"
                radius="pill"
                bg={selected ? 'accent' : 'surface'}
                border
                style={{
                  minHeight: 44,
                  justifyContent: 'center',
                  alignItems: grid ? 'center' : 'flex-start',
                }}
              >
                {/* `onAccent` on the selected chip: the theme's `text` is black in the light appearance, which is
                    3.7:1 on the accent fill — the one label in the control that must be read at a glance. */}
                <AppText
                  variant="caption"
                  numberOfLines={1}
                  style={{
                    color: selected ? color.onAccent : color.text,
                    opacity: selected ? 1 : 0.9,
                  }}
                >
                  {option.label}
                </AppText>
              </Box>
            </Pressable>
          );
        })}
      </Box>
    </Box>
  );
}
