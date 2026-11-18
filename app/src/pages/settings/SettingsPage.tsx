import { useEffect, useState, type ReactNode } from 'react';
import { Pressable, ScrollView, TextInput } from 'react-native';
import { useRouter } from 'expo-router';
import * as Clipboard from 'expo-clipboard';

import { RoomRow } from './RoomRow';
import { ChangePasswordSheet } from './ChangePasswordSheet';

import {
  canManage,
  formatLocation,
  parseLocation,
  useCurrentHome,
  type Home,
} from '../../entities/home';
import {
  Box,
  AppText,
  Button,
  Row,
  Screen,
  ScreenHeader,
  ScreenScroll,
  ChipPicker,
  Section,
  Toggle,
  useConfirm,
  useToast,
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
  useSetLocation,
  useSetTimeZone,
} from '../../entities/home';
import { deviceTimeZone } from '../../shared/lib/timeZone';
import { useMe, useSession, useSessions, useSignOutOthers } from '../../entities/session';
import { usePushTokens, useTurnOffAlerts } from '../../entities/notification';
import { useTheme, tokens } from '../../app-shell/theme';
import { useInlineEdit } from '../../shared/lib/useInlineEdit';
import licences from '../../shared/config/licences.json';
import appConfig from '../../../app.json';

const APP_VERSION = appConfig.expo.version;

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
  const { brand, color, dark } = useTheme();
  const appearance = usePreferences((s) => s.appearance);
  const toast = useToast();
  const confirm = useConfirm();
  const { data: sessions } = useSessions();
  const { data: pushTokens } = usePushTokens();
  const turnOffAlerts = useTurnOffAlerts();
  const signOutOthers = useSignOutOthers();

  /**
   * Everything a support person asks for in the first message, without the customer hunting for it.
   *
   * The rule that a secret is never printed applies here: ids, counts, versions and the timezone — never a token,
   * never a push token, never a device secret. A diagnostics button that leaks a credential is worse than none.
   */
  const copyDiagnostics = async () => {
    const text = [
      `${brand.productName} ${APP_VERSION}`,
      `home ${home?.name ?? '—'} (${home?.id ?? '—'})`,
      `devices ${dashboard?.devices_total ?? home?.device_count ?? '—'}`,
      `rooms ${(rooms ?? []).length}, members ${home?.member_count ?? '—'}`,
      `timezone ${home?.timezone ?? '—'}, appearance ${dark ? 'dark' : 'light'}`,
    ].join('\n');

    try {
      await Clipboard.setStringAsync(text);
      toast('Diagnostics copied');
    } catch {
      toast('The clipboard refused — tell support instead', 'failed');
    }
  };

  const setAppearance = usePreferences((s) => s.setAppearance);
  const setTimeZone = useSetTimeZone();
  const setLocation = useSetLocation();

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
  const [passwordOpen, setPasswordOpen] = useState(false);
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
      {/* `padded={false}`: this body draws its own sections edge to edge, and its header scrolls with it. */}
      <ScreenScroll padded={false} paddingTop={tokens.space.lg}>
        <ScreenHeader title="Settings" />

        <Section title="Account">
          <Row title="Email" value={me?.email ?? '—'} />
          <Row title="Name" value={me?.display_name ?? '—'} />
          {/* The rights live one tap from the account they belong to, not buried in a web page. */}
          {/* The app could not change a password at all: the only way out of a leaked one was to delete the
              account. This row is the fix, and the sheet behind it ends every other session. */}
          <Row title="Password" subtitle="Change it, and sign out every other device" onPress={() => setPasswordOpen(true)} />
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
              if (home)
                renameHome.mutate(
                  { id: home.id, name },
                  { onSuccess: () => toast(`Home renamed to ${name}`) },
                );
            }}
          />
          {/* Read-only until now, which meant a home created before the first-run screen asked (every
              home made before the timezone fix) could never be moved: every schedule in it stayed an hour
              or nine out. */}
          <EditableRow
            title="Time zone"
            value={home?.timezone ?? 'UTC'}
            placeholder="Europe/Berlin"
            onSave={(value) => {
              if (!home) return;
              setTimeZone.mutate(
                { id: home.id, timezone: value },
                { onSuccess: () => toast(`Time zone set to ${value}`) },
              );
            }}
            right={
              home && home.timezone !== deviceTimeZone() ? (
                <Pressable
                  accessibilityRole="button"
                  onPress={() => setTimeZone.mutate({ id: home.id, timezone: deviceTimeZone() })}
                  hitSlop={8}
                >
                  <AppText variant="tiny" style={{ color: color.accentInk }}>
                    Use {deviceTimeZone()}
                  </AppText>
                </Pressable>
              ) : null
            }
          />
          {setTimeZone.error ? (
            <AppText variant="tiny" tone="danger">
              {setTimeZone.error instanceof Error
                ? setTimeZone.error.message
                : 'That timezone was not accepted.'}
            </AppText>
          ) : null}

          {/* Where the home is. Optional, and the only reason it is asked for is the rules that talk about the
              sun: without coordinates the engine's Solar condition is `false` forever, so the rule editor does
              not offer "after sunset" at all. */}
          <LocationRow
            home={home}
            saving={setLocation.isPending}
            error={
              setLocation.error instanceof Error ? setLocation.error.message : null
            }
            onSave={(next) => {
              if (!home) return;
              setLocation.mutate(
                { id: home.id, lat: next?.lat ?? null, lon: next?.lon ?? null },
                // Clearing is a change too, and a silent one would look like a failed save.
                { onSuccess: () => toast(next ? 'Location saved' : 'Location cleared') },
              );
            }}
          />
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
            // Members navigated and Devices did not, two rows apart in the same section — a count you cannot open is
            // a number that answers "how many" and nothing else.
            onPress={() => router.push('/')}
          />
          {/* The count was the whole story until now: the API has served the member list, invitations and role
              changes since the first version, and the screen that shows "2 members" was where the story stopped. */}
          <Row
            title="Members"
            value={home?.member_count !== undefined ? String(home.member_count) : '—'}
            onPress={() => router.push('/members')}
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

        {/* Notifications. Push registration has existed since the sender landed and nothing could read it back, so the
            one question people ask — "will this phone buzz?" — had no answer anywhere in the app. */}
        <Section
          // "Alerts", not "Notifications": the screen that lists them is called Alerts, and one product should not have
          // two words for the same thing depending on which screen you are on.
          title="Alerts"
          footnote="Turning alerts off stops the push to your phones. The alert list in the app keeps filling."
        >
          <Row
            title="Phones receiving alerts"
            subtitle={pushTokens?.count === 0 ? 'Alerts appear in the app only' : undefined}
            value={pushTokens?.count !== undefined ? String(pushTokens.count) : '—'}
          />
          <Row
            // "Everywhere" says the scope without claiming it reaches other people's phones: this is the account's own
            // devices, and the subtitle says the part a person needs before pressing — the in-app list survives.
            title="Turn off alerts everywhere"
            subtitle={pushTokens?.count ? 'Your phones only — the list keeps working' : 'No phone is registered'}
            destructive
            disabled={!pushTokens?.count || turnOffAlerts.isPending}
            onPress={() =>
              confirm.ask({
                title: 'Turn off alerts on your phones?',
                message: 'You will not get a notification when an automation fires. The alert list inside the app keeps working, and pairing a phone again turns them back on.',
                confirmLabel: 'Turn off',
                destructive: true,
                onConfirm: () =>
                  turnOffAlerts.mutate(undefined, {
                    onSuccess: (result) =>
                      toast(
                        result.turned_off === 1
                          ? 'Alerts turned off on 1 phone'
                          : `Alerts turned off on ${result.turned_off} phones`,
                      ),
                    onError: () => toast('Could not turn the alerts off', 'failed'),
                  }),
              })
            }
          />
        </Section>

        {/* Security. The refresh-token table has carried a family id and a revocation timestamp since the first
            migration and nothing in the app could see either, so "I left it signed in on a phone I sold" had no answer
            inside the product. */}
        <Section
          title="Security"
        >
          <Row
            title="Signed-in devices"
            // "2" under a title reads as a device called 2 without saying what it counts. The subtitle names the
            // arithmetic: the count includes the phone in the customer's hand.
            subtitle="This one included"
            value={sessions?.count !== undefined ? String(sessions.count) : '—'}
          />
          <Row
            title="Sign out other devices"
            // The consequence, not the arithmetic: the count is one row above, and what a person needs before pressing
            // a destructive control is what happens to *this* phone.
            subtitle={
              sessions?.count === undefined
                ? 'Checking…'
                : sessions.count <= 1
                  ? 'This is the only one'
                  : 'You stay signed in here'
            }
            destructive
            disabled={sessions === undefined || sessions.count <= 1 || signOutOthers.isPending}
            onPress={() =>
              confirm.ask({
                title: 'Sign out other devices?',
                message: 'Every other phone and browser signed in to this account will have to sign in again. This one stays.',
                confirmLabel: 'Sign out',
                destructive: true,
                onConfirm: () =>
                  signOutOthers.mutate(undefined, {
                    onSuccess: (result) =>
                      toast(
                        result.revoked === 0
                          ? 'No other devices were signed in'
                          : `Signed out ${result.revoked} ${result.revoked === 1 ? 'device' : 'devices'}`,
                      ),
                    onError: () => toast('Could not sign the others out', 'failed'),
                  }),
              })
            }
          />
        </Section>

        <Section title="About">
          <Row title="App" value={brand.productName} />
          {/* The version was the whole About section. Two things belong beside it: what is in the build (a licence
              question every store and every enterprise buyer asks) and a way to hand a support person the facts
              without the customer reading a uuid off a screen. */}
          <Row title="Licences" value={`${licences.packages.length} packages`} onPress={() => router.push('/licences')} />
          <Row
            title="Copy diagnostics"
            subtitle="App version, this home, device count"
            onPress={() => {
              void copyDiagnostics();
            }}
          />
        </Section>

        <AppText variant="caption" tone="muted" style={{ textAlign: 'center' }}>
          {brand.productName} {APP_VERSION}
        </AppText>
      </ScreenScroll>
      {passwordOpen ? <ChangePasswordSheet onClose={() => setPasswordOpen(false)} /> : null}
      {confirm.sheet}
    </Screen>
  );
}

