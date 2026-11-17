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

export type Freshness = 'live' | 'stale' | 'offline';

type Seen = { online: boolean; last_seen_at?: string | null; report_interval_secs?: number | null };

/** `offline` (the platform says so), `stale` (online but silent), or `live`. */
export function freshness(device: Seen, now: number = Date.now()): Freshness {
  if (!device.online) return 'offline';
  if (!device.last_seen_at) return 'live';
  const seen = new Date(device.last_seen_at).getTime();
  if (Number.isNaN(seen)) return 'live';
  return now - seen > silenceThresholdMs(device) ? 'stale' : 'live';
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
  if (!device.last_seen_at) return null;
  const seen = new Date(device.last_seen_at).getTime();
  if (Number.isNaN(seen)) return null;

  const minutes = Math.round((now - seen) / 60_000);
  if (minutes < 2) return null;
  if (minutes < 60) return `Silent for ${minutes}m`;
  if (minutes < 1440) return `Silent for ${Math.round(minutes / 60)}h`;
  return `Silent for ${Math.round(minutes / 1440)}d`;
}
