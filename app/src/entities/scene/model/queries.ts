import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { request } from '../../../shared/api/client';
import { qk } from '../../../shared/api/keys';

export type SceneAction = {
  type: 'set_device';
  device_id: string;
  code: string;
  value: unknown;
};

export type Scene = {
  id: string;
  home_id: string;
  name: string;
  icon: string | null;
  actions: SceneAction[];
  created_at: string;
};

export type Activation = {
  scene_id: string;
  dispatched: number;
  failed: { index: number; device_id: string; code: string; message: string }[];
};

export function useScenes(homeId: string | undefined) {
  return useQuery({
    queryKey: qk.scenes(homeId ?? 'none'),
    queryFn: () => request<Scene[]>(`/homes/${homeId}/scenes`),
    enabled: Boolean(homeId),
    staleTime: 60_000,
  });
}

export function useActivateScene(homeId: string | undefined) {
  const qc = useQueryClient();

  return useMutation({
    mutationFn: (sceneId: string) =>
      request<Activation>(`/scenes/${sceneId}/activate`, { method: 'POST' }),
    // A scene changes device state; the dashboard reads those from the device
    // cache, which the socket will also refresh — invalidating here means the
    // screen is right even if the socket is down.
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.devices(homeId ?? 'none') }),
  });
}

/** What to tell the person who pressed the button. */
export function outcome(activation: Activation): string {
  if (activation.failed.length === 0) {
    return activation.dispatched === 1 ? 'Done' : `Done · ${activation.dispatched} devices`;
  }
  if (activation.dispatched === 0) {
    return `Nothing happened: ${activation.failed[0]?.message ?? 'unknown error'}`;
  }
  return `${activation.dispatched} done · ${activation.failed.length} failed`;
}

export function useCreateScene(homeId: string | undefined) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: { name: string; actions: SceneAction[] }) =>
      request<Scene>(`/homes/${homeId}/scenes`, {
        method: 'POST',
        body: JSON.stringify(input),
      }),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.scenes(homeId ?? 'none') }),
  });
}

export function useDeleteScene(homeId: string | undefined) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => request<void>(`/scenes/${id}`, { method: 'DELETE' }),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.scenes(homeId ?? 'none') }),
  });
}
