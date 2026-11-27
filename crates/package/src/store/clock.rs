use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn now() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|span| span.as_secs())
        .unwrap_or(0);
    render(seconds)
}

pub(crate) fn render(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64 + 719_468;
    let clock = seconds % 86_400;
    let era = days.div_euclid(146_097);
    let cycle = days.rem_euclid(146_097) as u64;
    let ordinal = (cycle - cycle / 1_460 + cycle / 36_524 - cycle / 146_096) / 365;
    let mut year = ordinal as i64 + era * 400;
    let remainder = cycle - (365 * ordinal + ordinal / 4 - ordinal / 100);
    let shifted = (5 * remainder + 2) / 153;
    let day = remainder - (153 * shifted + 2) / 5 + 1;
    let month = if shifted < 10 {
        shifted + 3
    } else {
        shifted - 9
    };
    if month <= 2 {
        year += 1;
    }
    let hour = clock / 3_600;
    let minute = (clock % 3_600) / 60;
    let second = clock % 60;
    format!("{year:04}{month:02}{day:02}T{hour:02}{minute:02}{second:02}Z")
}

#[cfg(test)]
mod tests {
    use super::render;

    #[test]
    fn calendar() {
        assert_eq!(render(0), "19700101T000000Z");
        assert_eq!(render(1_369_353_600), "20130524T000000Z");
        assert_eq!(render(1_709_251_199), "20240229T235959Z");
        assert_eq!(render(1_795_785_348), "20261127T131548Z");
    }
}
