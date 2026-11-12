//! Where the sun is, and therefore when it is dark.
//!
//! "Turn the hallway light on when motion is detected after sunset" is the example
//! the road map uses for the whole automation phase, and it needs this. The
//! algorithm is the standard NOAA approximation: accurate to about a minute, which
//! is two orders of magnitude better than a light switch needs, and it costs one
//! file and no dependency — a timezone database would be a much larger thing to
//! own for a worse answer.
//!
//! Times are **UTC minutes from midnight**. That is not a shortcut: sunrise is an
//! instant, and the instant is the same one in every timezone. A wall-clock
//! comparison would need the home's zone only because people think in wall clocks;
//! the sun does not.

use time::Date;

const DEG: f64 = std::f64::consts::PI / 180.0;

/// Sunrise and sunset for a date, in UTC minutes after midnight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Solar {
    pub sunrise: u32,
    pub sunset: u32,
}

/// The sun's daily arc at `lat`/`lon` on `date`.
///
/// `None` when the sun does not cross the horizon that day — polar day or polar
/// night. Callers must treat that as "no answer", never as midnight: a light that
/// switches on at 00:00 in Tromsø in June is a bug with a very long season.
pub fn solar_times(date: Date, lat: f64, lon: f64) -> Option<Solar> {
    if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        return None;
    }

    // Days since J2000.0, with the standard leap-day correction.
    let days = f64::from(date.to_julian_day()) - 2_451_545.0 + 0.0008;
    // Mean solar noon, corrected for longitude.
    let mean_noon = days - lon / 360.0;

    let mean_anomaly = (357.5291 + 0.985_600_28 * mean_noon).rem_euclid(360.0);
    let center = 1.9148 * (mean_anomaly * DEG).sin()
        + 0.02 * (2.0 * mean_anomaly * DEG).sin()
        + 0.0003 * (3.0 * mean_anomaly * DEG).sin();
    let ecliptic_longitude = (mean_anomaly + center + 180.0 + 102.9372).rem_euclid(360.0);

    let transit = 2_451_545.0 + mean_noon + 0.0053 * (mean_anomaly * DEG).sin()
        - 0.0069 * (2.0 * ecliptic_longitude * DEG).sin();

    let declination = (ecliptic_longitude * DEG).sin() * (23.44 * DEG).sin();
    let cos_hour_angle = (-0.833 * DEG).sin() - (lat * DEG).sin() * declination;
    let denominator = (lat * DEG).cos() * (1.0 - declination * declination).sqrt();
    if denominator.abs() < f64::EPSILON {
        return None;
    }
    let cos_hour_angle = cos_hour_angle / denominator;
    if !(-1.0..=1.0).contains(&cos_hour_angle) {
        return None;
    }

    let hour_angle = cos_hour_angle.acos().to_degrees();

    Some(Solar {
        sunrise: to_minutes(transit - hour_angle / 360.0),
        sunset: to_minutes(transit + hour_angle / 360.0),
    })
}

fn to_minutes(julian_day: f64) -> u32 {
    // `time`'s julian day is anchored at noon (JD 2451545.0 is 2000-01-01 12:00),
    // so half a day comes off before the fraction is read as a clock time. Without
    // this every solar time is 12 hours out — sunset at 08:23 in London in June.
    let minutes = ((julian_day - 0.5 - 2_451_545.0) * 1_440.0).round();
    // Julian days start at noon; wrap into a single day so the value is a clock
    // time rather than a number that grows.
    let wrapped = minutes.rem_euclid(1_440.0);
    wrapped as u32
}

/// Is `now_minutes` between sunrise and sunset?
pub fn is_daylight(solar: Solar, now_minutes: u32) -> bool {
    (solar.sunrise..solar.sunset).contains(&now_minutes)
}

