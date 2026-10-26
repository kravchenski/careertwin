//! What you have practised, day by day: minutes, sessions, focus blocks and words said. It feeds
//! the streak and the numbers in Usage, and nothing else; it never leaves this computer.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::write_private;
use crate::session::date;

const DAY: u64 = 86_400;
/// A day counts for the streak once you have practised this long, or finished a focus block.
const STREAK_SECONDS: u32 = 60;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Day {
    pub seconds: u32,
    pub sessions: u32,
    pub focus_blocks: u32,
    pub words: u32,
}

impl Day {
    fn active(&self) -> bool {
        self.seconds >= STREAK_SECONDS || self.focus_blocks > 0
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Stats {
    /// Keyed by `YYYY-MM-DD` (UTC), so the file sorts by date.
    pub days: BTreeMap<String, Day>,
}

impl Stats {
    /// The saved numbers, or an empty set when there is no file or it cannot be read.
    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(self).context("could not write the practice log")?;
        write_private(path, &bytes)
    }

    fn day_mut(&mut self, now: u64) -> &mut Day {
        self.days.entry(date(now)).or_default()
    }

    /// A finished stretch of practice: how long it lasted and how many words you said in it.
    pub fn add_practice(&mut self, now: u64, seconds: u32, words: u32) {
        let day = self.day_mut(now);
        day.seconds = day.seconds.saturating_add(seconds);
        day.words = day.words.saturating_add(words);
        day.sessions += 1;
    }

    pub fn add_focus_block(&mut self, now: u64) {
        self.day_mut(now).focus_blocks += 1;
    }

    pub fn today(&self, now: u64) -> Day {
        self.days.get(&date(now)).cloned().unwrap_or_default()
    }

    /// How many days in a row you practised, counting back from today. A day that has not
    /// happened yet does not break the streak, so it is still alive in the morning.
    pub fn streak(&self, now: u64) -> u32 {
        let active = |at: u64| self.days.get(&date(at)).is_some_and(Day::active);
        let mut count = 0;
        let mut at = now;
        if !active(at) {
            at = at.saturating_sub(DAY);
        }
        while active(at) {
            count += 1;
            if at < DAY {
                break;
            }
            at -= DAY;
        }
        count
    }

    /// The seconds practised on each of the last `n` days, the oldest first, with their dates.
    pub fn last_days(&self, now: u64, n: u32) -> Vec<(String, u32)> {
        (0..n)
            .rev()
            .map(|back| {
                let at = now.saturating_sub(u64::from(back) * DAY);
                (date(at), self.days.get(&date(at)).map_or(0, |d| d.seconds))
            })
            .collect()
    }

    pub fn totals(&self) -> Day {
        self.days.values().fold(Day::default(), |mut total, day| {
            total.seconds = total.seconds.saturating_add(day.seconds);
            total.sessions += day.sessions;
            total.focus_blocks += day.focus_blocks;
            total.words = total.words.saturating_add(day.words);
            total
        })
    }

    /// The seconds practised over the last seven days, today included.
    pub fn week_seconds(&self, now: u64) -> u32 {
        self.last_days(now, 7).iter().fold(0u32, |sum, (_, seconds)| sum.saturating_add(*seconds))
    }
}

/// How strongly a day is lit in the calendar, 0 to 4, against the goal for the day: 0 nothing,
/// 1 a short visit, 2 getting there, 3 the goal met, 4 twice the goal.
pub fn level(day: &Day, goal_seconds: u32) -> u8 {
    if !day.active() {
        return 0;
    }
    let goal = goal_seconds.max(60);
    match day.seconds {
        s if s >= goal * 2 => 4,
        s if s >= goal => 3,
        s if s * 5 >= goal * 2 => 2,
        _ => 1,
    }
}

/// One day of the calendar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub date: String,
    pub day: Day,
    pub level: u8,
    pub today: bool,
}

/// The last weeks as columns of seven days, Monday at the top, like a contribution graph. Days
/// after today are left empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Calendar {
    pub columns: Vec<[Option<Cell>; 7]>,
    /// Where a month begins: the column and the month, 1 to 12.
    pub months: Vec<(usize, u32)>,
    /// Days with any practice in the whole calendar.
    pub practised: u32,
}

