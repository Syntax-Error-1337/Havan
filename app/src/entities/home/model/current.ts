import { useEffect } from 'react';

import { useSession } from '../../session';
import { useHomes } from './queries';
import type { Home } from './types';

/**
 * The home this screen is about, whether or not one has been chosen yet.
 *
 * The session stores the chosen home, and it is empty on a fresh sign-in until *something* picks one — the
 * dashboard does it on mount, so every screen reached afterwards worked. Every screen reached *first* did
 * not: `/scenes` showed no scenes and no devices on a home with four of each, and `/settings` said
 * "Devices 0", because both asked the session a question the session had not answered yet.
 *
 * The rule is not "pick a default here" but "the app has already decided": with no stored choice the app
 * opens the first home, so that is the home these screens describe. The choice is then written down, so the
 * customer's next screen agrees with what they just saw.
 */
export function useCurrentHome(): {
  homeId: string | null;
  home: Home | undefined;
  isPending: boolean;
} {
  const stored = useSession((s) => s.homeId);
  const setHome = useSession((s) => s.setHome);
  const { data: homes, isPending } = useHomes();

  const home = homes?.find((h) => h.id === stored) ?? homes?.[0];
  const homeId = home?.id ?? null;

  useEffect(() => {
    if (!stored && homeId) void setHome(homeId);
  }, [stored, homeId, setHome]);

  return { homeId, home, isPending };
}
