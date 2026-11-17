import { useState } from 'react';
import { Pressable, ScrollView, TextInput } from 'react-native';

import { Box, AppText, Button, ChipPicker, Screen, ScreenHeader } from '../../shared/ui';
import { tokens } from '../../app-shell/theme';
import { useCurrentHome } from '../../entities/home';
import { useDevices, type Device } from '../../entities/device';
import {
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
  const { data: scenes, isPending } = useScenes(homeId ?? undefined);
  const { data: devices } = useDevices(homeId ?? undefined);
  const activate = useActivateScene(homeId ?? undefined);
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
      {/* The screen had no header and no way out: it is pushed from the dashboard's scene strip and the
          tab bar's Automations destination, and the only exit was the platform's own gesture. */}
      <ScreenHeader title="Scenes" />
      <ScrollView
        contentContainerStyle={{
          padding: tokens.space.lg,
          gap: tokens.space.md,
          paddingBottom: tokens.layout.tabBarClearance,
        }}
      >
        <Box gap="xs">
          <AppText variant="caption" tone="muted">
            One press, several devices. A scene cannot be scheduled — that is what automations are for.
          </AppText>
        </Box>

        {(scenes ?? []).length === 0 && !isPending ? (
          <AppText tone="muted">No scenes yet. Build one below.</AppText>
        ) : null}

        {(scenes ?? []).map((scene) => (
          <Box key={scene.id} row gap="md" p="md" radius="md" bg="surface" border>
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
              onPress={() => activate.mutate(scene.id)}
            />
            <Pressable onPress={() => remove.mutate(scene.id)} accessibilityRole="button">
              <AppText variant="caption" tone="danger">
                Delete
              </AppText>
            </Pressable>
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
              placeholderTextColor={tokens.color.muted}
              value={name}
              onChangeText={setName}
              style={field}
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
                      options={changes.map((p) => ({ id: p.code, label: readable(p.code, p) }))}
                      onChange={(code) =>
                        setActions((rows) =>
                          rows.map((row, i) =>
                            i === index
                              ? { ...row, code, value: defaultFor(changes.find((p) => p.code === code)) }
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

          {/* A grey button with no reason is a puzzle. This says what is still missing, in the order the
              form asks for it. */}
          {!canSave ? (
            <AppText variant="tiny" tone="muted">
              {name.trim().length === 0
                ? 'Name the scene to finish.'
                : 'Choose what the first action should do.'}
            </AppText>
          ) : null}
          <Button
            label={create.isPending ? 'Saving…' : 'Create scene'}
            disabled={!canSave}
            onPress={save}
          />
        </Box>
      </ScrollView>
    </Screen>
  );
}

/**
 * A point's code in a person's words: `brightness.level` is "Brightness", with its unit if it has one.
 *
 * The editor used to offer the wire name, which is a vendor's identifier, not a thing a customer sets.
 */
function readable(code: string, point: { unit?: string | null }): string {
  const slot = code.split('.')[1] ?? code.split('.')[0] ?? code;
  const words = slot.replace(/[_-]+/g, ' ');
  const label = words.charAt(0).toUpperCase() + words.slice(1);
  return point.unit ? `${label} (${point.unit})` : label;
}

/** The value a point starts at when it is chosen, so the control is never blank. */
function defaultFor(point: { kind?: string } | undefined): string {
  if (!point) return 'true';
  if (point.kind === 'bool') return 'true';
  if (point.kind === 'int' || point.kind === 'float') return '0';
  return '';
}

/**
 * A value control that matches the point's own type.
 *
 * A switch is On/Off buttons, a number is a numeric field with its unit, anything else is text. The old form
 * had one text box labelled "Value (true / false / number)" and asked the customer to know the wire format of
 * their own bulb.
 */
function ValueField({
  point,
  value,
  onChange,
}: {
  point: { kind?: string; unit?: string | null };
  value: string;
  onChange: (next: string) => void;
}) {
  if (point.kind === 'bool') {
    const on = value.trim().toLowerCase() === 'true';
    return (
      <Box gap="xs">
        <AppText variant="tiny" tone="muted" style={{ letterSpacing: 1.1 }}>
          SET TO
        </AppText>
        <ChipPicker
          label=""
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

  return (
    <Box gap="xs">
      <AppText variant="tiny" tone="muted" style={{ letterSpacing: 1.1 }}>
        SET TO{point.unit ? ` (${point.unit})` : ''}
      </AppText>
      <TextInput
        keyboardType={point.kind === 'int' || point.kind === 'float' ? 'numeric' : 'default'}
        placeholder={point.kind === 'int' || point.kind === 'float' ? '0' : 'Value'}
        placeholderTextColor={tokens.color.muted}
        value={value}
        onChangeText={onChange}
        style={field}
      />
    </Box>
  );
}

/**
 * Device chips, with a disambiguator when two devices share a name.
 *
 * Two bulbs called "Acme Bulb E27" are the normal case, not the exotic one: a
 * picker that shows the same label twice cannot be used to build a scene.
 */
export function deviceOptions(devices: Device[]): { id: string; label: string }[] {
  const seen = new Map<string, number>();
  for (const d of devices) seen.set(d.name, (seen.get(d.name) ?? 0) + 1);

  return devices.map((d) => ({
    id: d.id,
    // The serial tail is what is printed on the device, so it is the thing the
    // person can match against the one in their hand.
    label:
      (seen.get(d.name) ?? 0) > 1
        ? `${d.name} · ${(d.serial ?? '').slice(-4) || d.id.slice(-4)}`
        : d.name,
  }));
}

const field = {
  backgroundColor: tokens.color.bg,
  borderWidth: 1,
  borderColor: tokens.color.border,
  borderRadius: tokens.radius.md,
  padding: tokens.space.md,
  color: tokens.color.text,
  fontSize: tokens.font.body,
} as const;