impl Stats {
    /// The calendar of the last `weeks` weeks, the current one included.
    pub fn calendar(&self, now: u64, weeks: usize, goal_minutes: u32) -> Calendar {
        let weeks = weeks.max(1);
        let today = now / DAY;
        let weekday = (today + 3) % 7; // 1 January 1970 was a Thursday; Monday is 0
        let first = (today - weekday).saturating_sub((weeks as u64 - 1) * 7);
        let goal = goal_minutes.saturating_mul(60);
        let mut columns = Vec::with_capacity(weeks);
        let mut months: Vec<(usize, u32)> = Vec::new();
        let mut practised = 0;
        for column in 0..weeks {
            let mut cells: [Option<Cell>; 7] = Default::default();
            for (row, slot) in cells.iter_mut().enumerate() {
                let days = first + column as u64 * 7 + row as u64;
                if days > today {
                    continue;
                }
                let date = date(days * DAY);
                let day = self.days.get(&date).cloned().unwrap_or_default();
                let level = level(&day, goal);
                practised += u32::from(level > 0);
                *slot = Some(Cell { date, day, level, today: days == today });
            }
            let monday = date((first + column as u64 * 7) * DAY);
            let month: u32 = monday.get(5..7).and_then(|m| m.parse().ok()).unwrap_or(1);
            if months.last().is_none_or(|(_, last)| *last != month) {
                months.push((column, month));
            }
            columns.push(cells);
        }
        // a label needs room: when two are closer than three columns the earlier one gives way
        let mut kept: Vec<(usize, u32)> = Vec::new();
        for (index, (column, month)) in months.iter().enumerate() {
            let crowded = months.get(index + 1).is_some_and(|(next, _)| next - column < 3);
            if !crowded {
                kept.push((*column, *month));
            }
        }
        Calendar { columns, months: kept, practised }
    }
}

