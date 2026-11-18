import { useState } from 'react';
import { Pressable, ScrollView, TextInput } from 'react-native';

import {
  Box,
  AppText,
  Button,
  ChipPicker,
  Screen,
  ScreenHeader,
  useConfirm,
  ScreenScroll,
  ValueField,
  defaultPointValue,
  QueryError,
  readablePoint,
} from '../../shared/ui';
import { tokens, useTheme } from '../../app-shell/theme';
import type { Palette } from '../../shared/config/tokens';
import { useCurrentHome } from '../../entities/home';
import { useDevices, type Device } from '../../entities/device';
import {
  outcome,
  useActivateScene,
  useCreateScene,
  useDeleteScene,
  useScenes,
} from '../../entities/scene';

type Draft = { deviceId: string | null; code: string | null; value: string };

const emptyAction = (): Draft => ({ deviceId: null, code: null, value: 'true' });

/**
 * Scenes: the list, and the editor.
 *
 * The editor is deliberately a small list of "device, point, value" rows rather
 * than a canvas. A canvas is the right answer for a scene with a hundred steps;
 * nobody has one, and a canvas on a phone is a maze.
 */
export function ScenesPage() {
  // Falls back to the home the app would open: `/scenes` reached directly had no home, and therefore
  // no scenes and no devices, on a home that has four of each.
  const { homeId } = useCurrentHome();
  const { color } = useTheme();
  const { data: scenes, isPending, isError, refetch } = useScenes(homeId ?? undefined);
  const { data: devices } = useDevices(homeId ?? undefined);
  const activate = useActivateScene(homeId ?? undefined);
  const { ask, sheet } = useConfirm();
  /** What the last press did, in the same words the dashboard's strip uses. */
  const [note, setNote] = useState<{ sceneId: string; text: string; bad: boolean } | null>(null);

  const run = async (sceneId: string) => {
    setNote(null);
    try {
      const result = await activate.mutateAsync(sceneId);
      setNote({ sceneId, text: outcome(result), bad: result.failed.length > 0 });
    } catch {
      setNote({ sceneId, text: 'Could not run that scene.', bad: true });
    }
  };
  const create = useCreateScene(homeId ?? undefined);
  const remove = useDeleteScene(homeId ?? undefined);

  const [name, setName] = useState('');
  const [actions, setActions] = useState<Draft[]>([emptyAction()]);
  const [problem, setProblem] = useState<string | null>(null);
  const [done, setDone] = useState<string | null>(null);

  const pointsOf = (deviceId: string | null) =>
    (devices ?? []).find((d) => d.id === deviceId)?.points?.filter((p) => p.writable) ?? [];

  /** A value typed as text becomes the type the point expects. */
  const parseValue = (deviceId: string | null, code: string | null, raw: string): unknown => {
    const point = pointsOf(deviceId).find((p) => p.code === code);
    if (point?.kind === 'bool') return raw.trim().toLowerCase() === 'true';
    if (point?.kind === 'int' || point?.kind === 'float') {
      const n = Number(raw);
      return Number.isFinite(n) ? n : null;
    }
    return raw;
  };

  const complete = actions.every((a) => a.deviceId && a.code);
  const canSave = name.trim().length > 0 && complete && !create.isPending;
  // "Add another action" before the current one is finished builds a row the customer cannot complete and
  // cannot explain; the last row has to be valid first.
  const currentComplete = Boolean(actions[actions.length - 1]?.deviceId && actions[actions.length - 1]?.code);

  const save = () => {
    setProblem(null);
    setDone(null);

    const payload = actions.map((a) => ({
      type: 'set_device' as const,
      device_id: a.deviceId as string,
      code: a.code as string,
      value: parseValue(a.deviceId, a.code, a.value),
    }));

    if (payload.some((a) => a.value === null)) {
      setProblem('One of the values is not a number.');
      return;
    }

    create.mutate(
      { name: name.trim(), actions: payload },
      {
        onSuccess: () => {
          setName('');
          setActions([emptyAction()]);
          setDone('Scene saved.');
        },
        onError: (e) => setProblem(e instanceof Error ? e.message : 'Could not save that scene.'),
      },
    );
  };

  return (
    <Screen>
      {/* The confirmation sheet. A hook that returns a dialog nobody renders is a dialog that never opens,
          and a destructive button that silently does nothing is worse than one that asks. */}
      {sheet}
      {/* The screen had no header and no way out: it is pushed from the dashboard's scene strip and the
          tab bar's Automations destination, and the only exit was the platform's own gesture. */}
      <ScreenHeader title="Scenes" />
      {/* The shared body: bottom clearance, taps that land while the keyboard is up, and avoidance. */}
      <ScreenScroll gap={tokens.space.md} onRefresh={() => void refetch()}>
        <Box gap="xs">
          <AppText variant="caption" tone="muted">
            One press, several devices. A scene cannot be scheduled — that is what automations are for.
          </AppText>
        </Box>

        {/* "No scenes yet. Build one below." with a form that cannot save is the empty state doing the
            job of the failure state — the same defect the dashboard had. */}
        {isError ? (
          <QueryError
            title="We could not load your scenes"
            body="Your scenes are still there — the app could not reach the hub."
            onRetry={() => void refetch()}
            testID="scenes-unreachable"
          />
        ) : (scenes ?? []).length === 0 && !isPending ? (
          <AppText tone="muted">No scenes yet. Build one below.</AppText>
        ) : null}

        {(scenes ?? []).map((scene) => (
          <Box key={scene.id} gap="xs">
            <Box row gap="md" p="md" radius="md" bg="surface" border>
              <Box flex={1} gap="xs">
                <AppText weight="600">{scene.name}</AppText>
                <AppText variant="caption" tone="muted">
                  {`${scene.actions.length} action${scene.actions.length === 1 ? '' : 's'}`}
                </AppText>
              </Box>
              <Button
                label="Run"
                variant="ghost"
                loading={activate.isPending && activate.variables === scene.id}
                onPress={() => void run(scene.id)}
              />
              {/* Delete asks first. It used to remove a scene on one press, on a screen where "Run" is the
                  button under the thumb and the two sit a centimetre apart. */}
              <Pressable
                onPress={() =>
                  ask({
                    title: `Delete “${scene.name}”?`,
                    message: 'The devices keep their current settings; only the shortcut goes.',
                    confirmLabel: 'Delete',
                    destructive: true,
                    onConfirm: () => remove.mutate(scene.id),
                  })
                }
                accessibilityRole="button"
                accessibilityLabel={`Delete ${scene.name}`}
                style={{ minHeight: tokens.touch.min, justifyContent: 'center' }}
              >
                <AppText variant="caption" tone="danger">
                  Delete
                </AppText>
              </Pressable>
            </Box>

            {/* The strip on the dashboard says what a press did — "Sent · 3 devices", or why not. This screen
                runs the same action and said nothing at all, so the same press meant two different things
                depending on where it was pressed. */}
            {note?.sceneId === scene.id ? (
              <AppText variant="caption" tone={note.bad ? 'danger' : 'success'}>
                {note.text}
              </AppText>
            ) : null}
          </Box>
        ))}

        <Box gap="md" p="md" radius="md" bg="elevated" border>
          <AppText variant="title">New scene</AppText>

          <Box gap="xs">
            <AppText variant="tiny" tone="muted" style={{ letterSpacing: 1.1 }}>
              NAME
            </AppText>
            <TextInput
              placeholder="Good night, Movie time…"
              placeholderTextColor={color.placeholder}
              value={name}
              onChangeText={setName}
              style={fieldStyle(color)}
            />
          </Box>

          {actions.map((action, index) => {
            const changes = pointsOf(action.deviceId);
            const chosen = changes.find((p) => p.code === action.code);
            return (
              <Box key={index} gap="sm" p="sm" radius="sm" bg="surface">
                <Box row style={{ alignItems: 'center' }}>
                  <AppText variant="tiny" tone="muted" style={{ letterSpacing: 1.1, flex: 1 }}>
                    {actions.length > 1 ? `CHOOSE DEVICE ${index + 1}` : 'CHOOSE DEVICE'}
                  </AppText>
                  {actions.length > 1 ? (
                    <Pressable
                      accessibilityRole="button"
                      accessibilityLabel={`Remove device ${index + 1}`}
                      hitSlop={10}
                      onPress={() => setActions((rows) => rows.filter((_, i) => i !== index))}
                    >
                      <AppText variant="tiny" tone="danger">
                        Remove
                      </AppText>
                    </Pressable>
                  ) : null}
                </Box>

                <ChipPicker
                  label=""
                  columns={2}
                  value={action.deviceId}
                  options={deviceOptions(devices ?? [])}
                  onChange={(id) =>
                    setActions((rows) =>
                      rows.map((row, i) => (i === index ? { ...row, deviceId: id, code: null } : row)),
                    )
                  }
                />

                {!action.deviceId ? (
                  <AppText variant="caption" tone="muted">
                    Pick the device this scene should change, then what it should do.
                  </AppText>
                ) : null}

                {/* "What should happen" only exists once there is a device: a chip row of nothing is the
                    most confusing thing a form can show, and the old one said "Pick a device first" under a
                    control that was already offering choices. */}
                {action.deviceId ? (
                  changes.length > 0 ? (
                    <ChipPicker
                      label="What should happen"
                      value={action.code}
                      options={changes.map((p) => ({ id: p.code, label: readablePoint(p.code, p) }))}
                      onChange={(code) =>
                        setActions((rows) =>
                          rows.map((row, i) =>
                            i === index
                              ? { ...row, code, value: defaultPointValue(changes.find((p) => p.code === code)) }
                              : row,
                          ),
                        )
                      }
                    />
                  ) : (
                    <AppText variant="caption" tone="muted">
                      This device has nothing a scene can change.
                    </AppText>
                  )
                ) : null}

                {/* The value control follows the point's own type: On/Off for a switch, a number for a
                    level. A text box that says "Value (true / false / number)" asks the customer to know
                    the wire format of their own bulb. */}
                {chosen ? <ValueField point={chosen} value={action.value} onChange={(value) =>
                  setActions((rows) => rows.map((row, i) => (i === index ? { ...row, value } : row)))
                } /> : null}
              </Box>
            );
          })}

          {/* Hidden until the current action is complete, not shown-and-disabled: a control that offers
              more work before the first piece exists is a control that invites a mess. */}
          <Box row gap="sm">
            {currentComplete ? (
              <Button
                label="Add another action"
                variant="ghost"
                onPress={() => setActions((rows) => [...rows, emptyAction()])}
              />
            ) : null}
            {actions.length > 1 ? (
              <Button
                label="Remove last"
                variant="ghost"
                onPress={() => setActions((rows) => rows.slice(0, -1))}
              />
            ) : null}
          </Box>

          {problem ? (
            <AppText variant="caption" tone="danger">
              {problem}
            </AppText>
          ) : null}
          {done ? (
            <AppText variant="caption" tone="success">
              {done}
            </AppText>
          ) : null}

          {/* A grey button with no reason is a puzzle. This names *everything* still missing — it used to say
              "Name the scene to finish" on a form that had no device chosen either, so a person who typed a
              name and pressed the (still grey) button learned nothing new. */}
          {!canSave ? (
            <AppText variant="tiny" tone="muted" testID="scene-blocked-by">
              {missingForScene({ name, actions })}
            </AppText>
          ) : null}
          <Button
            label={create.isPending ? 'Saving…' : 'Create scene'}
            disabled={!canSave}
            onPress={save}
          />
        </Box>
      </ScreenScroll>
    </Screen>
  );
}

