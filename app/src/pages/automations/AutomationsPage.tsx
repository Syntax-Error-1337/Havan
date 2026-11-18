import { useState } from 'react';
import { useTheme } from '../../app-shell/theme';
import { Pressable, ScrollView, Switch, TextInput, View } from 'react-native';

import {
  Box,
  AppText,
  Button,
  Card,
  ChipPicker,
  EmptyState,
  TouchText,
  ListSkeleton,
  QueryError,
  useConfirm,
  Skeleton,
  Screen,
  ScreenScroll,
  ValueField,
  defaultPointValue,
  readablePoint,
} from '../../shared/ui';
import { tokens } from '../../app-shell/theme';
import { useHomes } from '../../entities/home';
import { ago } from '../../entities/notification';
import { useDevices } from '../../entities/device';
import {
  describe,
  useAutomationRuns,
  useAutomations,
  useCreateAutomation,
  useDeleteAutomation,
  useRunAutomation,
  useToggleAutomation,
  type Automation,
  type Condition,
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

  const { data: rules, isLoading, isError, refetch } = useAutomations(homeId);
  // Only for the one-line description: "turn Porch light on" instead of "set switch.power".
  const { data: homeDevices } = useDevices(homeId);
  const toggle = useToggleAutomation(homeId);
  const run = useRunAutomation();
  const { color } = useTheme();
  const { ask, sheet } = useConfirm();
  const remove = useDeleteAutomation(homeId);

  const [open, setOpen] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);

  return (
    <Screen>
      {sheet}
      <ScreenScroll gap={tokens.space.md} onRefresh={() => void refetch()}>
        {/* At 200% zoom a 195-point row cannot hold a display-size title and a button side by side; wrapping
            puts the button underneath instead of past the right edge. */}
        <Box row style={{ justifyContent: 'space-between', alignItems: 'flex-end', flexWrap: 'wrap', gap: 8 }}>
          <Box gap="xs">
            <AppText variant="display">Automations</AppText>
            {/* The header is a claim about the home, so it must not make one it could not check: with the
                fetch failed it said "Nothing runs on its own yet" directly above "We could not load your
                automations". */}
            <AppText variant="caption" tone="muted">
              {isError
                ? 'Could not load your rules'
                : rules?.length
                  ? `${rules.length} ${rules.length === 1 ? 'rule' : 'rules'}`
                  : // The empty card below already says "nothing yet" and explains what a rule is. Two sentences for one
                    // fact is how the hero card once said everything twice.
                    ''}
            </AppText>
          </Box>
          {/* Two blue actions for one outcome is one too many: on an empty screen the card owns the action, and the
              header button appears once there is a list to add to. */}
          {rules?.length === 0 && !creating ? null : (
            <Button
              label={creating ? 'Close' : 'New rule'}
              variant="ghost"
              onPress={() => setCreating((v) => !v)}
            />
          )}
        </Box>

        {creating && homeId ? (
          <NewRuleForm
            homeId={homeId}
            onDone={() => setCreating(false)}
          />
        ) : null}

        {isLoading ? <ListSkeleton rows={3} rowHeight={104} showHeader={false} /> : null}

        {/* Not while the form is open: an empty-state card underneath the editor that just appeared is the
          screen telling the customer two different things at once. */}
      {/* Before the empty state, and deliberately: a failed fetch left `rules` undefined, so this screen told
          the customer "Nothing runs on its own yet" and invited them to create a rule that could not be
          created — the exact failure `QueryError` exists to prevent, on the one screen that never got it. */}
      {isError ? (
        <QueryError
          title="We could not load your automations"
          body="Nothing has changed — the rules on your hub are still running."
          onRetry={() => void refetch()}
        />
      ) : null}

      {!isLoading && !isError && rules?.length === 0 && !creating ? (
          <EmptyState
            accent={color.accent}
            glyph="sparkle"
            title="No rules yet"
            body="A rule is a trigger, an optional condition and one or more actions. Turn a lamp on at sunset, or when a door sensor opens."
            // The only way to start was a ghost "New rule" link in the header, two lines above a card that had just
            // explained what a rule is. An empty state that ends without an action ends with the customer's guess.
            action={
              homeId ? (
                <Button label="Create your first rule" onPress={() => setCreating(true)} />
              ) : null
            }
            testID="automations-empty"
          />
        ) : null}

        {(rules ?? []).map((rule) => (
          <Card key={rule.id} accent={rule.enabled ? color.accent : undefined} padded={false}>
            <View style={{ padding: tokens.space.lg, gap: tokens.space.md }}>
            <Box row gap="md" style={{ alignItems: 'center' }}>
              <Box flex={1} gap="xs">
                <AppText weight="600">{rule.name}</AppText>
                <AppText variant="caption" tone="muted">
                  {describe(rule, (id) => (homeDevices ?? []).find((d) => d.id === id)?.name)}
                </AppText>
                {/* "Did it run?" is the first question about a rule that did not do what somebody expected, and the
                    answer used to cost a trip into the history. Silent while the rule has never fired — a rule created
                    a minute ago is not news. */}
                {rule.last_run_at ? (
                  <AppText variant="tiny" tone="muted">
                    Last ran {ago(rule.last_run_at)}
                  </AppText>
                ) : null}
              </Box>
              <Switch
                value={rule.enabled}
                // A bare switch in a row is announced as "switch, on" with no idea which rule it belongs to — the a11y
                // gate caught exactly that once the demo home had a rule to render.
                accessibilityLabel={`${rule.name}, ${rule.enabled ? 'on' : 'off'}`}
                onValueChange={(enabled) => toggle.mutate({ id: rule.id, enabled })}
                trackColor={{ true: color.accent, false: color.border }}
              />
            </Box>

            {/* Wraps, because at 200% zoom three 44-point controls no longer fit on one line: the a11y gate found
                "Delete" past the right edge, which is a control nobody can press with a thumb. */}
            <Box row gap="md" style={{ flexWrap: 'wrap' }}>
              {/* The only way to see a rule work used to be waiting for its trigger. This runs it now,
                  through the scheduler's own door, and the history below shows what it did — including
                  "skipped" and why, which is the answer a person testing a rule needs. */}
              <TouchText
                label={`Run ${rule.name} now`}
                hint="Runs this rule once, through the same path the schedule uses"
                disabled={!rule.enabled || run.isPending}
                onPress={() => run.mutate(rule.id)}
                testID={`run-${rule.id}`}
              >
                <AppText
                  variant="caption"
                  style={{ color: rule.enabled ? color.accentInk : color.muted }}
                >
                  {run.isPending ? 'Running…' : 'Run now'}
                </AppText>
              </TouchText>
              {/* No `accessibilityRole` meant no role in the DOM either: a screen reader announced this as
                  text, not as the control it is. */}
              <TouchText
                onPress={() => setOpen(open === rule.id ? null : rule.id)}
                label={`${open === rule.id ? 'Hide' : 'Show'} the run history for ${rule.name}`}
                testID={`history-${rule.id}`}
              >
                <AppText variant="caption" tone="accent">
                  {open === rule.id ? 'Hide history' : 'History'}
                </AppText>
              </TouchText>
              {/* Delete asks first, and says what goes with it: the rule's run history is CASCADE in the
                  database, so "delete" removes the evidence of what it did as well as the rule. It also sits
                  a centimetre from "Run now" and "History" on a screen used with a thumb. */}
              <TouchText
                destructive
                label={`Delete ${rule.name}`}
                onPress={() =>
                  ask({
                    title: `Delete “${rule.name}”?`,
                    message: 'The devices keep their current settings; the rule stops running. Its run history goes with it.',
                    confirmLabel: 'Delete',
                    destructive: true,
                    onConfirm: () => remove.mutate(rule.id),
                  })
                }
                disabled={remove.isPending}
              >
                <AppText variant="caption" tone="danger">
                  Delete
                </AppText>
              </TouchText>
            </Box>

            {open === rule.id ? <History rule={rule} /> : null}
            </View>
          </Card>
        ))}

        {toggle.error instanceof Error ? (
          <AppText variant="caption" tone="danger">
            {toggle.error.message}
          </AppText>
        ) : null}
      </ScreenScroll>
    </Screen>
  );
}

