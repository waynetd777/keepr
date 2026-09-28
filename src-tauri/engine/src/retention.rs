//! Which snapshots to keep: every one for a while, then one a day, one a week, one a month.

use chrono::{DateTime, Datelike, Local, TimeZone};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Retention {
    /// Keep every snapshot this recent.
    pub all_hours: u32,
    /// Then the last of each day for this many days,
    pub daily_days: u32,
    /// the last of each week for this many weeks,
    pub weekly_weeks: u32,
    /// and the last of each month for this many months (0: forever).
    pub monthly_months: u32,
}

impl Default for Retention {
    fn default() -> Retention {
        Retention { all_hours: 24, daily_days: 30, weekly_weeks: 52, monthly_months: 0 }
    }
}

/// Which of `times` (snapshot times) to keep, as indexes into `times`. The newest is always kept.
pub fn keep(r: &Retention, times: &[DateTime<Local>], now: DateTime<Local>) -> HashSet<usize> {
    let mut order: Vec<usize> = (0..times.len()).collect();
    order.sort_by(|a, b| times[*b].cmp(&times[*a])); // newest first
    let mut out = HashSet::new();
    if let Some(&newest) = order.first() {
        out.insert(newest);
    }
    let (mut days, mut weeks, mut months) = (HashSet::new(), HashSet::new(), HashSet::new());
    for &i in &order {
        let t = times[i];
        let age = now.signed_duration_since(t);
        if age.num_minutes() < r.all_hours as i64 * 60 {
            out.insert(i);
            continue;
        }
        // The newest snapshot of each day, week and month is the one kept for it.
        let day = t.date_naive();
        if age.num_days() < r.daily_days as i64 && days.insert(day) {
            out.insert(i);
        }
        let week = (t.iso_week().year(), t.iso_week().week());
        if age.num_days() < r.weekly_weeks as i64 * 7 && weeks.insert(week) {
            out.insert(i);
        }
        let month = (t.year(), t.month());
        let months_old = (now.year() - t.year()) * 12 + now.month() as i32 - t.month() as i32;
        if (r.monthly_months == 0 || months_old < r.monthly_months as i32) && months.insert(month) {
            out.insert(i);
        }
    }
    out
}

pub fn parse_time(s: &str) -> Option<DateTime<Local>> {
    DateTime::parse_from_rfc3339(s).ok().map(|t| Local.from_utc_datetime(&t.naive_utc()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn thins_out_with_age() {
        let now = Local.with_ymd_and_hms(2026, 9, 28, 15, 0, 0).unwrap();
        // Hourly for 400 days.
        let times: Vec<_> = (0..400 * 24).map(|h| now - Duration::hours(h)).collect();
        let r = Retention::default();
        let k = keep(&r, &times, now);
        let recent = k.iter().filter(|&&i| i < 24).count();
        assert_eq!(recent, 24);
        // About 24 + 29 days + ~47 more weeks + a few older months.
        assert!(k.len() > 24 + 29 + 40 && k.len() < 24 + 30 + 53 + 15, "{}", k.len());
        assert!(k.contains(&(times.len() - 1)) || k.iter().any(|&i| i > 380 * 24), "a month-end from last year stays");
        // Forever means the oldest month keeps one.
        let oldest_month = (times[times.len() - 1].year(), times[times.len() - 1].month());
        assert!(k.iter().any(|&i| (times[i].year(), times[i].month()) == oldest_month));

        let r = Retention { all_hours: 0, daily_days: 0, weekly_weeks: 0, monthly_months: 1 };
        let k = keep(&r, &times, now);
        assert_eq!(k.len(), 1, "only this month's newest");
    }
}
