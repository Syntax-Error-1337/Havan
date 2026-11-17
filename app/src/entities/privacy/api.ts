import { Platform } from 'react-native';

import { request } from '../../shared/api/client';

/**
 * The two things a person is legally owed about their own data: a copy of it, and the ability to have
 * it deleted.
 *
 * The API has had both since Phase 6 (`GET /v1/me/export`, `DELETE /v1/me`) and the app had neither
 * screen, which meant the rights existed on paper and not in the product. This module is the client
 * half: it names the file the way the server does, describes it in words, and — on the phone — puts it
 * somewhere the person can actually keep.
 */

export type ExportBundle = {
  format_version: number;
  exported_at: string;
  user: Record<string, unknown>;
  homes: unknown[];
  notifications: unknown[];
  telemetry: unknown[];
  notes: string[];
};

export type ErasureReport = {
  homes_deleted: number;
  devices_deleted: number;
  memberships_removed: number;
  notifications_deleted: number;
  sessions_revoked: number;
  push_tokens_removed: number;
  retention_days: number;
  purge_after: string;
};

/** The same name the server would have sent, so a person's downloads do not disagree with each other. */
export function exportFilename(bundle: ExportBundle, now: Date = new Date()): string {
  const fromBundle = bundle.exported_at.slice(0, 10);
  const stamp = /^\d{4}-\d{2}-\d{2}$/.test(fromBundle)
    ? fromBundle.replace(/-/g, '')
    : now.toISOString().slice(0, 10).replace(/-/g, '');
  return `haven-export-${stamp}.json`;
}

/** What is inside the file, in the words of someone who is not going to open it. */
export function describeExport(bundle: ExportBundle): string {
  const parts = [
    `${count(bundle.homes.length, 'home')}`,
    `${count(bundle.telemetry.length, 'reading')}`,
    `${count(bundle.notifications.length, 'notification')}`,
  ];
  return parts.join(' · ');
}

function count(n: number, noun: string): string {
  return `${n} ${noun}${n === 1 ? '' : 's'}`;
}

/**
 * Save the bundle where the person can find it.
 *
 * Web downloads; native writes a file into the app's cache and offers the share sheet, because a phone
 * has no Downloads folder the app may write to. Both paths are deliberately thin — the decisions above
 * are what tests can hold on to.
 */
export async function saveExport(
  bundle: ExportBundle,
  filename = exportFilename(bundle),
): Promise<string> {
  const json = JSON.stringify(bundle, null, 2);

  if (Platform.OS === 'web') {
    const blob = new Blob([json], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = filename;
    anchor.click();
    // The URL would otherwise be held for the life of the document, one per download.
    URL.revokeObjectURL(url);
    return filename;
  }

  // Imported lazily: the web build has no file system, and a top-level import makes the bundle pay for
  // native-only modules on every page.
  const { File, Paths } = await import('expo-file-system');
  const { shareAsync } = await import('expo-sharing');

  const file = new File(Paths.cache, filename);
  file.write(json);

  // The share sheet is how a phone file leaves the app sandbox. If there is nowhere to share to — a
  // simulator with no targets — the file is still written, and the caller can say where it is.
  if (await shareAsync(file.uri, { mimeType: 'application/json' }).then(
    () => true,
    () => false,
  )) {
    return filename;
  }
  return file.uri;
}

export function fetchExport(): Promise<ExportBundle> {
  return request<ExportBundle>('/me/export');
}

export function eraseAccount(): Promise<ErasureReport> {
  return request<ErasureReport>('/me', { method: 'DELETE' });
}

/** The receipt, in a sentence: what went, and what stays for how long. */
export function describeErasure(report: ErasureReport): string {
  return [
    `${count(report.homes_deleted, 'home')} and ${count(report.devices_deleted, 'device')} deleted`,
    `${count(report.sessions_revoked, 'session')} signed out`,
    `a receipt kept for ${report.retention_days} day${report.retention_days === 1 ? '' : 's'}`,
  ].join(' · ');
}