/** A row that becomes an input when tapped, and saves on Return or the Save button. */
/**
 * Latitude and longitude, in two fields nobody enjoys.
 *
 * The alternative — a map — is a dependency and a permission for a setting that half the customers never open.
 * What this owes the person instead is honesty: what it is for, what a value looks like, and what happens if
 * they type nonsense. Empty means "clear it", so a customer who moves house can.
 */
function LocationRow({
  home,
  saving,
  error,
  onSave,
}: {
  home: Home | undefined;
  saving: boolean;
  error: string | null;
  onSave: (next: { lat: number; lon: number } | null) => void;
}) {
  const { color } = useTheme();
  const [editing, setEditing] = useState(false);
  const [lat, setLat] = useState('');
  const [lon, setLon] = useState('');
  const [problem, setProblem] = useState<string | null>(null);

  const current = formatLocation(home?.geo_lat, home?.geo_lon);

  const save = () => {
    const parsed = parseLocation(lat, lon);
    if (parsed === 'invalid') {
      setProblem('Latitude runs −90 to 90 and longitude −180 to 180. Leave both empty to clear.');
      return;
    }
    setProblem(null);
    setEditing(false);
    onSave(parsed);
  };

  if (!editing) {
    return (
      <Row
        title="Location"
        subtitle="Used by sunrise and sunset rules"
        value={current}
        onPress={() => {
          setLat(home?.geo_lat != null ? String(home.geo_lat) : '');
          setLon(home?.geo_lon != null ? String(home.geo_lon) : '');
          setProblem(null);
          setEditing(true);
        }}
      />
    );
  }

  return (
    <Box gap="sm" p="md">
      {/* The editor had no title of its own, so two fields appeared between "Time zone" and "Devices" with no
          statement of what they belonged to or what Cancel would cancel. */}
      <AppText weight="600">Location</AppText>
      <AppText variant="tiny" tone="muted">
        Decimal degrees. Sunrise and sunset rules read this.
      </AppText>
      {[
        { label: 'Latitude', value: lat, set: setLat, placeholder: '51.5074' },
        { label: 'Longitude', value: lon, set: setLon, placeholder: '-0.1278' },
      ].map((field) => (
        <Box key={field.label} gap="xs">
          {/* Sentence case: all-caps with letter spacing is this screen's *section header* treatment, and the
              first version gave the field labels the same voice as ACCOUNT and HOME. */}
          <AppText variant="tiny" tone="muted">
            {field.label}
          </AppText>
          <TextInput
            value={field.value}
            onChangeText={field.set}
            placeholder={field.placeholder}
            placeholderTextColor={color.placeholder}
            keyboardType="numbers-and-punctuation"
            autoCorrect={false}
            accessibilityLabel={field.label.toLowerCase()}
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
      ))}
      {problem ? (
        <AppText variant="tiny" tone="danger">
          {problem}
        </AppText>
      ) : null}
      {error ? (
        <AppText variant="tiny" tone="danger">
          {error}
        </AppText>
      ) : null}
      {/* Real buttons, not two words at the end of a field: a grey "Cancel" in the card's footer reads as
          disabled, and the app already has one way to draw an action. */}
      <Box row gap="sm" style={{ marginTop: tokens.space.xs }}>
        <Button
          label="Cancel"
          variant="ghost"
          style={{ flex: 1 }}
          onPress={() => {
            setEditing(false);
            setProblem(null);
          }}
        />
        <Button label="Save" style={{ flex: 1 }} loading={saving} onPress={save} />
      </Box>
    </Box>
  );
}

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

