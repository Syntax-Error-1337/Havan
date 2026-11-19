import { useState } from 'react';
import { Modal, Pressable, StyleSheet, TextInput, View } from 'react-native';

import { useTheme, tokens } from '../../app-shell/theme';
import { withAlpha } from '../../shared/config/tokens';
import { useDeleteRoom, useRenameRoom } from '../../entities/home';
import { AppText, Confirm, Row, useConfirm, useToast } from '../../shared/ui';

export function RoomRow({
  id,
  name,
  homeId,
  canManage: mayManage,
}: {
  id: string;
  name: string;
  homeId?: string;
  /** A member sees the room, not the controls that would 403. */
  canManage: boolean;
}) {
  const renameRoom = useRenameRoom(homeId);
  const deleteRoom = useDeleteRoom(homeId);
  const roomToast = useToast();
  const { ask, sheet } = useConfirm();
  const [editing, setEditing] = useState(false);

  // A member gets the fact, not a control that would refuse: the row is not an editor and wears no
  // chevron, because there is nowhere for it to go.
  if (!mayManage) return <Row title={name} chevron={false} />;

  return (
    <>
      <Row
        title={name}
        onPress={() => setEditing(true)}
        // The chevron is the whole point. The row has always opened a rename, but nothing on it said so — an
        // inline editor opened *in place of the row* and the room name vanished while you typed a replacement.
        chevron
        right={
          <Pressable
            onPress={() => setEditing(true)}
            accessibilityRole="button"
            accessibilityLabel={`Edit ${name}`}
            // A word 20 points tall is not a target a thumb finds, and a 34-point-wide one is not either — the a11y
            // gate caught this at 34×44 the first time it ran. Both dimensions have to clear the minimum.
            style={{ minHeight: tokens.touch.min, minWidth: tokens.touch.min, justifyContent: 'center', alignItems: 'flex-end' }}
          >
            <AppText tone="accent" variant="subhead">
              Edit
            </AppText>
          </Pressable>
        }
      />
      {editing ? (
        <RoomSheet
          name={name}
          saving={renameRoom.isPending}
          error={renameRoom.error instanceof Error ? renameRoom.error.message : null}
          onCancel={() => setEditing(false)}
          onSave={(next) => {
            renameRoom.mutate(
              { id, name: next },
              {
                onSuccess: () => {
                  roomToast(`Renamed to ${next}`);
                  setEditing(false);
                },
              },
            );
          }}
          onDelete={() => {
            setEditing(false);
            // Deleting a room does not delete its devices — they become unassigned — but it is still the one
            // action here a person can regret, so it asks. `Alert.alert` is a no-op under react-native-web, so
            // this confirmation is drawn by the app and behaves the same everywhere.
            ask({
              title: `Delete ${name}?`,
              message: 'The devices in it stay, and become unassigned.',
              confirmLabel: 'Delete',
              destructive: true,
              onConfirm: () =>
                deleteRoom.mutate(id, { onSuccess: () => roomToast(`${name} deleted`) }),
            });
          }}
        />
      ) : null}
      {sheet}
    </>
  );
}

/**
 * One room, in a sheet: its name, a Save that cannot lie, and the one action that removes it.
 *
 * This replaces an inline editor that opened with an **empty** field — `EditableRow` was handed `value=""` — so
 * renaming "Living Room" meant retyping it from memory, and pressing Save with the field blank did nothing at all,
 * silently. A sheet that arrives pre-filled, says what it is editing, and cannot be saved when nothing changed is
 * smaller than the bug it removes.
 */
function RoomSheet({
  name,
  saving,
  error,
  onCancel,
  onSave,
  onDelete,
}: {
  name: string;
  saving: boolean;
  error: string | null;
  onCancel: () => void;
  onSave: (next: string) => void;
  onDelete: () => void;
}) {
  const { color, dark } = useTheme();
  const [draft, setDraft] = useState(name);
  const next = draft.trim();
  const changed = next.length > 0 && next !== name;

  return (
    <Modal
      // Named for a screen reader: React Native Web supplies role="dialog", aria-modal, the focus move and
      // Escape. It cannot invent a name, and a dialog with no name is announced as "dialog" and nothing more.
      transparent animationType="fade" visible onRequestClose={onCancel}
      aria-label="Room"
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
              Edit room
            </AppText>
            <AppText variant="caption" tone="muted">
              Rooms group your devices. Renaming one changes it for everyone in this home.
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
                if (changed) onSave(next);
              }}
              accessibilityLabel="Room name"
              placeholder="Living Room"
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

          {/* Destructive on its own row, under a hairline: sharing a baseline with Save made "Delete room" and "Save"
              two equally loud answers to one question, and the one a thumb finds first should not be the irreversible
              one. iOS does the same thing in an action sheet for the same reason. */}
          <View style={{ height: StyleSheet.hairlineWidth, backgroundColor: color.border }} />
          <Pressable
            onPress={onDelete}
            accessibilityRole="button"
            accessibilityLabel={`Delete ${name}`}
            style={{ minHeight: tokens.touch.min, justifyContent: 'center' }}
          >
            <AppText tone="danger">Delete room</AppText>
          </Pressable>

          <View style={{ flexDirection: 'row', alignItems: 'center', justifyContent: 'flex-end', gap: tokens.space.xl }}>
            <Pressable
              onPress={onCancel}
              accessibilityRole="button"
              hitSlop={12}
              style={{ minHeight: tokens.touch.min, justifyContent: 'center' }}
            >
              <AppText tone="muted">Cancel</AppText>
            </Pressable>
            <Pressable
              onPress={() => onSave(next)}
              accessibilityRole="button"
              accessibilityLabel="Save room name"
              accessibilityState={{ disabled: !changed || saving }}
              disabled={!changed || saving}
              hitSlop={12}
              style={{ minHeight: tokens.touch.min, justifyContent: 'center' }}
            >
              <AppText weight="700" style={{ color: changed ? color.accent : color.muted }}>
                {saving ? 'Saving…' : 'Save'}
              </AppText>
            </Pressable>
          </View>
        </View>
      </View>

    </Modal>
  );
}
