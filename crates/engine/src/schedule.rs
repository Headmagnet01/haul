//! Download time windows, and what to do when the queue is done.

use crate::error::EngineError;
use crate::store::Store;
use chrono::{Datelike, NaiveDateTime, NaiveTime, Weekday};
use serde::{Deserialize, Serialize};

/// Downloads run only between `start` and `stop` on the chosen weekdays.
/// `start > stop` is an overnight window; `start == stop` means all day.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Schedule {
    pub start: NaiveTime,
    pub stop: NaiveTime,
    pub weekdays: Vec<Weekday>,
}

/// For an overnight window the weekday that counts is the day the window started.
pub fn is_within_window(now: NaiveDateTime, s: &Schedule) -> bool {
    let (t, day) = (now.time(), now.weekday());
    let on = |d: Weekday| s.weekdays.contains(&d);
    if s.start == s.stop {
        on(day)
    } else if s.start < s.stop {
        on(day) && s.start <= t && t < s.stop
    } else {
        (t >= s.start && on(day)) || (t < s.stop && on(day.pred()))
    }
}

/// What the app does after the queue finishes. The engine only stores it;
/// the app carries it out on `Event::AllDone`.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub enum WhenDone {
    Nothing,
    Sleep,
    Shutdown,
}

const WHEN_DONE_KEY: &str = "when_done";

pub fn when_done(store: &Store) -> WhenDone {
    store
        .get_setting(WHEN_DONE_KEY)
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or(WhenDone::Nothing)
}

pub fn set_when_done(store: &Store, w: WhenDone) -> Result<(), EngineError> {
    store.set_setting(WHEN_DONE_KEY, &serde_json::to_value(w).expect("WhenDone serializes"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, NaiveDateTime, NaiveTime, Weekday};

    /// A date in ISO week 2 of 2030 on weekday `wd`, at hh:mm.
    fn at(wd: Weekday, h: u32, m: u32) -> NaiveDateTime {
        NaiveDate::from_isoywd_opt(2030, 2, wd).unwrap().and_hms_opt(h, m, 0).unwrap()
    }

    fn t(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    #[test]
    fn inside_day_window() {
        let s = Schedule { start: t(9, 0), stop: t(17, 0), weekdays: vec![Weekday::Mon] };
        assert!(is_within_window(at(Weekday::Mon, 12, 0), &s));
        assert!(!is_within_window(at(Weekday::Mon, 18, 0), &s));
        assert!(!is_within_window(at(Weekday::Tue, 12, 0), &s));
        assert!(is_within_window(at(Weekday::Mon, 9, 0), &s));
        assert!(!is_within_window(at(Weekday::Mon, 17, 0), &s));
    }

    #[test]
    fn overnight_window() {
        let s = Schedule { start: t(22, 0), stop: t(6, 0), weekdays: vec![Weekday::Fri] };
        assert!(is_within_window(at(Weekday::Fri, 23, 0), &s));
        assert!(is_within_window(at(Weekday::Sat, 5, 0), &s));
        assert!(!is_within_window(at(Weekday::Sat, 23, 0), &s));
        assert!(!is_within_window(at(Weekday::Fri, 5, 0), &s));
    }

    #[test]
    fn start_equals_stop_means_all_day() {
        let s = Schedule { start: t(0, 0), stop: t(0, 0), weekdays: vec![Weekday::Mon] };
        assert!(is_within_window(at(Weekday::Mon, 15, 0), &s));
        assert!(!is_within_window(at(Weekday::Tue, 15, 0), &s));
    }

    #[test]
    fn when_done_roundtrip() {
        let store = crate::store::Store::open_in_memory().unwrap();
        assert_eq!(when_done(&store), WhenDone::Nothing);
        set_when_done(&store, WhenDone::Shutdown).unwrap();
        assert_eq!(when_done(&store), WhenDone::Shutdown);
    }
}
