import { freshness, silenceWords, SILENT_AFTER_MS } from './silence';

const at = (msAgo: number, now = 1_700_000_000_000) => new Date(now - msAgo).toISOString();
const NOW = 1_700_000_000_000;

describe('freshness', () => {
  it('takes the platform at its word when it says the device is offline', () => {
    expect(freshness({ online: false, last_seen_at: at(1000, NOW) }, NOW)).toBe('offline');
  });

  it('calls a device that reported a minute ago live', () => {
    expect(freshness({ online: true, last_seen_at: at(60_000, NOW) }, NOW)).toBe('live');
  });

  it('calls an "online" device silent once the threshold has passed', () => {
    expect(freshness({ online: true, last_seen_at: at(SILENT_AFTER_MS + 1000, NOW) }, NOW)).toBe(
      'stale',
    );
  });

  it('does not invent a problem when the server sends no timestamp', () => {
    // An older server, or a device that has never reported: no evidence either way, so no warning.
    expect(freshness({ online: true, last_seen_at: null }, NOW)).toBe('live');
    expect(freshness({ online: true }, NOW)).toBe('live');
  });

  it('does not cry wolf over a timestamp it cannot parse', () => {
    expect(freshness({ online: true, last_seen_at: 'yesterday-ish' }, NOW)).toBe('live');
  });
});

describe('silenceWords', () => {
  it('says nothing about a device heard from a minute ago', () => {
    expect(silenceWords({ online: true, last_seen_at: at(60_000, NOW) }, NOW)).toBeNull();
  });

  it('counts minutes, then hours, then days', () => {
    expect(silenceWords({ online: false, last_seen_at: at(5 * 60_000, NOW) }, NOW)).toBe(
      'Silent for 5m',
    );
    expect(silenceWords({ online: false, last_seen_at: at(4 * 3_600_000, NOW) }, NOW)).toBe(
      'Silent for 4h',
    );
    expect(silenceWords({ online: false, last_seen_at: at(3 * 86_400_000, NOW) }, NOW)).toBe(
      'Silent for 3d',
    );
  });

  it('has nothing to say without a timestamp', () => {
    expect(silenceWords({ online: false }, NOW)).toBeNull();
  });
});
