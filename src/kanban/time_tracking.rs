use chrono::TimeDelta;

use super::*;
use std::collections::HashSet;
///The time recording type.
#[derive(PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Clone, Copy, Debug)]
pub enum TimeEntry {
    ///A time entry without any referent to actual time.
    InstanteousDuration(chrono::TimeDelta),
    ///A time entry with both a beginning and an ending time.
    Concluded(chrono::DateTime<chrono::Utc>, chrono::DateTime<Utc>),
    ///A time entry that hasn't been concluded.
    Started(chrono::DateTime<chrono::Utc>),
}
impl TimeEntry {
    /// Mark a time period as having been concluded.
    /// This produces a range of different results/
    ///
    /// For all Started items, it will turn it into a Concluded item
    /// with the current datetime
    pub fn conclude(self) -> Self {
        match self {
            Self::Started(started) => Self::Concluded(started, Utc::now()),
            _ => self,
        }
    }
    ///Calculate or retrieve the duration of the time entry.
    pub fn duration(self) -> chrono::TimeDelta {
        match self {
            Self::InstanteousDuration(x) => x,
            Self::Concluded(started, ended) => ended - started,
            Self::Started(start) => Utc::now() - start,
        }
    }
    ///Retrieve a 'human friendly' description of the time entry
    pub fn to_description(self) -> String {
        let dur = self.duration();
        match self {
            Self::InstanteousDuration(_) => format!(
                "{} hours, {} minutes",
                dur.num_hours(),
                dur.num_minutes() % 60
            ),
            Self::Started(start) => format!(
                "{}",
                if dur.num_days() > 0 {
                    // The date
                    start.format("%B %d")
                } else {
                    //The hour started
                    start.format("%I:%M:%S")
                }
            ),
            Self::Concluded(start, end) => {
                if start.day() == end.day()
                    && start.month() == end.month()
                    && start.year() == end.year()
                {
                    format!("{} - {}", start.format("%I:%M%P"), end.format("%I:%M%P %F"))
                } else if start.year() == end.year() {
                    format!("{} - {}", start.format("%B %d"), end.format("%B %d %Y"))
                } else {
                    format!("{} - {}", start.format("%B %d %Y"), end.format("%B %d %Y"))
                }
            }
        }
    }
}
///A container for the time records in a task
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TimeRecords {
    ///An association between time entries and, optionally, a string
    ///describing how the time was spent
    pub entries: Vec<(TimeEntry, Option<String>)>,
}
impl Default for TimeRecords {
    fn default() -> Self {
        Self::new()
    }
}
impl TimeRecords {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
    /// Finish recording, or add a new in-progress recording.
    ///
    /// * `description` - The description of the time being done
    pub fn handle_record_request(&mut self, description: Option<String>) {
        for item in self.entries.iter_mut().rev() {
            if matches!(item.0, TimeEntry::Started(_)) {
                item.0 = item.0.conclude();
                return;
            }
        }
        self.entries
            .push((TimeEntry::Started(Utc::now()), description));
    }
    pub fn is_recording(&self) -> bool {
        self.entries
            .iter()
            .rev()
            .any(|x| matches!(x.0, TimeEntry::Started(_)))
    }
    /// Get the total duration of all the time records in the structure
    pub fn duration(&self) -> chrono::TimeDelta {
        self.entries
            .iter()
            .map(|x| x.0.duration())
            .fold(chrono::TimeDelta::new(0, 0).unwrap(), |a, b| a + b)
    }
    #[cfg(test)]
    pub(crate) fn add_duration_test(&mut self, d: chrono::TimeDelta) {
        self.entries.push((TimeEntry::InstanteousDuration(d), None));
    }
}
///Collect the time spent of each child task.
///
///Produces a list of task Ids and associated durations.
pub fn collect_child_durations(
    document: &KanbanDocument,
    item: &KanbanItem,
) -> Vec<(KanbanId, TimeDelta)> {
    let mut result = Vec::new();
    let mut seen: HashSet<KanbanId> = HashSet::new();
    seen.extend(item.child_tasks.iter());
    for i in item.child_tasks.iter() {
        // This needs to start at zero because on_tree calls the function on the root node.
        let mut current = document.get_task(*i).unwrap().time_records.duration();
        document.on_tree(*i, 0, |document, x, _| {
            if seen.contains(&x) {
                return;
            }
            seen.insert(x);
            let task = document.get_task(x);
            current += task.unwrap().time_records.duration();
        });
        result.push((*i, current));
    }
    result
}
#[cfg(test)]
mod test {

    use crate::kanban::tests::make_document_easy;
    use proptest::prelude::*;

    use super::*;

    // ── proptest strategies ───────────────────────────────────────────────