/// "1 h 05 min", "12 min", "40 s": a length of time for people.
pub fn duration_label(seconds: u32, hours: &str, minutes: &str, secs: &str) -> String {
    if seconds >= 3600 {
        format!("{} {hours} {:02} {minutes}", seconds / 3600, seconds % 3600 / 60)
    } else if seconds >= 60 {
        format!("{} {minutes}", seconds / 60)
    } else {
        format!("{seconds} {secs}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 2026-10-25 12:00 UTC
    const NOW: u64 = 1_792_929_600;

    #[test]
    fn practice_adds_up_on_the_day_it_happened() {
        let mut stats = Stats::default();
        stats.add_practice(NOW, 600, 120);
        stats.add_practice(NOW + 60, 300, 40);
        stats.add_focus_block(NOW);
        let today = stats.today(NOW);
        assert_eq!(today, Day { seconds: 900, sessions: 2, focus_blocks: 1, words: 160 });
        assert_eq!(stats.today(NOW + DAY), Day::default());
    }

    #[test]
    fn the_streak_counts_consecutive_days_and_survives_the_morning() {
        let mut stats = Stats::default();
        for back in 1..=3 {
            stats.add_practice(NOW - back * DAY, 120, 10);
        }
        assert_eq!(stats.streak(NOW), 3, "today has not started, yesterday keeps it alive");
        stats.add_practice(NOW, 120, 10);
        assert_eq!(stats.streak(NOW), 4);
        stats.add_practice(NOW - 5 * DAY, 120, 10);
        assert_eq!(stats.streak(NOW), 4, "a gap ends the run");
        assert_eq!(stats.streak(NOW + 3 * DAY), 0, "three quiet days break it");
    }

    #[test]
    fn a_few_seconds_do_not_keep_a_streak_but_a_focus_block_does() {
        let mut stats = Stats::default();
        stats.add_practice(NOW, 20, 3);
        assert_eq!(stats.streak(NOW), 0);
        stats.add_focus_block(NOW);
        assert_eq!(stats.streak(NOW), 1);
    }

    #[test]
    fn the_last_days_run_oldest_first_and_fill_the_gaps() {
        let mut stats = Stats::default();
        stats.add_practice(NOW, 300, 0);
        stats.add_practice(NOW - 2 * DAY, 900, 0);
        let days = stats.last_days(NOW, 3);
        assert_eq!(days.iter().map(|(_, s)| *s).collect::<Vec<_>>(), vec![900, 0, 300]);
        assert_eq!(days[2].0, "2026-10-25");
        assert_eq!(stats.week_seconds(NOW), 1200);
    }

    #[test]
    fn totals_cover_every_day() {
        let mut stats = Stats::default();
        stats.add_practice(NOW, 100, 5);
        stats.add_practice(NOW - 40 * DAY, 200, 7);
        stats.add_focus_block(NOW);
        let total = stats.totals();
        assert_eq!((total.seconds, total.sessions, total.focus_blocks, total.words), (300, 2, 1, 12));
    }

    #[test]
    fn it_round_trips_and_a_bad_file_gives_an_empty_log() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data").join("stats.json");
        let mut stats = Stats::default();
        stats.add_practice(NOW, 90, 9);
        stats.save(&path).unwrap();
        assert_eq!(Stats::load(&path).today(NOW).seconds, 90);
        fs::write(&path, "{ not json").unwrap();
        assert!(Stats::load(&path).days.is_empty());
        assert!(Stats::load(&dir.path().join("missing.json")).days.is_empty());
    }

    #[test]
    fn the_levels_follow_the_goal_for_the_day() {
        let day = |seconds, focus_blocks| Day { seconds, focus_blocks, ..Day::default() };
        let goal = 25 * 60;
        assert_eq!(level(&day(0, 0), goal), 0);
        assert_eq!(level(&day(30, 0), goal), 0, "half a minute is not a visit");
        assert_eq!(level(&day(0, 1), goal), 1, "a finished focus block is");
        assert_eq!(level(&day(5 * 60, 0), goal), 1);
        assert_eq!(level(&day(10 * 60, 0), goal), 2);
        assert_eq!(level(&day(25 * 60, 0), goal), 3);
        assert_eq!(level(&day(49 * 60, 0), goal), 3);
        assert_eq!(level(&day(50 * 60, 0), goal), 4);
        assert_eq!(level(&day(600, 0), 0), 4, "a goal of nothing counts as a minute and does not divide by zero");
    }

    #[test]
    fn the_calendar_has_whole_weeks_with_monday_at_the_top_and_nothing_after_today() {
        let mut stats = Stats::default();
        stats.add_practice(NOW, 30 * 60, 100); // Sunday 25 October 2026
        stats.add_practice(NOW - 6 * DAY, 10 * 60, 40); // Monday 19 October
        stats.add_focus_block(NOW - 7 * DAY); // Sunday 18 October
        let calendar = stats.calendar(NOW, 4, 25);
        assert_eq!(calendar.columns.len(), 4);
        let last = &calendar.columns[3];
        assert_eq!(last[0].as_ref().unwrap().date, "2026-10-19", "the week of today starts on Monday");
        assert_eq!(last[0].as_ref().unwrap().level, 2);
        let sunday = last[6].as_ref().unwrap();
        assert_eq!((sunday.date.as_str(), sunday.level, sunday.today), ("2026-10-25", 3, true));
        assert_eq!(calendar.columns[2][6].as_ref().unwrap().level, 1);
        assert_eq!(calendar.practised, 3);
        // a Wednesday: the days after it are empty
        let midweek = stats.calendar(NOW - 4 * DAY, 2, 25);
        let week = &midweek.columns[1];
        assert!(week[..3].iter().all(Option::is_some) && week[3..].iter().all(Option::is_none), "{week:?}");
        assert_eq!(week[2].as_ref().unwrap().date, "2026-10-21");
        assert!(week[2].as_ref().unwrap().today);
    }

    #[test]
    fn months_are_named_where_they_begin_and_never_on_top_of_each_other() {
        let calendar = Stats::default().calendar(NOW, 26, 25);
        let months: Vec<u32> = calendar.months.iter().map(|(_, m)| *m).collect();
        assert!(months.windows(2).all(|w| w[0] != w[1]), "{months:?}");
        assert_eq!(*months.last().unwrap(), 10, "the current month is the last label");
        for pair in calendar.months.windows(2) {
            assert!(pair[1].0 - pair[0].0 >= 3, "{:?}", calendar.months);
        }
        assert_eq!(calendar.practised, 0);
        assert_eq!(Stats::default().calendar(NOW, 0, 25).columns.len(), 1, "never an empty calendar");
    }

    #[test]
    fn lengths_read_naturally() {
        assert_eq!(duration_label(40, "h", "min", "s"), "40 s");
        assert_eq!(duration_label(12 * 60 + 5, "h", "min", "s"), "12 min");
        assert_eq!(duration_label(3600 + 5 * 60, "h", "min", "s"), "1 h 05 min");
    }
}
