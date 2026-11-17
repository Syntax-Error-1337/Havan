import { Pressable } from 'react-native';

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
}: {
  /** Visible heading for the row. Optional: a caller that already draws its own heading passes nothing. */
  label?: string;
  value: string | null;
  options: { id: string; label: string }[];
  onChange: (id: string) => void;
  emptyLabel?: string;
}) {
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
        {options.map((option) => (
          <Pressable
            key={option.id}
            onPress={() => onChange(option.id)}
            accessibilityRole="button"
            accessibilityState={{ selected: value === option.id }}
            accessibilityLabel={label ? `${label}: ${option.label}` : option.label}
          >
            {/* An unselected chip said its label in muted grey on a surface that is nearly the same grey: the
                choice was there and unreadable. `elevated` is a step up from the card, and the label keeps the
                text tone — selection is carried by the fill, not by hiding the other options. */}
            {/* 44 tall, not 36: a chip is a button, and a row of 36-point buttons is a row of near-misses for
                anybody with a thumb rather than a mouse. The text stays 13-point; the padding grows. */}
            <Box
              px="md"
              radius="pill"
              bg={value === option.id ? 'accent' : 'surface'}
              border
              style={{ minHeight: 44, justifyContent: 'center' }}
            >
              <AppText
                variant="caption"
                style={{ opacity: value === option.id ? 1 : 0.9 }}
              >
                {option.label}
              </AppText>
            </Box>
          </Pressable>
        ))}
      </Box>
    </Box>
  );
}
