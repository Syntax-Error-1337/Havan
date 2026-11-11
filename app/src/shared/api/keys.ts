/**
 * Query keys are tuples from one factory. A typo'd key is a cache that never
 * invalidates, which looks like a backend bug for a week.
 */
export const qk = {
  me: ['me'] as const,
  homes: ['homes'] as const,
  home: (id: string) => ['home', id] as const,
  dashboard: (homeId: string) => ['home', homeId, 'dashboard'] as const,
  devices: (homeId: string) => ['home', homeId, 'devices'] as const,
  device: (id: string) => ['device', id] as const,
  history: (deviceId: string, code: string, range: string) =>
    ['device', deviceId, 'history', code, range] as const,
  automations: (homeId: string) => ['home', homeId, 'automations'] as const,
  capabilities: ['catalog', 'capabilities'] as const,
};
