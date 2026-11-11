export type Role = 'guest' | 'member' | 'admin' | 'owner';

export type Home = {
  id: string;
  owner_id: string;
  name: string;
  timezone: string;
  role: Role;
  member_count: number;
  device_count: number;
  created_at: string;
};

/** A member may control; an admin may change things; the owner may delete. */
export function canManage(role: Role): boolean {
  return role === 'admin' || role === 'owner';
}
