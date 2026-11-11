export type Firmware = {
  id: string;
  product_id: string;
  version: string;
  size: number;
  sha256: string;
  ed25519_sig: string;
  object_key: string;
  created_at: string;
};

export type CampaignStatus = 'draft' | 'running' | 'paused' | 'halted' | 'completed';

export type Campaign = {
  id: string;
  vendor_id: string;
  product_id: string;
  firmware_id: string;
  rollout_pct: number;
  status: CampaignStatus;
  created_at: string;
  started_at: string | null;
};

export type Progress = {
  total: number;
  pending: number;
  sent: number;
  downloading: number;
  verified: number;
  rebooting: number;
  success: number;
  failed: number;
  skipped: number;
};

export type CampaignRow = {
  campaign: Campaign;
  progress: Progress;
  complete: boolean;
};

/** How far a rollout has actually got, 0–100. */
export function percentDone(progress: Progress): number {
  if (progress.total === 0) return 0;
  const finished = progress.success + progress.failed + progress.skipped;
  return Math.round((finished / progress.total) * 100);
}

/** What a vendor should look at first: failures, then work still moving. */
export function headline(row: CampaignRow): string {
  const p = row.progress;
  if (p.total === 0) return 'Matched no devices';
  if (p.failed > 0) return `${p.failed} failed`;
  if (row.complete) return 'Done';
  return `${p.success} of ${p.total} updated`;
}