/**
 * Device chips, with a disambiguator when two devices share a name.
 *
 * Two bulbs called "Acme Bulb E27" are the normal case, not the exotic one: a
 * picker that shows the same label twice cannot be used to build a scene.
 */
export function deviceOptions(devices: Device[]): { id: string; label: string }[] {
  // A scene *does* something, so a device with nothing to set is not a scene target. A room sensor was offered
  // in this list, and picking it produced an action with no points — the customer's only clue was an empty
  // "SET TO" under a device that cannot be set.
  const controllable = devices.filter((d) => d.points?.some((p) => p.writable));
  const seen = new Map<string, number>();
  for (const d of controllable) seen.set(d.name, (seen.get(d.name) ?? 0) + 1);

  return controllable.map((d) => ({
    id: d.id,
    // The serial tail is what is printed on the device, so it is the thing the
    // person can match against the one in their hand.
    label:
      (seen.get(d.name) ?? 0) > 1
        ? `${d.name} · ${(d.serial ?? '').slice(-4) || d.id.slice(-4)}`
        : d.name,
  }));
}

/**
 * The field's style, from the *current* palette.
 *
 * This was `tokens.color.bg` — the static dark palette that exists for legacy call sites — so in light mode the
 * scene name was a black box with white text on a white card. A token that does not come from `useTheme()` is a
 * dark-mode assumption wearing a token's clothes.
 */
/**
 * What the form is still waiting for, in the order it asks.
 *
 * `every` rather than the first one: a person who fixes one thing should be told about the next in the same
 * visit, not on the next attempt.
 */
export function missingForScene({ name, actions }: { name: string; actions: Draft[] }): string {
  const problems: string[] = [];
  const complete = actions.filter((a) => a.deviceId && a.code);
  if (complete.length === 0) problems.push('choose a device and what it should do');
  if (name.trim().length === 0) problems.push('name the scene');
  if (problems.length === 0) return '';
  return `Still to do: ${problems.join(', and ')}.`;
}

const fieldStyle = (color: Palette) =>
  ({
    backgroundColor: color.elevated,
    borderWidth: 1,
    borderColor: color.border,
    borderRadius: tokens.radius.md,
    padding: tokens.space.md,
    color: color.text,
    fontSize: tokens.font.body,
  }) as const;