/// The next time this solar event happens after `from`.
///
/// Used to arm a solar trigger: the worker asks "when is the next sunset?" once,
/// stores it, and the scheduler does the rest.
pub fn next_occurrence(
    from: time::OffsetDateTime,
    lat: f64,
    lon: f64,
    sunset: bool,
    offset_min: i32,
) -> Option<time::OffsetDateTime> {
    for day in 0..3 {
        let date = from.date() + time::Duration::days(day);
        let Some(solar) = solar_times(date, lat, lon) else {
            continue;
        };
        let minutes =
            i64::from(if sunset { solar.sunset } else { solar.sunrise }) + i64::from(offset_min);
        let midnight = date.midnight().assume_utc();
        let at = midnight + time::Duration::minutes(minutes);
        if at > from {
            return Some(at);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::date;

    /// Sunrise and sunset in UTC for a city, on a known day. The tolerance is two
    /// minutes: tighter would be asserting the algorithm's rounding, looser would
    /// let a real error through.
    fn assert_close(actual: u32, expected_minutes: u32) {
        let diff = actual.abs_diff(expected_minutes);
        assert!(diff <= 2, "expected ~{expected_minutes}, got {actual}");
    }

    #[test]
    fn london_midsummer() {
        // 21 June 2026: sunrise 03:43 UTC, sunset 20:21 UTC.
        let solar = solar_times(date!(2026 - 06 - 21), 51.5074, -0.1278).expect("london");
        assert_close(solar.sunrise, 3 * 60 + 43);
        assert_close(solar.sunset, 20 * 60 + 21);
    }

    #[test]
    fn london_midwinter() {
        // 21 December 2026: sunrise 08:04 UTC, sunset 15:53 UTC.
        let solar = solar_times(date!(2026 - 12 - 21), 51.5074, -0.1278).expect("london");
        assert_close(solar.sunrise, 8 * 60 + 4);
        assert_close(solar.sunset, 15 * 60 + 53);
    }

    #[test]
    fn the_equator_in_march_is_about_six_and_a_half_to_seven() {
        // Not six o'clock: in late March the equation of time puts solar noon at
        // ~12:07 UTC, so both ends of the day sit seven minutes late. A test that
        // expected the clock's six would be asserting a wrong answer politely.
        let solar = solar_times(date!(2026 - 03 - 21), 0.0, 0.0).expect("equator");
        assert_close(solar.sunrise, 6 * 60 + 7);
        assert_close(solar.sunset, 18 * 60 + 12);
    }

    #[test]
    fn the_poles_report_nothing_rather_than_midnight() {
        // Tromsø in June: the sun does not set.
        assert!(solar_times(date!(2026 - 06 - 21), 69.6492, 18.9553).is_none());
        // …and in December it does not rise.
        assert!(solar_times(date!(2026 - 12 - 21), 69.6492, 18.9553).is_none());
    }

    #[test]
    fn nonsense_coordinates_are_refused() {
        assert!(solar_times(date!(2026 - 06 - 21), 91.0, 0.0).is_none());
        assert!(solar_times(date!(2026 - 06 - 21), 0.0, 181.0).is_none());
    }

    #[test]
    fn day_and_night_flip_around_the_arc() {
        let solar = Solar {
            sunrise: 6 * 60,
            sunset: 18 * 60,
        };
        assert!(is_daylight(solar, 12 * 60));
        assert!(!is_daylight(solar, 5 * 60));
        assert!(!is_daylight(solar, 19 * 60));
    }

    #[test]
    fn the_next_sunset_is_tonight_or_tomorrow_night() {
        // 12:00 UTC in London on the longest day: sunset is still ahead.
        let noon = time::macros::datetime!(2026-06-21 12:00 UTC);
        let tonight = next_occurrence(noon, 51.5074, -0.1278, true, 0).expect("sunset");
        assert_eq!(tonight.date(), date!(2026 - 06 - 21));

        // 23:00 UTC: today's sunset has passed, so it must roll to tomorrow.
        let late = time::macros::datetime!(2026-06-21 23:00 UTC);
        let tomorrow = next_occurrence(late, 51.5074, -0.1278, true, 0).expect("sunset");
        assert_eq!(tomorrow.date(), date!(2026 - 06 - 22));
    }

    #[test]
    fn an_offset_shifts_the_armed_time() {
        let noon = time::macros::datetime!(2026-06-21 12:00 UTC);
        let base = next_occurrence(noon, 51.5074, -0.1278, true, 0).expect("sunset");
        let early = next_occurrence(noon, 51.5074, -0.1278, true, -30).expect("sunset");
        assert_eq!(base - early, time::Duration::minutes(30));
    }
}
