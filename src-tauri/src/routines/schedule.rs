//! Routine schedules: parsing the CLI forms and computing the next fire.
//! Times are local wall-clock time.

use chrono::{DateTime, Datelike, Duration as ChronoDuration, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone};
use serde::{Deserialize, Serialize};

pub const MIN_EVERY_SECS: u64 = 10;
const DAYS: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Schedule {
    Every { secs: u64 },
    Daily { hour: u32, minute: u32 },
    /// days: 0 = Monday … 6 = Sunday
    Weekly { days: Vec<u32>, hour: u32, minute: u32 },
    /// "YYYY-MM-DD HH:MM", local time
    Once { at: String },
}

/// `45s`, `30m`, `2h`, `1h30m`; a bare number means minutes.
pub fn parse_every(s: &str) -> Result<u64, String> {
    let t = s.trim().to_lowercase();
    let bad = || format!("--every takes an interval like 45s, 30m, 2h or 1h30m (got '{}')", s);
    let secs = if let Ok(n) = t.parse::<u64>() {
        n * 60
    } else {
        let mut total = 0u64;
        let mut num = String::new();
        for c in t.chars() {
            if c.is_ascii_digit() {
                num.push(c);
                continue;
            }
            let n: u64 = num.parse().map_err(|_| bad())?;
            num.clear();
            total += n * match c {
                'h' => 3600,
                'm' => 60,
                's' => 1,
                _ => return Err(bad()),
            };
        }
        if !num.is_empty() {
            return Err(bad());
        }
        total
    };
    if secs < MIN_EVERY_SECS {
        return Err(format!("--every must be at least {}s", MIN_EVERY_SECS));
    }
    Ok(secs)
}

/// `HH:MM`, 24-hour.
pub fn parse_hm(s: &str) -> Result<(u32, u32), String> {
    NaiveTime::parse_from_str(s.trim(), "%H:%M")
        .map(|t| (chrono::Timelike::hour(&t), chrono::Timelike::minute(&t)))
        .map_err(|_| format!("expected a 24-hour time like 09:00 (got '{}')", s))
}

/// `mon,wed,fri@09:00`.
pub fn parse_weekly(s: &str) -> Result<Schedule, String> {
    let (days, time) = s
        .split_once('@')
        .ok_or_else(|| format!("--weekly takes days@time, e.g. mon,wed,fri@09:00 (got '{}')", s))?;
    let mut list = Vec::new();
    for d in days.split(',').map(|d| d.trim().to_lowercase()) {
        let i = DAYS
            .iter()
            .position(|x| *x == d)
            .ok_or_else(|| format!("unknown day '{}' — use mon, tue, wed, thu, fri, sat, sun", d))?;
        if !list.contains(&(i as u32)) {
            list.push(i as u32);
        }
    }
    list.sort();
    let (hour, minute) = parse_hm(time)?;
    Ok(Schedule::Weekly { days: list, hour, minute })
}

/// `YYYY-MM-DD HH:MM` (must be in the future when created).
pub fn parse_once(s: &str, now: DateTime<Local>) -> Result<Schedule, String> {
    let at = NaiveDateTime::parse_from_str(s.trim(), "%Y-%m-%d %H:%M")
        .map_err(|_| format!("--once takes \"YYYY-MM-DD HH:MM\" (got '{}')", s))?;
    let when = local(at).ok_or("that time doesn't exist in your time zone")?;
    if when <= now {
        return Err(format!("{} is already in the past", s.trim()));
    }
    Ok(Schedule::Once { at: s.trim().to_string() })
}

/// `YYYY-MM-DD` — the routine stops after that whole day.
pub fn parse_until(s: &str) -> Result<String, String> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
        .map(|_| s.trim().to_string())
        .map_err(|_| format!("--until takes YYYY-MM-DD (got '{}')", s))
}

fn local(ndt: NaiveDateTime) -> Option<DateTime<Local>> {
    Local.from_local_datetime(&ndt).earliest()
}

fn at_time(date: NaiveDate, hour: u32, minute: u32) -> Option<DateTime<Local>> {
    local(date.and_hms_opt(hour, minute, 0)?)
}

/// First fire strictly after `now` (None: nothing left, e.g. a past one-shot).
pub fn next_after(s: &Schedule, now: DateTime<Local>) -> Option<DateTime<Local>> {
    match s {
        Schedule::Every { secs } => Some(now + ChronoDuration::seconds(*secs as i64)),
        Schedule::Daily { hour, minute } => (0..=1)
            .filter_map(|d| at_time(now.date_naive() + ChronoDuration::days(d), *hour, *minute))
            .find(|t| *t > now),
        Schedule::Weekly { days, hour, minute } => (0..=7)
            .map(|d| now.date_naive() + ChronoDuration::days(d))
            .filter(|date| days.contains(&date.weekday().num_days_from_monday()))
            .filter_map(|date| at_time(date, *hour, *minute))
            .find(|t| *t > now),
        Schedule::Once { at } => NaiveDateTime::parse_from_str(at, "%Y-%m-%d %H:%M")
            .ok()
            .and_then(local)
            .filter(|t| *t > now),
    }
}

