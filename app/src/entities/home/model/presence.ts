import { useMutation, useQueryClient } from '@tanstack/react-query';

import { request } from '../../../shared/api/client';
import { qk } from '../../../shared/api/keys';

/**
 * "I am at home" / "I have left".
 *
 * Presence is per member and set by the person, not inferred: a geofence that guesses
 * wrong turns the hallway light on for an empty flat, and a user cannot argue with it.
 * The automation condition "nobody home" reads the aggregate of these answers.
 */
export function useSetPresence(homeId: string | undefined) {
  const qc = useQueryClient();

  return useMutation({
    mutationFn: (present: boolean) =>
      request<void>(`/homes/${homeId}/presence`, {
        method: 'POST',
        body: JSON.stringify({ present }),
      }),
    onSuccess: () => {
      // Any automation that looks at presence is now evaluated differently.
      void qc.invalidateQueries({ queryKey: qk.dashboard(homeId ?? 'none') });
    },
  });
}
