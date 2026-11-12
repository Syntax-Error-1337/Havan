import { useQuery } from '@tanstack/react-query';

import { request } from '../../../shared/api/client';
import { qk } from '../../../shared/api/keys';

export type Me = {
  id: string;
  email: string | null;
  display_name: string;
  locale: string;
};

/** Who is signed in. The settings screen needs a name to show, and the token only
 *  carries an id — asking the server is cheaper than decoding a JWT locally. */
export function useMe() {
  return useQuery({
    queryKey: qk.me,
    queryFn: () => request<Me>('/me'),
    staleTime: 5 * 60_000,
  });
}
