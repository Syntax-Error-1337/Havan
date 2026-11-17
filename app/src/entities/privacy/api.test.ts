import { describeErasure, describeExport, exportFilename, type ExportBundle } from './api';

const bundle: ExportBundle = {
  format_version: 1,
  exported_at: '2026-11-17T09:20:11Z',
  user: { id: 'u1', email: 'demo@haven.local' },
  homes: [{}, {}],
  notifications: [{}],
  telemetry: [{}, {}, {}],
  notes: ['vendor-held licence records are not yours to see'],
};

/**
 * What the person is told about their own export.
 *
 * The file name and the summary are the two things a screen can get wrong without anyone noticing: a
 * date that does not match the server's, or a count that says "1 readings".
 */
describe('the export a person downloads', () => {
  it('is named the way the server names it', () => {
    expect(exportFilename(bundle)).toBe('haven-export-20261117.json');
  });

  it('falls back to today when the timestamp is unreadable, rather than producing "undefined"', () => {
    const broken = { ...bundle, exported_at: '' };
    expect(exportFilename(broken, new Date('2026-01-02T00:00:00Z'))).toBe(
      'haven-export-20260102.json',
    );
  });

  it('describes its contents in whole words', () => {
    expect(describeExport(bundle)).toBe('2 homes · 3 readings · 1 notification');
  });

  it('says "1 reading", not "1 readings"', () => {
    expect(describeExport({ ...bundle, telemetry: [{}] })).toContain('1 reading ·');
  });
});

describe('the receipt for a deleted account', () => {
  it('says what went and what stays, in one sentence', () => {
    const words = describeErasure({
      homes_deleted: 1,
      devices_deleted: 5,
      memberships_removed: 1,
      notifications_deleted: 3,
      sessions_revoked: 2,
      push_tokens_removed: 1,
      retention_days: 30,
      purge_after: '2026-12-17T00:00:00Z',
    });

    expect(words).toBe('1 home and 5 devices deleted · 2 sessions signed out · a receipt kept for 30 days');
  });
});
