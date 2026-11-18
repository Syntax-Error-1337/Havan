import { useState } from 'react';
import { Modal, Pressable, StyleSheet, TextInput, View } from 'react-native';

import { useTheme, tokens } from '../../app-shell/theme';
import { withAlpha } from '../../shared/config/tokens';
import { AppText, useToast } from '../../shared/ui';
import { useRenameScene } from '../../entities/scene';

/**
 * Rename a scene.
 *
 * The list could create and delete and nothing else, so a scene called "Good nihgt" had exactly one repair: delete it
 * and rebuild six actions. The field arrives pre-filled and Save stays muted until the name actually differs — the same
 * two rules as the room sheet, because the same defect was in both.
 */
export function RenameSceneSheet({
  sceneId,
  name,
  homeId,
  onClose,
}: {
  sceneId: string;
  name: string;
  homeId: string | undefined;
  onClose: () => void;
}) {
  const { color, dark } = useTheme();
  const toast = useToast();
  const rename = useRenameScene(homeId);
  const [draft, setDraft] = useState(name);

  const next = draft.trim();
  const changed = next.length > 0 && next !== name;
  const error = rename.error instanceof Error ? rename.error.message : null;

  const save = () => {
    if (!changed) return;
    rename.mutate(
      { id: sceneId, name: next },
      {
        onSuccess: () => {
          toast(`Renamed to ${next}`);
          onClose();
        },
      },
    );
  };

  return (
    <Modal transparent animationType="fade" visible onRequestClose={onClose}>
      <View
        style={{
          flex: 1,
          justifyContent: 'center',
          padding: tokens.space.xl,
          backgroundColor: withAlpha(color.shadow, 0.62),
        }}
      >
        <View
          style={{
            padding: 20,
            borderRadius: tokens.radius.xl,
            backgroundColor: color.elevated,
            borderWidth: StyleSheet.hairlineWidth,
            borderColor: color.border,
            shadowColor: color.shadow,
            shadowOpacity: dark ? 0.5 : 0.22,
            shadowRadius: 24,
            shadowOffset: { width: 0, height: 12 },
            elevation: 12,
            gap: tokens.space.md,
          }}
        >
          <View style={{ gap: tokens.space.xs }}>
            <AppText variant="title" weight="700">
              Scene name
            </AppText>
            <AppText variant="caption" tone="muted">
              Only the name changes — the actions in it stay exactly as they are.
            </AppText>
          </View>

          <View style={{ gap: tokens.space.xs }}>
            <AppText variant="tiny" tone="muted">
              Name
            </AppText>
            <TextInput
              value={draft}
              onChangeText={setDraft}
              autoFocus
              selectTextOnFocus
              returnKeyType="done"
              onSubmitEditing={save}
              accessibilityLabel="Scene name"
              placeholder="Good night"
              placeholderTextColor={color.placeholder}
              style={{
                paddingVertical: tokens.space.sm,
                paddingHorizontal: tokens.space.md,
                borderRadius: tokens.radius.md,
                backgroundColor: color.card,
                borderWidth: StyleSheet.hairlineWidth,
                borderColor: color.border,
                color: color.text,
              }}
            />
            {error ? (
              <AppText variant="tiny" tone="danger">
                {error}
              </AppText>
            ) : null}
          </View>

          <View style={{ flexDirection: 'row', alignItems: 'center', justifyContent: 'flex-end', gap: tokens.space.xl }}>
            <Pressable
              onPress={onClose}
              accessibilityRole="button"
              hitSlop={12}
              style={{ minHeight: tokens.touch.min, justifyContent: 'center' }}
            >
              <AppText tone="muted">Cancel</AppText>
            </Pressable>
            <Pressable
              onPress={save}
              accessibilityRole="button"
              accessibilityLabel="Save scene name"
              accessibilityState={{ disabled: !changed || rename.isPending }}
              disabled={!changed || rename.isPending}
              hitSlop={12}
              style={{ minHeight: tokens.touch.min, justifyContent: 'center' }}
            >
              <AppText weight="700" style={{ color: changed ? color.accent : color.muted }}>
                {rename.isPending ? 'Saving…' : 'Save'}
              </AppText>
            </Pressable>
          </View>
        </View>
      </View>
    </Modal>
  );
}
