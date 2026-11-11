import { useState } from 'react';
import { Pressable, ScrollView, Switch, TextInput } from 'react-native';

import { Box, AppText, Button, Screen, ChipPicker } from '../../shared/ui';
import { tokens } from '../../app-shell/theme';
import { useHomes } from '../../entities/home';
import { useDevices } from '../../entities/device';
import {
  describe,
  useAutomationRuns,
  useAutomations,
  useCreateAutomation,
  useDeleteAutomation,
  useToggleAutomation,
  type Automation,
} from '../../entities/automation';

/**
 * Rules.
 *
 * A list of what runs and whether it runs, a switch, and a way to look at the
 * last executions — because the first question about any automation is always
 * "did it actually run?", and the log answers it without a debugger.
 */
export function AutomationsPage() {
  const { data: homes } = useHomes();
  const homeId = homes?.[0]?.id;

  const { data: rules, isLoading } = useAutomations(homeId);
  const toggle = useToggleAutomation(homeId);
  const remove = useDeleteAutomation(homeId);

  const [open, setOpen] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);

  return (
    <Screen>
      <ScrollView contentContainerStyle={{ padding: tokens.space.lg, gap: tokens.space.md }}>
        <Box row style={{ justifyContent: 'space-between', alignItems: 'flex-end' }}>
          <Box gap="xs">
            <AppText variant="display">Automatic</AppText>
            <AppText variant="caption" tone="muted">
              {rules?.length ? `${rules.length} rule(s)` : 'Nothing runs on its own yet'}
            </AppText>
          </Box>
          <Button
            label={creating ? 'Close' : 'New rule'}
            variant="ghost"
            onPress={() => setCreating((v) => !v)}
          />
        </Box>

        {creating && homeId ? (
          <NewRuleForm
            homeId={homeId}
            onDone={() => setCreating(false)}
          />
        ) : null}

        {isLoading ? <AppText tone="muted">Loading…</AppText> : null}

        {!isLoading && rules?.length === 0 ? (
          <Box gap="sm" p="lg" radius="md" bg="surface" border>
            <AppText variant="title">No rules</AppText>
            <AppText tone="muted">
              A rule is a trigger, an optional condition and one or more actions. Turn a lamp on at
              sunset, or when a door sensor opens.
            </AppText>
          </Box>
        ) : null}

        {(rules ?? []).map((rule) => (
          <Box key={rule.id} gap="sm" p="md" radius="md" bg="surface" border>
            <Box row gap="md" style={{ alignItems: 'center' }}>
              <Box flex={1} gap="xs">
                <AppText weight="600">{rule.name}</AppText>
                <AppText variant="caption" tone="muted">
                  {describe(rule)}
                </AppText>
              </Box>
              <Switch
                value={rule.enabled}
                onValueChange={(enabled) => toggle.mutate({ id: rule.id, enabled })}
                trackColor={{ true: tokens.color.accent, false: tokens.color.border }}
              />
            </Box>

            <Box row gap="md">
              <Pressable onPress={() => setOpen(open === rule.id ? null : rule.id)}>
                <AppText variant="caption" tone="accent">
                  {open === rule.id ? 'Hide history' : 'History'}
                </AppText>
              </Pressable>
              <Pressable
                onPress={() => remove.mutate(rule.id)}
                disabled={remove.isPending}
              >
                <AppText variant="caption" tone="danger">
                  Delete
                </AppText>
              </Pressable>
            </Box>

            {open === rule.id ? <History rule={rule} /> : null}
          </Box>
        ))}

        {toggle.error instanceof Error ? (
          <AppText variant="caption" tone="danger">
            {toggle.error.message}
          </AppText>
        ) : null}
      </ScrollView>
    </Screen>
  );
}

function History({ rule }: { rule: Automation }) {
  const { data: runs, isLoading } = useAutomationRuns(rule.id);

  if (isLoading) return <AppText variant="caption" tone="muted">Loading…</AppText>;
  if (!runs?.length) {
    return (
      <AppText variant="caption" tone="muted">
        Never run. {rule.enabled ? 'It is on — check the trigger.' : 'It is off.'}
      </AppText>
    );
  }

  return (
    <Box gap="xs">
      {runs.map((run) => (
        <Box key={run.id} gap="xs">
          <AppText variant="caption" tone={run.status === 'completed' ? 'success' : 'danger'}>
            {new Date(run.started_at).toLocaleString()} · {run.status} · {run.trigger_kind}
          </AppText>
          {run.log.steps.map((step, i) => (
            <AppText key={`${run.id}-${i}`} variant="caption" tone="muted">
              {'  '}
              {step.kind === 'trigger' ? '↯' : '→'} {step.detail}
            </AppText>
          ))}
        </Box>
      ))}
    </Box>
  );
}

/**
 * A rule the app can actually build today: a schedule or a device crossing a
 * value, and one action on one point. Anything richer is still a curl call —
 * better an honest small editor than a form that pretends.
 */
