import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { request } from '../../../shared/api/client';
import { qk } from '../../../shared/api/keys';

export type Trigger =
  | { type: 'schedule'; every_secs: number }
  | { type: 'device_state'; device_id: string; code: string; op: Op; value: unknown }
  | { type: 'manual' };

export type Op = 'eq' | 'gt' | 'lt' | 'gte' | 'lte';

export type Action =
  | { type: 'set_device'; device_id: string; code: string; value: unknown }
  | { type: 'notify'; message: string };

/** The conditions the engine evaluates before it runs the actions. */
export type Condition =
  | { type: 'device_state'; device_id: string; code: string; op: Op; value: unknown }
  | { type: 'time_of_day'; after: string; before: string }
  | { type: 'present'; presence: 'home' | 'away' }
  | { type: 'solar'; event: 'sunrise' | 'sunset'; offset_min: number };

export type AutomationDef = {
  trigger: Trigger;
  conditions: Condition[];
  actions: Action[];
};

export type Automation = {
  id: string;
  home_id: string;
  name: string;
  enabled: boolean;
  definition: AutomationDef;
  next_run_at: string | null;
  /**
   * When it last ran, whatever fired it.
   *
   * The API computes it in the list query; a screen that had to ask per row would be one request per rule on the one
   * screen people open when they are already unsure whether something works.
   */
  last_run_at?: string | null;
};

export type RunRecord = {
  id: string;
  trigger_kind: string;
  status: string;
  started_at: string;
  log: { steps: { kind: string; index?: number; detail: string }[] };
};

export function useAutomations(homeId: string | undefined) {
  return useQuery({
    queryKey: qk.automations(homeId ?? ''),
    queryFn: () => request<Automation[]>(`/homes/${homeId}/automations`),
    enabled: Boolean(homeId),
  });
}

export function useAutomationRuns(automationId: string | null) {
  return useQuery({
    queryKey: ['automation', automationId, 'runs'],
    queryFn: () => request<RunRecord[]>(`/automations/${automationId}/runs?limit=10`),
    enabled: Boolean(automationId),
  });
}

/**
 * Run a rule once, now.
 *
 * The rule is armed the same way the scheduler arms it, so the worker executes it on its next tick and
 * the run lands in the same history — including a `skipped` verdict when the conditions do not hold,
 * which is the answer somebody testing a rule actually wants. The history is refetched on a short delay
 * because the run happens in another process.
 */
export function useRunAutomation() {
  const qc = useQueryClient();

  return useMutation({
    mutationFn: (id: string) =>
      request<void>(`/automations/${id}/run`, { method: 'POST' }),
    onSuccess: (_data, id) => {
      // The worker's tick is measured in seconds; two refetches cover it without polling forever.
      void qc.invalidateQueries({ queryKey: qk.automations('') });
      setTimeout(() => {
        void qc.invalidateQueries({ queryKey: ['automation', id, 'runs'] });
      }, 1500);
      setTimeout(() => {
        void qc.invalidateQueries({ queryKey: ['automation', id, 'runs'] });
      }, 4000);
    },
  });
}

export function useToggleAutomation(homeId: string | undefined) {
  const qc = useQueryClient();

  return useMutation({
    mutationFn: (input: { id: string; enabled: boolean }) =>
      request<Automation>(`/automations/${input.id}`, {
        method: 'PATCH',
        body: JSON.stringify({ enabled: input.enabled }),
      }),
    // Optimistic: a switch that waits for a round trip feels broken. The
    // server's answer replaces the guess on settle, so a refusal self-corrects.
    onMutate: async ({ id, enabled }) => {
      const key = qk.automations(homeId ?? '');
      await qc.cancelQueries({ queryKey: key });
      const previous = qc.getQueryData<Automation[]>(key);
      qc.setQueryData<Automation[]>(key, (list) =>
        (list ?? []).map((a) => (a.id === id ? { ...a, enabled } : a)),
      );
      return { previous, key };
    },
    onError: (_error, _input, context) => {
      if (context?.previous) qc.setQueryData(context.key, context.previous);
    },
    onSettled: () => qc.invalidateQueries({ queryKey: qk.automations(homeId ?? '') }),
  });
}

