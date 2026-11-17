import { useEffect, useState, type ReactNode } from 'react';
import { Pressable, ScrollView, TextInput } from 'react-native';
import { useRouter } from 'expo-router';

import { canManage, useCurrentHome } from '../../entities/home';
import {
  Box,
  AppText,
  Row,
  Screen,
  ScreenHeader,
  ChipPicker,
  Section,
  Toggle,
  useConfirm,
} from '../../shared/ui';
import { usePreferences, type Appearance } from '../../entities/preferences';
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
import { useTheme, tokens } from '../../app-shell/theme';
import { useInlineEdit } from '../../shared/lib/useInlineEdit';

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
  const { brand, color } = useTheme();
  const appearance = usePreferences((s) => s.appearance);
  const setAppearance = usePreferences((s) => s.setAppearance);

  const { homeId } = useCurrentHome();
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
  const [adding, setAdding] = useState(false);
  const [roomName, setRoomName] = useState('');

  const create = () => {
    const name = roomName.trim();
    if (!name) return;
    createRoom.mutate(
      { name },
      {
        onSuccess: () => {
          setRoomName('');
          setAdding(false);
        },
      },
    );
  };

  const home = dashboard?.home ?? homes?.find((h) => h.id === homeId);
  // A member cannot add, rename or delete rooms: the server answers 403 and these rows used to fail
  // silently. The role ships with the homes list, so a control is drawn only when it will work.
  const role = homes?.find((h) => h.id === homeId)?.role;
  const mayManage = role ? canManage(role) : false;

  return (
    <Screen>
      <ScrollView
        contentContainerStyle={{
          paddingTop: tokens.space.lg,
          paddingBottom: tokens.layout.tabBarClearance,
        }}
      >
        <ScreenHeader title="Settings" />

        <Section title="Account">
          <Row title="Email" value={me?.email ?? '—'} />
          <Row title="Name" value={me?.display_name ?? '—'} />
          {/* The rights live one tap from the account they belong to, not buried in a web page. */}
          <Row
            title="Privacy"
            subtitle="A copy of your data, or delete your account"
            onPress={() => router.push('/privacy')}
          />
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
          {/* `home.role` is not in the payload, so the rule comes from the homes list. */}
          {/* `devices.length` is a *page* of the dashboard's cursor, and on this screen the query often
              answers with none of it — so a six-device home said "Devices 0". The total is the only count
              that is true regardless of how much of the list was fetched. */}
          {/* An em dash while the count is in flight: "Devices 0" for the half-second before the answer
              arrives is the same lie as "Devices 0" on a home that has them, and just as believable. */}
          <Row
            title="Devices"
            value={
              dashboard?.devices_total !== undefined || home?.device_count !== undefined
                ? String(dashboard?.devices_total ?? home?.device_count)
                : '—'
            }
          />
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

        <Section
          title="Rooms"
          footnote={
            mayManage
              ? 'A device belongs to the room you put it in.'
              : 'Only an owner or admin can change the rooms in this home.'
          }
        >
          {(rooms ?? []).map((room) => (
            <RoomRow
              key={room.id}
              id={room.id}
              name={room.name}
              homeId={homeId ?? undefined}
              canManage={mayManage}
            />
          ))}
          {/* This row used to POST a room called "New room" the instant it was touched — no name, no
              confirmation, and a chevron on a row that navigates nowhere. It asks for the name first. */}
          {!mayManage ? null : adding ? (
            <Box row gap="sm" p="md" style={{ alignItems: 'center' }}>
              <TextInput
                value={roomName}
                onChangeText={setRoomName}
                autoFocus
                placeholder="Room name"
                placeholderTextColor={color.muted}
                onSubmitEditing={create}
                style={{
                  flex: 1,
                  color: color.text,
                  borderBottomWidth: 1,
                  borderBottomColor: color.border,
                  paddingVertical: 8,
                }}
              />
              {/* Pressable, not a text with an onPress: a word is a 15-point tap target, and the only
                  way to finish the only form on the screen should not be one you can miss. */}
              <Pressable
                accessibilityRole="button"
                accessibilityLabel="Add the room"
                disabled={!roomName.trim()}
                onPress={create}
                hitSlop={10}
              >
                <AppText weight="600" style={{ color: roomName.trim() ? color.accent : color.muted }}>
                  Add
                </AppText>
              </Pressable>
              <Pressable
                accessibilityRole="button"
                accessibilityLabel="Cancel"
                onPress={() => {
                  setAdding(false);
                  setRoomName('');
                }}
                hitSlop={10}
              >
                <AppText tone="muted">Cancel</AppText>
              </Pressable>
            </Box>
          ) : (
            <Row
              title={rooms && rooms.length === 0 ? 'Add the first room' : 'Add a room'}
              onPress={() => setAdding(true)}
              chevron={false}
            />
          )}
          {createRoom.isError ? (
            <AppText variant="tiny" tone="danger" style={{ paddingHorizontal: tokens.space.lg }}>
              Only an owner or admin can add a room to this home.
            </AppText>
          ) : null}
        </Section>

        {/* The palette has had a light half since the first theme pass and nothing in the app could
            reach it — the comment in `providers.tsx` claimed Settings could switch modes and no such
            control existed. This is that control. */}
        <Section
          title="Appearance"
          footnote={`${brand.productName} opens in the appearance your operator chose. This is your own phone's opinion, remembered on this device.`}
        >
          <ChipPicker
            options={[
              { id: 'system', label: 'Match my phone' },
              { id: 'light', label: 'Light' },
              { id: 'dark', label: 'Dark' },
            ]}
            value={appearance}
            onChange={(id) => void setAppearance(id as Appearance)}
          />
        </Section>

        <Section title="About">
          <Row title="App" value={brand.productName} />
        </Section>

        <AppText variant="caption" tone="muted" style={{ textAlign: 'center' }}>
          {brand.productName} 0.1.0
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
  const { color, brand } = useTheme();
  // The rules — reset the draft on open, blank is a cancel, unchanged is not a request — live in the
  // hook, because the device header renames things too and the two screens must behave the same.
  const { editing, draft, setDraft, open, save } = useInlineEdit(value, onSave);

  if (!editing) {
    return (
      <Row
        title={title}
        value={right ? undefined : value}
        right={right}
        // Tapping the row still edits it when a control sits on the right: the
        // Delete button captures its own taps, the rest of the row opens the draft.
        onPress={open}
      />
    );
  }

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
function RoomRow({
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
  const { ask, sheet } = useConfirm();

  const confirmDelete = () => {
    // Deleting a room does not delete its devices — they become unassigned — but it
    // is still the one action here a person can regret, so it asks.
    // `Alert.alert` is a no-op under react-native-web, so this confirmation never appeared and the room
    // was never deleted in the web build. The in-app sheet behaves the same everywhere.
    ask({
      title: `Delete ${name}?`,
      message: 'The devices in it stay, and become unassigned.',
      confirmLabel: 'Delete',
      destructive: true,
      onConfirm: () => deleteRoom.mutate(id),
    });
  };

  // A member gets the fact, not a control that would refuse: the row is not an editor and wears no
  // chevron, because there is nowhere for it to go.
  if (!mayManage) return <Row title={name} chevron={false} />;

  return (
    <>
      {sheet}
      <EditableRow
        title={name}
        value=""
        placeholder="Room name"
      onSave={(next) => {
        if (mayManage) renameRoom.mutate({ id, name: next });
      }}
      right={
        mayManage ? (
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
        ) : null
      }
      />
    </>
  );
}
