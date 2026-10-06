//! Gregorian firmware wall-clock conversion for certificate validity checks.
/// `timezone` is minutes east of UTC; an unspecified zone is supplied as zero.
pub fn unix_seconds(
    year: u16,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
    timezone: i16,
) -> Option<u64> {
    let year = i64::from(year) - i64::from(month <= 2);
    let month = i64::from(month);
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(day) - 1;
    let days = era * 146097 + year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year
        - 719468;
    let seconds =
        days * 86400 + i64::from(hour) * 3600 + i64::from(minute) * 60 + i64::from(second)
            - i64::from(timezone) * 60;
    seconds.try_into().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_day_matches_an_independent_counting_calendar_through_2101() {
        // The oracle walks each calendar day from the Unix epoch, without
        // using the closed-form production calendar algorithm.
        let mut seconds = 0;
        for year in 1970..=2101 {
            let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
            for (index, days) in [
                31,
                if leap { 29 } else { 28 },
                31,
                30,
                31,
                30,
                31,
                31,
                30,
                31,
                30,
                31,
            ]
            .into_iter()
            .enumerate()
            {
                for day in 1..=days {
                    assert_eq!(
                        unix_seconds(year, index as u8 + 1, day, 0, 0, 0, 0),
                        Some(seconds)
                    );
                    assert_eq!(
                        unix_seconds(year, index as u8 + 1, day, 23, 59, 59, 0),
                        Some(seconds + 86399)
                    );
                    seconds += 86400;
                }
            }
        }
        assert_eq!(unix_seconds(1970, 1, 1, 8, 0, 0, 480), Some(0));
        assert_eq!(unix_seconds(1970, 1, 1, 0, 0, 0, -480), Some(28800));
        assert_eq!(unix_seconds(1969, 12, 31, 23, 59, 59, 0), None);
    }
}
