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

/** A member may control; an admin may change things; the owner may delete. */
export function canManage(role: Role): boolean {
  return role === 'admin' || role === 'owner';
}
