import { useState } from 'react';
import { Pressable, ScrollView, TextInput } from 'react-native';

import { Box, AppText, Button, Screen, ChipPicker } from '../../shared/ui';
import { tokens } from '../../app-shell/theme';
import { useSession } from '../../entities/session';
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
  const homeId = useSession((s) => s.homeId);
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
      <ScrollView contentContainerStyle={{ padding: tokens.space.lg, gap: tokens.space.md }}>
        <Box gap="xs">
          <AppText variant="display">Scenes</AppText>
          <AppText variant="caption" tone="muted">
            One press, several devices. A scene cannot be scheduled — that is what
            automations are for.
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

          <TextInput
            placeholder="Name, e.g. Good night"
            placeholderTextColor={tokens.color.muted}
            value={name}
            onChangeText={setName}
            style={field}
          />

          {actions.map((action, index) => (
            <Box key={index} gap="sm" p="sm" radius="sm" bg="surface">
              <ChipPicker
                label={`Device ${index + 1}`}
                value={action.deviceId}
                options={(devices ?? []).map((d: Device) => ({ id: d.id, label: d.name }))}
                onChange={(id) =>
                  setActions((rows) =>
                    rows.map((row, i) => (i === index ? { ...row, deviceId: id, code: null } : row)),
                  )
                }
              />
              <ChipPicker
                label="Point"
                value={action.code}
                options={pointsOf(action.deviceId).map((p) => ({ id: p.code, label: p.code }))}
                onChange={(code) =>
                  setActions((rows) => rows.map((row, i) => (i === index ? { ...row, code } : row)))
                }
                emptyLabel="Pick a device first"
              />
              <TextInput
                placeholder="Value (true / false / number)"
                placeholderTextColor={tokens.color.muted}
                value={action.value}
                onChangeText={(value) =>
                  setActions((rows) => rows.map((row, i) => (i === index ? { ...row, value } : row)))
                }
                style={field}
              />
            </Box>
          ))}

          <Box row gap="sm">
            <Button
              label="Add action"
              variant="ghost"
              onPress={() => setActions((rows) => [...rows, emptyAction()])}
            />
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

const field = {
  backgroundColor: tokens.color.bg,
  borderWidth: 1,
  borderColor: tokens.color.border,
  borderRadius: tokens.radius.md,
  padding: tokens.space.md,
  color: tokens.color.text,
  fontSize: tokens.font.body,
} as const;
