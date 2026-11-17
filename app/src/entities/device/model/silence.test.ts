import {
  expectedReportWords,
  freshness,
  quietAfterWords,
  silenceThresholdMs,
  silenceWords,
  SILENT_AFTER_MS,
} from './silence';

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

describe('a product that declares how often it reports', () => {
  it('uses the default when the product never said', () => {
    expect(silenceThresholdMs({})).toBe(SILENT_AFTER_MS);
    expect(silenceThresholdMs({ report_interval_secs: null })).toBe(SILENT_AFTER_MS);
    expect(silenceThresholdMs({ report_interval_secs: 0 })).toBe(SILENT_AFTER_MS);
  });

  it('gives an hourly sensor an hour, not a quarter of one', () => {
    // Three intervals: one missed report is normal, three is a pattern.
    expect(silenceThresholdMs({ report_interval_secs: 3600 })).toBe(3 * 3_600_000);

    const hourly = { online: true, last_seen_at: at(40 * 60_000, NOW), report_interval_secs: 3600 };
    expect(freshness(hourly, NOW)).toBe('live');
  });

  it('still catches a chatty product, and never below the floor', () => {
    // A device that reports every ten seconds is silent after a minute... but the floor is 15 minutes,
    // because a phone that has been asleep should not wake up to a wall of warnings.
    expect(silenceThresholdMs({ report_interval_secs: 10 })).toBe(SILENT_AFTER_MS);
    expect(freshness({ online: true, last_seen_at: at(20 * 60_000, NOW), report_interval_secs: 10 }, NOW)).toBe(
      'stale',
    );
  });
});

describe('the promise behind the silence', () => {
  it('says nothing when the product never declared one', () => {
    expect(expectedReportWords({})).toBeNull();
    expect(expectedReportWords({ report_interval_secs: 0 })).toBeNull();
  });

  it('names the interval in the unit a person would use', () => {
    // Phrased as a habit, not a form field: "Reports hourly" rather than "every 1 hour".
    expect(expectedReportWords({ report_interval_secs: 30 })).toBe('Reports every 30 seconds');
    expect(expectedReportWords({ report_interval_secs: 60 })).toBe('Reports every minute');
    expect(expectedReportWords({ report_interval_secs: 600 })).toBe('Reports every 10 minutes');
    expect(expectedReportWords({ report_interval_secs: 3600 })).toBe('Reports hourly');
    expect(expectedReportWords({ report_interval_secs: 86_400 })).toBe('Reports daily');
  });

  it('says how late is too late, from the same threshold', () => {
    expect(quietAfterWords({})).toBe('15m');
    expect(quietAfterWords({ report_interval_secs: 3600 })).toBe('3h');
    expect(quietAfterWords({ report_interval_secs: 86_400 })).toBe('72h');
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

/**
 * The heartbeat is not the reading. A sensor can hold a perfectly good connection and report on an
 * interval, so a device whose socket is live can be carrying a twelve-hour-old number — and that is what a
 * green dot beside "Updated 12h ago" was: two facts from two different clocks, both drawn as "now".
 */
describe('a reading older than the connection', () => {
  const NOW = Date.parse('2026-11-17T22:00:00Z');
  const minutesAgo = (m: number): string => new Date(NOW - m * 60_000).toISOString();

  const sensor = {
    online: true,
    last_seen_at: minutesAgo(1),
    report_interval_secs: 300,
    points: [
      { writable: false, value: 21.5, updated_at: minutesAgo(12 * 60) },
      { writable: false, value: 87, updated_at: minutesAgo(30) },
    ],
  };

  it('counts the device stale when its oldest reading is old', () => {
    expect(freshness(sensor, NOW)).toBe('stale');
    expect(silenceWords(sensor, NOW)).toBe('Silent for 12h');
  });

  it('stays live while every reading is inside the window', () => {
    const fresh = [{ writable: false, value: 87, updated_at: minutesAgo(5) }];
    expect(freshness({ ...sensor, points: fresh }, NOW)).toBe('live');
  });

  it('ignores a writable point: a bulb that has been off for days is off, not silent', () => {
    const bulb = {
      online: true,
      last_seen_at: minutesAgo(1),
      points: [{ writable: true, value: false, updated_at: minutesAgo(72 * 60) }],
    };
    expect(freshness(bulb, NOW)).toBe('live');
  });

  it('falls back to the heartbeat when no point carries a time', () => {
    const old = { online: true, last_seen_at: minutesAgo(240), points: [{ writable: false, value: 3 }] };
    expect(freshness(old, NOW)).toBe('stale');
  });
});