    prop_compose! {
        /// Arbitrary non-negative duration up to ~1000 hours.
        fn arb_delta()(secs in 0i64..=3_600_000) -> TimeDelta {
            TimeDelta::new(secs, 0).unwrap()
        }
    }

    prop_compose! {
        fn arb_instant_entry()(secs in 0i64..=3_600_000) -> TimeEntry {
            TimeEntry::InstanteousDuration(TimeDelta::new(secs, 0).unwrap())
        }
    }

    prop_compose! {
        /// A Concluded entry where end >= start.
        fn arb_concluded_entry()(start_secs in 0i64..=1_000_000, dur in 0i64..=3_600_000) -> TimeEntry {
            let start = DateTime::UNIX_EPOCH + TimeDelta::new(start_secs, 0).unwrap();
            let end   = start + TimeDelta::new(dur, 0).unwrap();
            TimeEntry::Concluded(start, end)
        }
    }

    fn arb_entry() -> impl Strategy<Value = TimeEntry> {
        prop_oneof![arb_instant_entry(), arb_concluded_entry(),]
    }

    prop_compose! {
        fn arb_records()(entries in prop::collection::vec(arb_entry(), 0..20)) -> TimeRecords {
            TimeRecords { entries: entries.into_iter().map(|e| (e, None)).collect() }
        }
    }

    // ── TimeRecords properties ────────────────────────────────────────────

    proptest! {
        #[test]
        fn total_duration_equals_sum_of_entries(records in arb_records()) {
            let expected: TimeDelta = records.entries.iter()
                .map(|(e, _)| e.duration())
                .fold(TimeDelta::zero(), |a, b| a + b);
            prop_assert_eq!(records.duration(), expected);
        }

        #[test]
        fn concluded_entry_duration_is_non_negative(entry in arb_concluded_entry()) {
            prop_assert!(entry.duration() >= TimeDelta::zero());
        }

        #[test]
        fn start_stop_recording_adds_concluded_entry(mut records in arb_records()) {
            let before = records.entries.len();
            records.handle_record_request(None);
            // Either stopped an existing Started entry (count unchanged) or added one.
            let after = records.entries.len();
            prop_assert!(after == before || after == before + 1);
            // No Started entries remain after stopping.
            records.handle_record_request(None);
            // Calling stop when nothing is recording adds a new Started entry,
            // so calling it again concludes it — either way no more than one Started remains.
            let started_count = records.entries.iter()
                .filter(|(e, _)| matches!(e, TimeEntry::Started(_)))
                .count();
            prop_assert!(started_count <= 1);
        }

        #[test]
        fn adding_instantaneous_entry_increases_duration(mut records in arb_records(), delta in arb_delta()) {
            let before = records.duration();
            records.entries.push((TimeEntry::InstanteousDuration(delta), None));
            prop_assert_eq!(records.duration(), before + delta);
        }
    }
    ///Test that the time record container correctly handles a timer toggle
    ///signal
    #[test]
    fn test_recording() {
        let mut t = TimeRecords::new();
        t.entries
            .push((TimeEntry::Started(DateTime::UNIX_EPOCH), None));
        assert!(matches!(t.entries[0].0, TimeEntry::Started(_)));
        t.handle_record_request(None);
        assert!(matches!(t.entries[0].0, TimeEntry::Concluded(_, _)));
        assert_eq!(t.entries[0].0.duration(), t.duration());
        t.handle_record_request(None);
        assert_eq!(t.entries.len(), 2);
    }
    #[test]
    fn test_task_duration() {
        // 0 -> {1,2}
        // 2->{3,4}
        // 3->{4}
        let children = [vec![1, 2], Vec::new(), vec![3, 4], vec![4], Vec::new()];
        let mut document = make_document_easy(5, &children);
        // Trivial case, the answer doesn't have any paths where an item can be
        // counted more than once.
        {
            {
                let task = document.get_task_mut(1).unwrap();
                task.time_records
                    .add_duration_test(chrono::TimeDelta::new(5, 0).unwrap());
            }
            let zero_task = document.get_task(0).unwrap();
            let duration = collect_child_durations(&document, zero_task);
            assert_eq!(
                duration
                    .iter()
                    .fold(TimeDelta::new(0, 0).unwrap(), |start, x| x.1 + start),
                TimeDelta::new(5, 0).unwrap()
            );
        }
        // Non-trivial case, the item in question can be counted twice if the
        // implementation is wrong.
        {
            {
                let task = document.get_task_mut(4).unwrap();
                task.time_records
                    .add_duration_test(chrono::TimeDelta::new(5, 0).unwrap());
            }
            let zero_task = document.get_task(0).unwrap();
            let duration = collect_child_durations(&document, zero_task);
            assert_eq!(
                duration
                    .iter()
                    .fold(TimeDelta::new(0, 0).unwrap(), |start, x| x.1 + start),
                TimeDelta::new(10, 0).unwrap()
            );
        }
    }
}
