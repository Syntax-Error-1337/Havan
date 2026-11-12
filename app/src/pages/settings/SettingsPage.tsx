import { useState, type ReactNode } from 'react';
import { Alert, Pressable, ScrollView, TextInput } from 'react-native';
import { useRouter } from 'expo-router';

import { Box, AppText, Row, Screen, Section, Toggle } from '../../shared/ui';
import {
  useCreateRoom,
  useDashboard,
  useSetPresence,
  useDeleteRoom,
  useHomes,
  useRenameHome,
  useRenameRoom,
  useRooms,
} from '../../entities/home';
import { useMe, useSession } from '../../entities/session';
import { useTheme } from '../../app-shell/theme';
import { tokens } from '../../shared/config/tokens';

/**
 * Settings.
 *
 * Grouped rows, the way iOS does it: who you are, what the home is called, which
 * rooms exist, and the one destructive action where it cannot be hit by accident.
 *
 * Edits happen in place — a settings screen that pushes a form for a one-word rename
 * is a screen people stop opening. Each editable row owns its draft: one page-level
 * draft is a stale closure waiting to happen, where the Save button of the row you
 * opened second sends the text of the first.
 */
export function SettingsPage() {
  const router = useRouter();

  const homeId = useSession((s) => s.homeId);
  const signOut = useSession((s) => s.signOut);

  const { data: me } = useMe();
  const { data: homes } = useHomes();
  const { data: dashboard } = useDashboard(homeId ?? null);
  const { data: rooms } = useRooms(homeId ?? undefined);

  const renameHome = useRenameHome();
  const createRoom = useCreateRoom(homeId ?? undefined);
  const setPresence = useSetPresence(homeId ?? undefined);
  // Local until the server confirms; the switch must move when the finger does.
  const [atHome, setAtHome] = useState(true);

  const home = dashboard?.home ?? homes?.find((h) => h.id === homeId);

  return (
    <Screen>
      <ScrollView contentContainerStyle={{ paddingTop: tokens.space.lg, paddingBottom: 64 }}>
        <Box px="lg" style={{ paddingBottom: tokens.space.md }}>
          <AppText variant="display" weight="700">
            Settings
          </AppText>
        </Box>

        <Section title="Account">
          <Row title="Email" value={me?.email ?? '—'} />
          <Row title="Name" value={me?.display_name ?? '—'} />
          <Row
            title="Sign out"
            destructive
            onPress={() => {
              void signOut();
            }}
          />
        </Section>

        <Section title="Home" footnote="Renaming a home changes it for everyone in it.">
          <EditableRow
            title="Name"
            value={home?.name ?? ''}
            placeholder="Home name"
            onSave={(name) => {
              if (home) renameHome.mutate({ id: home.id, name });
            }}
          />
          <Row title="Time zone" value={home?.timezone ?? 'UTC'} />
          <Row title="Devices" value={String(dashboard?.devices.length ?? 0)} />
          <Row
            title="At home"
            subtitle="Automations that ask “is anyone home” use this"
            right={
              <Toggle
                label="At home"
                value={atHome}
                disabled={setPresence.isPending}
                onChange={(next) => {
                  setAtHome(next);
                  setPresence.mutate(next);
                }}
              />
            }
          />
        </Section>

        <Section title="Rooms" footnote="A device belongs to the room you put it in.">
          {(rooms ?? []).map((room) => (
            <RoomRow key={room.id} id={room.id} name={room.name} homeId={homeId ?? undefined} />
          ))}
          <Row
            title={rooms && rooms.length === 0 ? 'Add the first room' : 'Add a room'}
            onPress={() => createRoom.mutate({ name: 'New room' })}
          />
        </Section>

        <Section title="About">
          <Row title="App" value="Haven" />
          <Row title="Back to home" onPress={() => router.push('/')} />
        </Section>

        <AppText variant="caption" tone="muted" style={{ textAlign: 'center' }}>
          Haven 0.1.0
        </AppText>
      </ScrollView>
    </Screen>
  );
}

/** A row that becomes an input when tapped, and saves on Return or the Save button. */
function EditableRow({
  title,
  value,
  placeholder,
  onSave,
  right,
}: {
  title: string;
  value: string;
  placeholder: string;
  onSave: (next: string) => void;
  right?: ReactNode;
}) {
  const { color } = useTheme();
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(value);

  if (!editing) {
    return (
      <Row
        title={title}
        value={right ? undefined : value}
        right={right}
        // Tapping the row still edits it when a control sits on the right: the
        // Delete button captures its own taps, the rest of the row opens the draft.
        onPress={() => {
          setDraft(value);
          setEditing(true);
        }}
      />
    );
  }

  const save = () => {
    setEditing(false);
    const next = draft.trim();
    if (next.length > 0 && next !== value) onSave(next);
  };

  return (
    <Box row gap="sm" p="md" style={{ alignItems: 'center' }}>
      <TextInput
        value={draft}
        onChangeText={setDraft}
        autoFocus
        placeholder={placeholder}
        placeholderTextColor={color.muted}
        onSubmitEditing={save}
        style={{
          flex: 1,
          color: color.text,
          borderBottomWidth: 1,
          borderBottomColor: color.border,
          paddingVertical: tokens.space.sm,
        }}
      />
      <Pressable onPress={save} accessibilityRole="button">
        <AppText tone="accent">Save</AppText>
      </Pressable>
    </Box>
  );
}

/** One room: tap the name to rename it, Delete to remove it (with a confirmation). */
function RoomRow({ id, name, homeId }: { id: string; name: string; homeId?: string }) {
  const renameRoom = useRenameRoom(homeId);
  const deleteRoom = useDeleteRoom(homeId);

  const confirmDelete = () => {
    // Deleting a room does not delete its devices — they become unassigned — but it
    // is still the one action here a person can regret, so it asks.
    Alert.alert(`Delete ${name}?`, 'The devices in it stay, and become unassigned.', [
      { text: 'Cancel', style: 'cancel' },
      { text: 'Delete', style: 'destructive', onPress: () => deleteRoom.mutate(id) },
    ]);
  };

  return (
    <EditableRow
      title={name}
      value={name}
      placeholder="Room name"
      onSave={(next) => renameRoom.mutate({ id, name: next })}
      right={
        <Pressable
          onPress={confirmDelete}
          accessibilityRole="button"
          accessibilityLabel={`Delete ${name}`}
          hitSlop={12}
        >
          <AppText tone="danger" variant="subhead">
            Delete
          </AppText>
        </Pressable>
      }
    />
  );
}
