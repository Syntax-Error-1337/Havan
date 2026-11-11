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
  label: string;
  value: string | null;
  options: { id: string; label: string }[];
  onChange: (id: string) => void;
  emptyLabel?: string;
}) {
  return (
    <Box gap="xs">
      <AppText variant="caption" tone="muted">
        {label}
      </AppText>
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
            accessibilityLabel={`${label}: ${option.label}`}
          >
            <Box px="md" py="sm" radius="pill" bg={value === option.id ? 'accent' : 'surface'} border>
              <AppText variant="caption" tone={value === option.id ? 'text' : 'muted'}>
                {option.label}
              </AppText>
            </Box>
          </Pressable>
        ))}
      </Box>
    </Box>
  );
}
