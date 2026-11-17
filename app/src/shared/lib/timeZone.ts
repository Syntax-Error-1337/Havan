import { getCalendars } from 'expo-localization';

/**
 * The IANA timezone of the device, for the home it is about to create.
 *
 * A home's timezone is not decoration: schedules and automations are evaluated in it, so a customer in
 * IST whose home was created as UTC gets their "7 am" at half past noon. The first-run screen used to
 * send a literal `'UTC'` for every person on the planet.
 *
 * Falls back to `UTC` rather than to a guess: a wrong zone is a schedule that fires at the wrong hour,
 * and an obviously-wrong one is a thing the settings screen lets somebody fix.
 */
export function deviceTimeZone(): string {
  try {
    const zone = getCalendars()[0]?.timeZone;
    if (typeof zone === 'string' && zone.length > 0) return zone;
  } catch {
    // `expo-localization` throws on a platform where the calendar data is unavailable. Not a reason to
    // refuse to create a home.
  }
  return 'UTC';
}