export function useDeleteAutomation(homeId: string | undefined) {
  const qc = useQueryClient();

  return useMutation({
    mutationFn: (id: string) => request<void>(`/automations/${id}`, { method: 'DELETE' }),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.automations(homeId ?? '') }),
  });
}

export function useCreateAutomation(homeId: string | undefined) {
  const qc = useQueryClient();

  return useMutation({
    mutationFn: (input: { name: string; definition: AutomationDef }) =>
      request<Automation>(`/homes/${homeId}/automations`, {
        method: 'POST',
        body: JSON.stringify(input),
      }),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.automations(homeId ?? '') }),
  });
}

/** One condition, in the words a person used to ask for it. */
export function describeCondition(condition: Condition): string {
  switch (condition.type) {
    case 'present':
      return condition.presence === 'home' ? 'someone is home' : 'nobody is home';
    case 'solar':
      // Daylight, not astronomy: "after sunset" is the customer's sentence for "it is dark".
      return condition.event === 'sunset' ? 'it is dark' : 'it is light';
    case 'time_of_day':
      return `it is between ${condition.after} and ${condition.before}`;
    case 'device_state':
      return `${condition.code.split('.').pop()} is ${String(condition.value)}`;
    default:
      return '';
  }
}

/**
 * An interval in the words a person says it in.
 *
 * "Every 1440 min" is a number the API is happy with and a sentence nobody says: a daily rule is "Every day". Seconds
 * are only used below a minute, where they are the honest unit.
 */
export function humanEvery(secs: number): string {
  if (secs % 86_400 === 0) {
    const days = secs / 86_400;
    return days === 1 ? 'Every day' : `Every ${days} days`;
  }
  if (secs % 3_600 === 0) {
    const hours = secs / 3_600;
    return hours === 1 ? 'Every hour' : `Every ${hours} hours`;
  }
  if (secs % 60 === 0) return `Every ${secs / 60} min`;
  return `Every ${secs}s`;
}

/**
 * What a rule does, in one line, for the list.
 *
 * `nameOf` resolves a device id to what the customer calls it. Without it the line says "set switch.power", which is the
 * wire format of somebody else's firmware — the exact class of copy this project keeps finding on screens that were
 * written from the API outwards instead of from the customer inwards.
 */
export function describe(automation: Automation, nameOf?: (deviceId: string) => string | undefined): string {
  const { trigger, actions } = automation.definition;

  const when =
    trigger.type === 'schedule'
      ? humanEvery(trigger.every_secs)
      : trigger.type === 'device_state'
        ? `When ${trigger.code.split('.').pop()} ${
            trigger.op === 'eq' ? 'is' : trigger.op === 'gt' ? '>' : '<'
          } ${String(trigger.value)}`
        : 'Manual';

  const then = actions
    .map((a) => {
      if (a.type !== 'set_device') return 'send an alert';
      const device = nameOf?.(a.device_id);
      const point = a.code.split('.').pop() ?? a.code;
      // "turn Acme Bulb E27 on" reads like a sentence; "set switch.power" reads like a header dump.
      if (point === 'power') return `turn ${device ?? 'the device'} ${a.value === false || a.value === 'false' ? 'off' : 'on'}`;
      if (point === 'locked') return `${a.value === true || a.value === 'true' ? 'lock' : 'unlock'} ${device ?? 'the device'}`;
      return `set ${point} on ${device ?? 'the device'} to ${String(a.value)}`;
    })
    .join(', ');

  // The condition is half the sentence. A rule that says "Every 5 min → set switch.power" and silently only
  // runs when somebody is home is a rule nobody can debug from the list.
  const only = (automation.definition.conditions ?? []).map(describeCondition).filter(Boolean).join(' and ');

  return `${when}${only ? `, if ${only}` : ''} → ${then}`;
}
