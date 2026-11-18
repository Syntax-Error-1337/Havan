import { Box, AppText } from './Box';
import { ChipPicker } from './ChipPicker';
import { tokens, useTheme } from '../../app-shell/theme';
import { TextInput } from 'react-native';

/**
 * A value control that matches a point's own type.
 *
 * Scenes and automations both ask the same question — "what should this point be set to?" — and both used to
 * answer it with one text box labelled "Value (true / false / number)", which asks the customer to know the
 * wire format of their own bulb. A switch is On/Off chips, a number is a numeric field with its unit, and
 * anything else is text.
 *
 * This lived in the scenes page until the automations editor needed it, which is the point at which a control
 * stops being a screen's detail and becomes the platform's.
 */
export function ValueField({
  point,
  value,
  onChange,
  label = 'SET TO',
}: {
  point: { kind?: string; unit?: string | null };
  value: string;
  onChange: (next: string) => void;
  /** The heading above the control; a trigger says "BECOMES" rather than "SET TO". */
  label?: string;
}) {
  // Declared before the early return: a hook after a branch is a hook that does not run on every render.
  const { color } = useTheme();
  if (point.kind === 'bool') {
    const on = value.trim().toLowerCase() === 'true';
    return (
      <Box gap="xs">
        <AppText variant="tiny" tone="muted" style={{ letterSpacing: 1.1 }}>
          {label}
        </AppText>
        <ChipPicker
          value={on ? 'on' : 'off'}
          options={[
            { id: 'on', label: 'On' },
            { id: 'off', label: 'Off' },
          ]}
          onChange={(next) => onChange(next === 'on' ? 'true' : 'false')}
        />
      </Box>
    );
  }

  const numeric = point.kind === 'int' || point.kind === 'float';
  return (
    <Box gap="xs">
      <AppText variant="tiny" tone="muted" style={{ letterSpacing: 1.1 }}>
        {point.unit ? `${label} (${point.unit})` : label}
      </AppText>
      <TextInput
        keyboardType={numeric ? 'numeric' : 'default'}
        placeholder={numeric ? '0' : 'Value'}
        placeholderTextColor={color.placeholder}
        value={value}
        onChangeText={onChange}
        // The theme's palette, not `tokens.color`: the static one is dark, and in light mode this field was a
        // black box on a white card — the same defect the scene editor had.
        style={{
          backgroundColor: color.elevated,
          borderWidth: 1,
          borderColor: color.border,
          borderRadius: tokens.radius.md,
          padding: tokens.space.md,
          color: color.text,
          fontSize: tokens.font.body,
        }}
      />
    </Box>
  );
}

/**
 * A point's code in a person's words: `brightness.level` is "Brightness", with its unit if it has one.
 *
 * The wire name is a vendor's identifier, not something a customer sets.
 */
export function readablePoint(code: string, point: { unit?: string | null }): string {
  const slot = code.split('.')[1] ?? code.split('.')[0] ?? code;
  const words = slot.replace(/[_-]+/g, ' ');
  const label = words.charAt(0).toUpperCase() + words.slice(1);
  return point.unit ? `${label} (${point.unit})` : label;
}

/** The value a point starts at when it is chosen, so the control is never blank or wrong-typed. */
export function defaultPointValue(point: { kind?: string } | undefined): string {
  if (!point) return 'true';
  if (point.kind === 'bool') return 'true';
  if (point.kind === 'int' || point.kind === 'float') return '0';
  return '';
}
