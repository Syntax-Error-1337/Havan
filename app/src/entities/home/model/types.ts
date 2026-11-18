export type Role = 'guest' | 'member' | 'admin' | 'owner';

export type Home = {
  id: string;
  owner_id: string;
  name: string;
  timezone: string;
  role: Role;
  member_count: number;
  device_count: number;
  /**
   * Where the home is, for rules that talk about sunrise and sunset.
   *
   * `null` is the honest answer for a home created before the app asked, and it is the reason the rule editor
   * only offers sun conditions where there is a location: a condition evaluated without coordinates is
   * `false`, forever, so offering it would be a control that can only fail.
   */
  geo_lat?: number | null;
  geo_lon?: number | null;
  created_at: string;
};

/**
 * A coordinate pair, as a person reads it.
 *
 * Four decimal places is about eleven metres — far more precision than a sunrise needs, and far less noise than
 * the float that comes back from the API.
 */
export function formatLocation(lat?: number | null, lon?: number | null): string {
  if (typeof lat !== 'number' || typeof lon !== 'number') return 'Not set';
  return `${lat.toFixed(4)}, ${lon.toFixed(4)}`;
}

/**
 * What a typed coordinate means, or null when it is not one.
 *
 * The service refuses a location it cannot use; saying so before the request is the difference between a form
 * that explains itself and one that fails after a round trip. Empty is `null` — "clear it", which is a legitimate
 * answer — and anything outside the planet's own bounds is refused.
 */
export function parseLocation(lat: string, lon: string): { lat: number; lon: number } | null | 'invalid' {
  const latText = lat.trim();
  const lonText = lon.trim();
  if (!latText && !lonText) return null;
  const latValue = Number(latText);
  const lonValue = Number(lonText);
  if (!latText || !lonText) return 'invalid';
  if (!Number.isFinite(latValue) || !Number.isFinite(lonValue)) return 'invalid';
  if (latValue < -90 || latValue > 90) return 'invalid';
  if (lonValue < -180 || lonValue > 180) return 'invalid';
  return { lat: latValue, lon: lonValue };
}

/** A member may control; an admin may change things; the owner may delete. */
export function canManage(role: Role): boolean {
  return role === 'admin' || role === 'owner';
}
