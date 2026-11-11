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

export type AutomationDef = {
  trigger: Trigger;
  conditions: unknown[];
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

  return `${when} → ${then}`;
}
