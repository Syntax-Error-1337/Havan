/**
 * Whether a device is *heard from*, as distinct from whether the platform believes it is online.
 *
 * A device that drops off Wi-Fi mid-session keeps its online flag until the broker's last will fires or
 * the presence sweep notices — which can be minutes, and after a partial network failure on a phone
 * hotspot has been seen to be hours. In that window the app happily shows yesterday's temperature as
 * this morning's, and the customer's conclusion is that the platform is lying rather than that the bulb
 * is asleep.
 *
 * So every screen that shows a device's state classifies it here, with one threshold, rather than each
 * deciding for itself what "recently" means.
 */

/**
 * The fallback threshold, for products that never declared how often they report.
 *
 * Long enough to cover a sleepy sensor; short enough to catch a real dropout.
 */
export const SILENT_AFTER_MS = 15 * 60_000;

/**
 * How late a report may be before the device counts as quiet: three of its own intervals.
 *
 * A product that says "I report every hour" is not silent at sixteen minutes; a product that says "every
 * ten seconds" is, and treating it like the hourly one would hide an outage for a quarter of an hour. The
 * platform never uses a shorter window than `SILENT_AFTER_MS`, so a chatty product does not make the app
 * twitchy — and a product that declares nothing falls back to the default.
 */
export function silenceThresholdMs(device: { report_interval_secs?: number | null }): number {
  const declared = device.report_interval_secs;
  if (!declared || !Number.isFinite(declared) || declared <= 0) return SILENT_AFTER_MS;
  return Math.max(SILENT_AFTER_MS, declared * 3 * 1000);
}

/**
 * "4h ago", from an ISO stamp. A silent device's age is the most useful thing about it, and every screen
 * that prints it should print it the same way — this lived in the hero card until the tile needed it, and
 * an entity importing from a widget is a dependency pointing the wrong way up the layers.
 */
export function relative(iso: string | null | undefined): string {
  if (!iso) return 'never';
  const ms = Date.now() - new Date(iso).getTime();
  if (Number.isNaN(ms) || ms < 0) return 'just now';
  const minutes = Math.round(ms / 60000);
  if (minutes < 2) return 'just now';
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.round(hours / 24)}d ago`;
}

export type Freshness = 'live' | 'stale' | 'offline';

type Seen = {
  online: boolean;
  last_seen_at?: string | null;
  report_interval_secs?: number | null;
  /** Points, when the caller has them: a sensor's reading can be older than the connection. */
  points?: { writable?: boolean; value?: unknown; updated_at?: string | null }[];
};

/**
 * The age of the oldest thing a screen is about to show as current, in milliseconds.
 *
 * `last_seen_at` is the device's heartbeat, and a sensor can hold a healthy connection while its reading
 * is half a day old — that is exactly the case that made a tile say "Updated 12h ago" beside a green dot
 * and an online count. Writable points are exempt: a bulb that has been off for three days is *off*, and
 * its age says nothing about whether the platform can reach it.
 */
export function readingAgeMs(device: Seen, now: number = Date.now()): number | null {
  const seen = device.last_seen_at ? new Date(device.last_seen_at).getTime() : NaN;
  let age = Number.isNaN(seen) ? null : now - seen;

  for (const point of device.points ?? []) {
    if (point.writable) continue;
    if (point.value === null || point.value === undefined) continue;
    const at = point.updated_at ? new Date(point.updated_at).getTime() : NaN;
    if (Number.isNaN(at)) continue;
    const pointAge = now - at;
    if (age === null || pointAge > age) age = pointAge;
  }
  return age;
}

/** `offline` (the platform says so), `stale` (online but silent), or `live`. */
export function freshness(device: Seen, now: number = Date.now()): Freshness {
  if (!device.online) return 'offline';
  const age = readingAgeMs(device, now);
  if (age === null) return 'live';
  return age > silenceThresholdMs(device) ? 'stale' : 'live';
}

/**
 * What the product said it would do, in words — or `null` when it never said.
 *
 * A customer reading "Silent for 4h" has no way to know whether that is a fault or the schedule. Naming
 * the promise turns a mystery into a fact: "expects a report every hour" explains why four hours is late
 * for one device and normal for another.
 */
export function expectedReportWords(device: {
  report_interval_secs?: number | null;
}): string | null {
  const secs = device.report_interval_secs;
  if (!secs || !Number.isFinite(secs) || secs <= 0) return null;

  // Capitalised and phrased as a habit rather than a schedule entry: "Reports hourly" is how a person
  // describes it, and "every 1 hour" is how a form does.
  if (secs < 60) return `Reports every ${secs} seconds`;
  const minutes = Math.round(secs / 60);
  if (minutes === 1) return 'Reports every minute';
  if (minutes < 60) return `Reports every ${minutes} minutes`;
  const hours = Math.round(minutes / 60);
  if (hours === 1) return 'Reports hourly';
  if (hours < 24) return `Reports every ${hours} hours`;
  const days = Math.round(hours / 24);
  return days === 1 ? 'Reports daily' : `Reports every ${days} days`;
}

/** The threshold in words, for the screen that explains it: "3h", "15m". */
export function quietAfterWords(device: { report_interval_secs?: number | null }): string {
  const minutes = Math.round(silenceThresholdMs(device) / 60_000);
  if (minutes < 60) return `${minutes}m`;
  return `${Math.round(minutes / 60)}h`;
}

/**
 * How long it has been quiet, in words: "Silent for 4h".
 *
 * Returns `null` when there is nothing worth saying — a device heard from a minute ago is not news, and
 * a line that always says something is a line people stop reading.
 */
export function silenceWords(device: Seen, now: number = Date.now()): string | null {
  const age = readingAgeMs(device, now);
  if (age === null) return null;

  const minutes = Math.round(age / 60_000);
  if (minutes < 2) return null;
  if (minutes < 60) return `Silent for ${minutes}m`;
  if (minutes < 1440) return `Silent for ${Math.round(minutes / 60)}h`;
  return `Silent for ${Math.round(minutes / 1440)}d`;
}
