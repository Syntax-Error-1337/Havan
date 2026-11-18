import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { request } from '../../shared/api/client';
import { qk } from '../../shared/api/keys';

/** The events a subscriber can choose. Kept in step with the worker's outbox. */
export const WEBHOOK_EVENTS = [
  'licence.activated',
  'device.claimed',
  'firmware.failed',
] as const;

export type WebhookEvent = (typeof WEBHOOK_EVENTS)[number];

export type WebhookDelivery = {
  at: string;
  delivered: boolean;
  attempts: number;
  error: string | null;
};

export type Webhook = {
  id: string;
  url: string;
  events: string[];
  active: boolean;
  created_at: string;
  /** The newest attempt, or null when nothing has ever been sent. An endpoint that has been refusing for a week looks
   *  exactly like one nobody has needed yet, and this is the column that tells them apart. */
  last_delivery: WebhookDelivery | null;
  /** Present only on the response that created the subscription. */
  secret?: string;
};

export function useWebhooks() {
  return useQuery({
    queryKey: qk.webhooks,
    queryFn: () => request<Webhook[]>('/vendor/webhooks'),
  });
}

/**
 * The signing secret is shown once, like an API key, and held in the mutation result rather than
 * the query cache so it does not survive a tab switch.
 */
export function useSubscribeWebhook() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: { url: string; events: string[] }) =>
      request<Webhook>('/vendor/webhooks', {
        method: 'POST',
        body: JSON.stringify(input),
      }),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.webhooks }),
  });
}

export function useUnsubscribeWebhook() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => request<void>(`/vendor/webhooks/${id}`, { method: 'DELETE' }),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.webhooks }),
  });
}