function NewRuleForm({ homeId, onDone }: { homeId: string; onDone: () => void }) {
  const { data: devices } = useDevices(homeId);
  const create = useCreateAutomation(homeId);

  const [name, setName] = useState('');
  const [kind, setKind] = useState<'schedule' | 'device_state'>('schedule');
  const [minutes, setMinutes] = useState('5');
  const [source, setSource] = useState<string | null>(null);
  const [sourceCode, setSourceCode] = useState<string | null>(null);
  const [sourceValue, setSourceValue] = useState('true');
  const [target, setTarget] = useState<string | null>(null);
  const [targetCode, setTargetCode] = useState<string | null>(null);
  const [targetValue, setTargetValue] = useState('true');

  const points = (id: string | null) =>
    (devices ?? []).find((d) => d.id === id)?.points?.filter((p) => p.writable) ?? [];

  const field = {
    backgroundColor: tokens.color.bg,
    borderWidth: 1,
    borderColor: tokens.color.border,
    borderRadius: tokens.radius.md,
    padding: tokens.space.md,
    color: tokens.color.text,
    fontSize: tokens.font.body,
  } as const;

  const canSave =
    name.trim().length > 0 &&
    target !== null &&
    targetCode !== null &&
    (kind === 'schedule' || (source !== null && sourceCode !== null));

  const save = () => {
    if (!canSave || !target || !targetCode) return;

    const parsed = (text: string): unknown =>
      text === 'true' ? true : text === 'false' ? false : Number.isNaN(Number(text)) ? text : Number(text);

    create.mutate(
      {
        name: name.trim(),
        definition: {
          trigger:
            kind === 'schedule'
              ? { type: 'schedule', every_secs: Math.max(5, Number(minutes) * 60 || 300) }
              : {
                  type: 'device_state',
                  device_id: source as string,
                  code: sourceCode as string,
                  op: 'eq',
                  value: parsed(sourceValue),
                },
          conditions: [],
          actions: [
            { type: 'set_device', device_id: target, code: targetCode, value: parsed(targetValue) },
          ],
        },
      },
      { onSuccess: onDone },
    );
  };

  return (
    <Box gap="md" p="md" radius="md" bg="elevated" border>
      <AppText variant="title">New rule</AppText>

      <TextInput
        placeholder="Name, e.g. Bathroom light at night"
        placeholderTextColor={tokens.color.muted}
        value={name}
        onChangeText={setName}
        style={field}
      />

      <AppText variant="caption" tone="muted">
        When
      </AppText>
      <Box row gap="sm">
        <Button
          label="Every few minutes"
          variant={kind === 'schedule' ? 'primary' : 'ghost'}
          onPress={() => setKind('schedule')}
        />
        <Button
          label="A device changes"
          variant={kind === 'device_state' ? 'primary' : 'ghost'}
          onPress={() => setKind('device_state')}
        />
      </Box>

      {kind === 'schedule' ? (
        <TextInput
          placeholder="Minutes between runs"
          placeholderTextColor={tokens.color.muted}
          keyboardType="number-pad"
          value={minutes}
          onChangeText={setMinutes}
          style={field}
        />
      ) : (
        <>
          <ChipPicker
            label="Device"
            value={source}
            options={(devices ?? []).map((d) => ({ id: d.id, label: d.name }))}
            onChange={(id) => {
              setSource(id);
              setSourceCode(null);
            }}
          />
          <ChipPicker
            label="Point"
            value={sourceCode}
            options={points(source).map((p) => ({ id: p.code, label: p.code }))}
            onChange={setSourceCode}
          />
          <TextInput
            placeholder="Becomes (true / false / number)"
            placeholderTextColor={tokens.color.muted}
            value={sourceValue}
            onChangeText={setSourceValue}
            style={field}
          />
        </>
      )}

      <AppText variant="caption" tone="muted">
        Then
      </AppText>
      <ChipPicker
        label="Device"
        value={target}
        options={(devices ?? []).map((d) => ({ id: d.id, label: d.name }))}
        onChange={(id) => {
          setTarget(id);
          setTargetCode(null);
        }}
      />
      <ChipPicker
        label="Point"
        value={targetCode}
        options={points(target).map((p) => ({ id: p.code, label: p.code }))}
        onChange={setTargetCode}
      />
      <TextInput
        placeholder="Set to (true / false / number)"
        placeholderTextColor={tokens.color.muted}
        value={targetValue}
        onChangeText={setTargetValue}
        style={field}
      />

      {create.error instanceof Error ? (
        <AppText variant="caption" tone="danger">
          {create.error.message}
        </AppText>
      ) : null}

      <Button
        label={create.isPending ? 'Saving…' : 'Create'}
        disabled={!canSave || create.isPending}
        onPress={save}
      />
    </Box>
  );
}
