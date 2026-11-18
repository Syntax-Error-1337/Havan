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

/** What a rule does, in one line, for the list. */
export function describe(automation: Automation): string {
  const { trigger, actions } = automation.definition;

  const when =
    trigger.type === 'schedule'
      ? trigger.every_secs % 60 === 0
        ? `Every ${trigger.every_secs / 60} min`
        : `Every ${trigger.every_secs}s`
      : trigger.type === 'device_state'
        ? `When ${trigger.code.split('.').pop()} ${
            trigger.op === 'eq' ? 'is' : trigger.op === 'gt' ? '>' : '<'
          } ${String(trigger.value)}`
        : 'Manual';

  const then = actions
    .map((a) => (a.type === 'set_device' ? `set ${a.code}` : 'notify'))
    .join(', ');

  // The condition is half the sentence. A rule that says "Every 5 min → set switch.power" and silently only
  // runs when somebody is home is a rule nobody can debug from the list.
  const only = (automation.definition.conditions ?? []).map(describeCondition).filter(Boolean).join(' and ');

  return `${when}${only ? `, if ${only}` : ''} → ${then}`;
}
