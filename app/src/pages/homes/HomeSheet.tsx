import { useState } from 'react';
import { Modal, Pressable, StyleSheet, TextInput, View } from 'react-native';

import { useTheme, tokens } from '../../app-shell/theme';
import { withAlpha } from '../../shared/config/tokens';
import type { Home } from '../../entities/home';
import { AppText, Confirm, useToast } from '../../shared/ui';
import { useDeleteHome, useRenameHome } from '../../entities/home';

/**
 * One home, opened for the two things a home list could not do: rename it and remove it.
 *
 * `/homes` let you switch and create. A home created by mistake — the second one someone makes to see what happens —
 * could not be renamed or deleted anywhere in the product, and `DELETE /homes/{id}` has been on the server since the
 * first version with nothing calling it. Delete is owner-only and confirmed twice over: the devices, rooms, scenes and
 * rules in it go with it.
 */
export function HomeSheet({
  home,
  onClose,
  onDeleted,
}: {
  home: Home;
  onClose: () => void;
  onDeleted: () => void;
}) {
  const { color, dark } = useTheme();
  const toast = useToast();
  const rename = useRenameHome();
  const remove = useDeleteHome();
  const [draft, setDraft] = useState(home.name);
  const [confirming, setConfirming] = useState(false);

  const next = draft.trim();
  const changed = next.length > 0 && next !== home.name;
  const owner = home.role === 'owner';
  const error = rename.error instanceof Error ? rename.error.message : null;

  return (
    <Modal
      // Named for a screen reader: React Native Web supplies role="dialog", aria-modal, the focus move and
      // Escape. It cannot invent a name, and a dialog with no name is announced as "dialog" and nothing more.
      transparent animationType="fade" visible onRequestClose={onClose}
      aria-label="Home"
    >
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
              Home name
            </AppText>
            <AppText variant="caption" tone="muted">
              The name is what everyone in this home sees, in the app and on the dashboard.
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
              onSubmitEditing={() => {
                if (changed) rename.mutate({ id: home.id, name: next }, { onSuccess: () => { toast(`Renamed to ${next}`); onClose(); } });
              }}
              accessibilityLabel="Home name"
              placeholder="Workshop"
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

          {owner ? (
            <>
              <View style={{ height: StyleSheet.hairlineWidth, backgroundColor: color.border }} />
              <Pressable
                onPress={() => {
                  setConfirming(false);
                  setConfirming(true);
                }}
                accessibilityRole="button"
                accessibilityLabel={`Delete ${home.name}`}
                style={{ minHeight: tokens.touch.min, justifyContent: 'center' }}
              >
                <AppText tone="danger">Delete this home</AppText>
              </Pressable>
            </>
          ) : (
            <AppText variant="tiny" tone="muted">
              Only the owner can delete a home. You can leave this home from Members.
            </AppText>
          )}

          <View style={{ flexDirection: 'row', alignItems: 'center', justifyContent: 'flex-end', gap: tokens.space.xl }}>
            <Pressable onPress={onClose} accessibilityRole="button" hitSlop={12} style={{ minHeight: tokens.touch.min, justifyContent: 'center' }}>
              <AppText tone="muted">Cancel</AppText>
            </Pressable>
            <Pressable
              onPress={() => rename.mutate({ id: home.id, name: next }, { onSuccess: () => { toast(`Renamed to ${next}`); onClose(); } })}
              accessibilityRole="button"
              accessibilityLabel="Save home name"
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

      {confirming ? (
        <Confirm
          title={`Delete ${home.name}?`}
          message={`Every device, room, scene and rule in it goes too. ${home.device_count > 0 ? `That is ${home.device_count} device${home.device_count === 1 ? '' : 's'}.` : ''}`}
          confirmLabel="Delete"
          destructive
          onCancel={() => setConfirming(false)}
          onConfirm={() => {
            setConfirming(false);
            remove.mutate(home.id, {
              onSuccess: () => {
                toast(`${home.name} deleted`);
                onDeleted();
              },
            });
          }}
        />
      ) : null}
    </Modal>
  );
}
