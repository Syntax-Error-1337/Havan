import { create } from 'zustand';

import { secureStore } from '../../../shared/lib/secure';

/**
 * What this person chose, as opposed to what the platform chose.
 *
 * Appearance is the only preference so far, and it is the one that proves the pattern: the operator
 * sets a brand and a mode for everybody, and a customer is allowed to disagree about the mode. The
 * preference is stored beside the session (keychain on a phone, localStorage on the web) because it is
 * this device's answer, not the account's.
 *
 * `system` is deliberately the default: a phone set to dark at 9 p.m. is a phone whose owner has
 * already told every app what they want.
 */
export type Appearance = 'system' | 'light' | 'dark';

const KEY = 'haven.appearance';

type PreferencesState = {
  appearance: Appearance;
  hydrated: boolean;
  hydrate: () => Promise<void>;
  setAppearance: (appearance: Appearance) => Promise<void>;
};

export const usePreferences = create<PreferencesState>((set) => ({
  appearance: 'system',
  hydrated: false,

  hydrate: async () => {
    const stored = await secureStore.get(KEY);
    const appearance: Appearance =
      stored === 'light' || stored === 'dark' || stored === 'system' ? stored : 'system';
    set({ appearance, hydrated: true });
  },

  setAppearance: async (appearance) => {
    // State first: the screen repaints on the tap, and the write is what makes it survive a relaunch.
    set({ appearance });
    await secureStore.set(KEY, appearance);
  },
}));