/// Is `now` past the end of the `until` day?
pub fn past_until(until: &str, now: DateTime<Local>) -> bool {
    NaiveDate::parse_from_str(until, "%Y-%m-%d")
        .ok()
        .and_then(|d| at_time(d + ChronoDuration::days(1), 0, 0))
        .is_some_and(|end| now >= end)
}

pub fn describe(s: &Schedule) -> String {
    let hm = |h: &u32, m: &u32| format!("{:02}:{:02}", h, m);
    match s {
        Schedule::Every { secs } => {
            let (h, m, sec) = (secs / 3600, secs % 3600 / 60, secs % 60);
            let mut out = String::from("every ");
            if h > 0 {
                out += &format!("{}h", h);
            }
            if m > 0 {
                out += &format!("{}m", m);
            }
            if sec > 0 {
                out += &format!("{}s", sec);
            }
            out
        }
        Schedule::Daily { hour, minute } => format!("daily {}", hm(hour, minute)),
        Schedule::Weekly { days, hour, minute } => {
            let names: Vec<&str> = days.iter().filter_map(|d| DAYS.get(*d as usize).copied()).collect();
            format!("weekly {}@{}", names.join(","), hm(hour, minute))
        }
        Schedule::Once { at } => format!("once {}", at),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> DateTime<Local> {
        local(NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap()).unwrap()
    }

    #[test]
    fn every_accepts_units_and_bare_minutes() {
        assert_eq!(parse_every("45s"), Ok(45));
        assert_eq!(parse_every("30m"), Ok(1800));
        assert_eq!(parse_every("2h"), Ok(7200));
        assert_eq!(parse_every("1h30m"), Ok(5400));
        assert_eq!(parse_every("5"), Ok(300));
        assert!(parse_every("5s").is_err());
        assert!(parse_every("10x").is_err());
        assert!(parse_every("1h30").is_err());
    }

    #[test]
    fn weekly_and_times_parse() {
        assert_eq!(
            parse_weekly("fri,mon,Wed@09:05"),
            Ok(Schedule::Weekly { days: vec![0, 2, 4], hour: 9, minute: 5 })
        );
        assert!(parse_weekly("mon").is_err());
        assert!(parse_weekly("funday@09:00").is_err());
        assert!(parse_hm("25:00").is_err());
    }

    #[test]
    fn once_must_be_future() {
        let now = t("2026-10-01 12:00");
        assert!(parse_once("2026-10-01 12:30", now).is_ok());
        assert!(parse_once("2026-10-01 11:00", now).unwrap_err().contains("past"));
        assert!(parse_once("tomorrow", now).is_err());
    }

    #[test]
    fn next_daily_rolls_to_tomorrow() {
        let s = Schedule::Daily { hour: 9, minute: 0 };
        assert_eq!(next_after(&s, t("2026-10-01 08:00")), Some(t("2026-10-01 09:00")));
        assert_eq!(next_after(&s, t("2026-10-01 09:00")), Some(t("2026-10-02 09:00")));
    }

    #[test]
    fn next_weekly_finds_the_next_listed_day() {
        // 2026-10-01 is a Thursday
        let s = parse_weekly("mon,thu@09:00").unwrap();
        assert_eq!(next_after(&s, t("2026-10-01 08:00")), Some(t("2026-10-01 09:00")));
        assert_eq!(next_after(&s, t("2026-10-01 10:00")), Some(t("2026-10-05 09:00")));
    }

    #[test]
    fn once_and_every() {
        let once = Schedule::Once { at: "2026-10-01 12:30".into() };
        assert_eq!(next_after(&once, t("2026-10-01 12:00")), Some(t("2026-10-01 12:30")));
        assert_eq!(next_after(&once, t("2026-10-01 13:00")), None);
        let every = Schedule::Every { secs: 90 };
        assert_eq!(next_after(&every, t("2026-10-01 12:00")), Some(t("2026-10-01 12:00") + ChronoDuration::seconds(90)));
    }

    #[test]
    fn until_covers_the_whole_day() {
        assert!(!past_until("2026-10-01", t("2026-10-01 23:59")));
        assert!(past_until("2026-10-01", t("2026-10-02 00:00")));
    }

    #[test]
    fn descriptions_read_like_the_flags() {
        assert_eq!(describe(&Schedule::Every { secs: 5400 }), "every 1h30m");
        assert_eq!(describe(&Schedule::Every { secs: 45 }), "every 45s");
        assert_eq!(describe(&parse_weekly("mon,fri@09:00").unwrap()), "weekly mon,fri@09:00");
        assert_eq!(describe(&Schedule::Daily { hour: 18, minute: 0 }), "daily 18:00");
    }
}