function History({ rule }: { rule: Automation }) {
  const { data: runs, isLoading } = useAutomationRuns(rule.id);

  if (isLoading) return <Skeleton height={13} width="60%" />;
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
/** The conditions a person can pick, and the one they get by not picking any. */
export type ConditionChoice = 'any' | 'home' | 'away' | 'sunset' | 'sunrise' | 'window';

/**
 * `HH:MM`, 24-hour, or `null`.
 *
 * The engine takes a wall-clock window in the home's timezone and the editor never offered one — "only between 22:00
 * and 06:00" is the second most common rule there is, after "at sunset". Parsing here rather than trusting the field
 * means a half-typed time cannot be sent as `"2"` and evaluated as nothing.
 */
export function clockValue(text: string): string | null {
  const match = /^([01]?\d|2[0-3]):([0-5]\d)$/.exec(text.trim());
  if (!match) return null;
  return `${match[1]!.padStart(2, '0')}:${match[2]!}`;
}

/**
 * The chips, and the reason a home without a location sees fewer of them.
 *
 * `Condition::Solar` is evaluated against the home's own sunrise and sunset; with no coordinates the engine
 * returns `false`, so a rule with "after sunset" would silently never run. Offering a control that can only
 * fail is worse than not offering it — the customer would conclude the automation is broken.
 */
export function conditionOptions(hasLocation: boolean): { id: string; label: string }[] {
  const options = [
    // "Only if… any time" is a tautology; the first chip means "no condition at all".
    { id: 'any', label: 'No condition' },
    { id: 'home', label: 'Someone is home' },
    { id: 'away', label: 'Nobody is home' },
    { id: 'window', label: 'Only between two times' },
  ];
  if (hasLocation) {
    options.push({ id: 'sunset', label: 'After sunset' }, { id: 'sunrise', label: 'After sunrise' });
  }
  return options;
}

/** The wire shape the engine expects, from the chip the person pressed. */
export function conditionFor(choice: ConditionChoice, window?: { after: string; before: string }): Condition[] {
  switch (choice) {
    case 'window':
      // Refused rather than sent half-built: the caller validates with `clockValue` first.
      return window ? [{ type: 'time_of_day', after: window.after, before: window.before }] : [];
    case 'home':
      return [{ type: 'present', presence: 'home' }];
    case 'away':
      return [{ type: 'present', presence: 'away' }];
    case 'sunset':
      return [{ type: 'solar', event: 'sunset', offset_min: 0 }];
    case 'sunrise':
      return [{ type: 'solar', event: 'sunrise', offset_min: 0 }];
    default:
      return [];
  }
}

function NewRuleForm({ homeId, onDone }: { homeId: string; onDone: () => void }) {
  const { color } = useTheme();
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
  // "Only if" — the second half of the sentence the engine has always been able to evaluate and the editor
  // has always sent as an empty array.
  const [condition, setCondition] = useState<ConditionChoice>('any');
  const [windowAfter, setWindowAfter] = useState('22:00');
  const [windowBefore, setWindowBefore] = useState('06:00');
  const { data: homes } = useHomes();
  const home = homes?.find((h) => h.id === homeId);
  const hasLocation = typeof home?.geo_lat === 'number' && typeof home?.geo_lon === 'number';

  const points = (id: string | null) =>
    (devices ?? []).find((d) => d.id === id)?.points?.filter((p) => p.writable) ?? [];

  // From the theme, not from `tokens.bootPalette`: the legacy name made these fields black boxes with white
  // text in the light appearance, on a screen nobody had opened in light mode with the editor showing.
  const field = {
    backgroundColor: color.elevated,
    borderWidth: 1,
    borderColor: color.border,
    borderRadius: tokens.radius.md,
    padding: tokens.space.md,
    color: color.text,
    fontSize: tokens.font.body,
  } as const;

  const clockOk =
    condition !== 'window' || Boolean(clockValue(windowAfter) && clockValue(windowBefore));

  const canSave =
    name.trim().length > 0 &&
    target !== null &&
    targetCode !== null &&
    clockOk &&
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
          conditions: conditionFor(condition, {
            after: clockValue(windowAfter) ?? windowAfter,
            before: clockValue(windowBefore) ?? windowBefore,
          }),
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
        placeholderTextColor={color.placeholder}
        value={name}
        onChangeText={setName}
        style={field}
      />

      {/* "When" was a ghost text button beside a full-width primary one — two different controls from two
          different design systems, for one either/or question. It is a chip row like every other choice here. */}
      <ChipPicker
        label="When"
        value={kind}
        options={[
          { id: 'schedule', label: 'Every few minutes' },
          { id: 'device_state', label: 'A device changes' },
        ]}
        onChange={(next) => setKind(next === 'device_state' ? 'device_state' : 'schedule')}
      />

      {/* The engine has evaluated conditions since the first version; the editor sent `conditions: []`, so
          "turn the light on when motion is detected, but only if someone is home" was a curl call. Presence and
          day/night need no picker and no value, which is why they are the two offered first. */}
      <ChipPicker
        label="Only if"
        value={condition}
        options={conditionOptions(hasLocation)}
        onChange={(next) => setCondition(next as ConditionChoice)}
      />
      {condition === 'window' ? (
        <Box gap="sm">
          <Box row gap="sm">
            <Box flex={1} gap="xs">
              <AppText variant="tiny" tone="muted">
                From
              </AppText>
              <TextInput
                value={windowAfter}
                onChangeText={setWindowAfter}
                placeholder="22:00"
                placeholderTextColor={color.placeholder}
                accessibilityLabel="From time"
                style={field}
              />
            </Box>
            <Box flex={1} gap="xs">
              <AppText variant="tiny" tone="muted">
                To
              </AppText>
              <TextInput
                value={windowBefore}
                onChangeText={setWindowBefore}
                placeholder="06:00"
                placeholderTextColor={color.placeholder}
                accessibilityLabel="To time"
                style={field}
              />
            </Box>
          </Box>
          <AppText variant="tiny" tone="muted">
            {clockValue(windowAfter) && clockValue(windowBefore)
              ? `Runs only between ${clockValue(windowAfter)} and ${clockValue(windowBefore)}, ${home?.timezone ?? 'the home\'s timezone'}. If the “to” time is earlier than the “from” time, the window runs overnight.`
              : 'Times are 24-hour, like 22:00.'}
          </AppText>
        </Box>
      ) : null}

      {!hasLocation ? (
        <AppText variant="tiny" tone="muted">
          This home has no location set, so sunrise and sunset are not available here.
        </AppText>
      ) : null}

      {kind === 'schedule' ? (
        <Box gap="xs">
          {/* A bare "5" under a schedule is a number with no unit. */}
          <AppText variant="tiny" tone="muted" style={{ letterSpacing: 1.1 }}>
            EVERY (MINUTES)
          </AppText>
          <TextInput
            placeholder="5"
            placeholderTextColor={color.placeholder}
            keyboardType="number-pad"
            value={minutes}
            onChangeText={setMinutes}
            accessibilityLabel="Minutes between runs"
            style={field}
          />
        </Box>
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
          {source ? (
            points(source).length === 0 ? (
              <AppText variant="caption" tone="muted">
                This device has nothing a rule can watch.
              </AppText>
            ) : (
              <ChipPicker
                label="What should it watch"
                value={sourceCode}
                options={points(source).map((p) => ({ id: p.code, label: readablePoint(p.code, p) }))}
                onChange={(code) => {
                  setSourceCode(code);
                  setSourceValue(defaultPointValue(points(source).find((p) => p.code === code)));
                }}
              />
            )
          ) : (
            <AppText variant="caption" tone="muted">
              Pick the device whose change should start the rule.
            </AppText>
          )}
          {sourceCode ? (
            <ValueField
              label="WHEN IT BECOMES"
              point={points(source).find((p) => p.code === sourceCode) ?? {}}
              value={sourceValue}
              onChange={setSourceValue}
            />
          ) : null}
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
      {target ? (
        points(target).length === 0 ? (
          <AppText variant="caption" tone="muted">
            This device has nothing a rule can change.
          </AppText>
        ) : (
          <ChipPicker
            label="What should happen"
            value={targetCode}
            options={points(target).map((p) => ({ id: p.code, label: readablePoint(p.code, p) }))}
            onChange={(code) => {
              setTargetCode(code);
              setTargetValue(defaultPointValue(points(target).find((p) => p.code === code)));
            }}
          />
        )
      ) : (
        <AppText variant="caption" tone="muted">
          Pick the device the rule should change.
        </AppText>
      )}
      {targetCode ? (
        <ValueField
          point={points(target).find((p) => p.code === targetCode) ?? {}}
          value={targetValue}
          onChange={setTargetValue}
        />
      ) : null}

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
