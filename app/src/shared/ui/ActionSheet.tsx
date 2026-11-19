import { useCallback, useState } from 'react';
import { Modal, Pressable, StyleSheet, View } from 'react-native';

import { useTheme, tokens } from '../../app-shell/theme';
import { AppText } from './Box';
import { Icon, type IconName } from './Icon';
import { withAlpha } from '../config/tokens';

/**
 * A short list of things you can add, anchored to the bottom of the screen.
 *
 * The dashboard's centre button used to jump straight to "add a device", while adding a room lived in a
 * chip among the filters and adding a scene lived in a dashed card beside the device list. Three creates,
 * three places, three different shapes — a customer looking for "add something" had to know which one they
 * wanted before they could find where to say it.
 *
 * One button, one list. Rows rather than columns because the labels are sentences ("Add a scene, then set
 * it going from the Automations tab"), and a sheet you can read is faster than a grid you have to decode.
 */
export type ActionItem = {
  key: string;
  label: string;
  hint?: string;
  icon: IconName;
  onPress: () => void;
};

export type ActionSheetOptions = {
  title: string;
  items: ActionItem[];
};

export function useActionSheet() {
  const [options, setOptions] = useState<ActionSheetOptions | null>(null);

  const open = useCallback((next: ActionSheetOptions) => setOptions(next), []);
  const close = useCallback(() => setOptions(null), []);

  return {
    open,
    close,
    sheet: options ? (
      <ActionSheet
        {...options}
        onCancel={close}
        onPick={(item) => {
          close();
          item.onPress();
        }}
      />
    ) : null,
  };
}

export function ActionSheet({
  title,
  items,
  onPick,
  onCancel,
}: ActionSheetOptions & { onPick: (item: ActionItem) => void; onCancel: () => void }) {
  const { color } = useTheme();

  return (
    // Named, or a screen reader says "dialog" and stops. When a sheet has no title the first action names the choice it
    // is offering, which is what a person would say out loud if they were reading it to somebody.
    <Modal
      transparent
      animationType="fade"
      visible
      onRequestClose={onCancel}
      aria-label={title ?? items[0]?.label ?? 'Choose an action'}
    >
      <Pressable
        // Tapping the wash is a cancel, the same as every sheet on the platform: a person who opened this
        // by accident should not have to find a button to undo one tap.
        accessibilityLabel="Dismiss"
        onPress={onCancel}
        style={{
          flex: 1,
          justifyContent: 'flex-end',
          backgroundColor: withAlpha(color.shadow, 0.6),
        }}
      >
        <Pressable
          // Swallows taps inside the sheet without a handler of its own.
          onPress={() => undefined}
          style={{
            margin: tokens.space.md,
            padding: tokens.space.md,
            borderRadius: tokens.radius.xl,
            backgroundColor: color.elevated,
            borderWidth: StyleSheet.hairlineWidth,
            borderColor: color.border,
            gap: 4,
          }}
        >
          <AppText variant="tiny" tone="muted" style={{ letterSpacing: 1.1, paddingHorizontal: 10 }}>
            {title.toUpperCase()}
          </AppText>

          {items.map((item) => (
            <Pressable
              key={item.key}
              accessibilityRole="button"
              accessibilityLabel={item.label}
              testID={`action-${item.key}`}
              onPress={() => onPick(item)}
              style={({ pressed }) => ({
                flexDirection: 'row',
                alignItems: 'center',
                gap: tokens.space.md,
                paddingHorizontal: 10,
                paddingVertical: 12,
                borderRadius: tokens.radius.md,
                backgroundColor: pressed ? withAlpha(color.accent, 0.12) : 'transparent',
              })}
            >
              <View
                style={{
                  width: 34,
                  height: 34,
                  borderRadius: 17,
                  alignItems: 'center',
                  justifyContent: 'center',
                  backgroundColor: withAlpha(color.accent, 0.16),
                }}
              >
                <Icon name={item.icon} size={18} color={color.accent} />
              </View>
              <View style={{ flex: 1, gap: 1 }}>
                <AppText variant="subhead" weight="600">
                  {item.label}
                </AppText>
                {item.hint ? (
                  <AppText variant="tiny" tone="muted">
                    {item.hint}
                  </AppText>
                ) : null}
              </View>
              <Icon name="chevron-right" size={16} color={color.muted} />
            </Pressable>
          ))}

          <Pressable
            accessibilityRole="button"
            accessibilityLabel="Cancel"
            testID="action-cancel"
            onPress={onCancel}
            style={{ alignItems: 'center', paddingVertical: 12 }}
          >
            <AppText weight="600" tone="muted">
              Cancel
            </AppText>
          </Pressable>
        </Pressable>
      </Pressable>
    </Modal>
  );
}
