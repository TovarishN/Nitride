//! Independent document quiet periods and oldest-ready ordering.
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

#[derive(Default)]
pub(super) struct Schedule {
    pub(super) documents: BTreeMap<String, Instant>,
    ready: BTreeMap<String, Instant>,
    pub(super) reload: Option<Instant>,
}
impl Schedule {
    pub fn edit(&mut self, uri: &str, now: Instant, delay: Duration) {
        self.forget(uri);
        if let Some(due) = now.checked_add(delay) {
            self.documents.insert(uri.to_owned(), due);
        }
    }
    pub fn languages<'a>(
        &mut self,
        uris: impl Iterator<Item = &'a str>,
        now: Instant,
        delay: Duration,
    ) {
        self.reload = now.checked_add(delay);
        self.ready.clear();
        if let Some(due) = self.reload {
            for uri in uris {
                self.documents
                    .entry(uri.to_owned())
                    .and_modify(|old| *old = (*old).max(due))
                    .or_insert(due);
            }
        } else {
            // An unrepresentably distant quiet period waits for an explicit barrier.
            self.documents.clear();
        }
    }
    pub fn loaded(&mut self) {
        self.reload = None;
    }
    pub fn next(&self, reload_pending: bool) -> Option<Instant> {
        let document = || self.documents.values().copied().min();
        if reload_pending {
            self.reload.or_else(document)
        } else {
            document()
        }
    }
    pub fn promote(&mut self, now: Instant) {
        let due: Vec<_> = self
            .documents
            .iter()
            .filter(|(_, due)| **due <= now)
            .map(|(uri, due)| (uri.clone(), *due))
            .collect();
        for (uri, due) in due {
            self.documents.remove(&uri);
            self.ready.entry(uri).or_insert(due);
        }
    }
    pub fn expedite<'a>(&mut self, uris: impl Iterator<Item = &'a str>, now: Instant) {
        self.promote(now);
        for uri in uris {
            let due = self.documents.remove(uri).unwrap_or(now).min(now);
            self.ready.entry(uri.to_owned()).or_insert(due);
        }
    }
    pub fn pop(&mut self, pending: &BTreeSet<String>) -> Option<String> {
        self.ready.retain(|uri, _| pending.contains(uri));
        let uri = self
            .ready
            .iter()
            .min_by_key(|(uri, due)| (**due, *uri))
            .map(|(uri, _)| uri.clone())?;
        self.ready.remove(&uri);
        self.documents.remove(&uri);
        Some(uri)
    }
    pub fn forget(&mut self, uri: &str) {
        self.documents.remove(uri);
        self.ready.remove(uri);
    }
    pub fn retain(&mut self, pending: &BTreeSet<String>, reload_pending: bool) {
        self.documents.retain(|uri, _| pending.contains(uri));
        self.ready.retain(|uri, _| pending.contains(uri));
        if !reload_pending {
            self.reload = None;
        }
    }
    pub fn clear(&mut self) {
        self.documents.clear();
        self.ready.clear();
        self.reload = None;
    }
}

impl<W: std::io::Write> super::Server<W> {
    pub(super) fn schedule_document(&mut self, uri: &str) {
        self.timing.edit(uri, Instant::now(), self.debounce);
    }
    pub(super) fn schedule_languages(&mut self) {
        self.timing.languages(
            self.documents.keys().map(String::as_str),
            Instant::now(),
            self.debounce,
        );
    }
    pub(super) fn flush_due(&mut self) -> std::io::Result<()> {
        let now = Instant::now();
        if self.reload {
            if !self.timing.next(true).is_some_and(|due| due <= now) {
                return Ok(());
            }
            self.reload = false;
            self.timing.loaded();
            self.reload_languages()?;
        }
        self.timing.promote(now);
        if self.jobs.worker.is_some() {
            return self.pump();
        }
        while let Some(uri) = self.timing.pop(&self.pending) {
            self.pending.remove(&uri);
            self.analyze(&uri)?;
        }
        Ok(())
    }
    pub(super) fn expedite(&mut self, target: Option<&str>) -> std::io::Result<()> {
        if std::mem::take(&mut self.reload) {
            self.timing.loaded();
            self.reload_languages()?;
        }
        let now = Instant::now();
        match target {
            Some(uri) if self.pending.contains(uri) => {
                self.timing.expedite(std::iter::once(uri), now)
            }
            Some(_) => self.timing.promote(now),
            None => self
                .timing
                .expedite(self.pending.iter().map(String::as_str), now),
        }
        self.pump()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::BTreeSet,
        time::{Duration, Instant},
    };
    fn at(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }
    fn pending(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn editing_b_preserves_a_quiet_deadline() {
        let now = Instant::now();
        let mut timing = Schedule::default();
        timing.edit("a", now, Duration::from_millis(100));
        timing.edit("b", at(now, 50), Duration::from_millis(100));
        assert_eq!(timing.next(false), Some(at(now, 100)));
        timing.promote(at(now, 100));
        assert_eq!(timing.pop(&pending(&["a", "b"])), Some("a".into()));
        assert_eq!(timing.next(false), Some(at(now, 150)));
        assert_eq!(timing.pop(&pending(&["b"])), None);
    }
    #[test]
    fn oldest_ready_wins_before_lexical_order_or_new_request() {
        let now = Instant::now();
        let mut timing = Schedule::default();
        timing.edit("z", now, Duration::from_millis(10));
        timing.edit("a", at(now, 10), Duration::from_millis(10));
        timing.promote(at(now, 20));
        timing.expedite(["new"].into_iter(), at(now, 30));
        let names = pending(&["z", "a", "new"]);
        assert_eq!(timing.pop(&names), Some("z".into()));
        assert_eq!(timing.pop(&names), Some("a".into()));
        assert_eq!(timing.pop(&names), Some("new".into()));
    }
    #[test]
    fn tie_order_is_deterministic_and_editing_ready_file_resets_it() {
        let now = Instant::now();
        let mut timing = Schedule::default();
        timing.edit("a", now, Duration::from_millis(10));
        timing.edit("b", now, Duration::from_millis(10));
        timing.promote(at(now, 10));
        timing.edit("a", at(now, 11), Duration::from_millis(10));
        assert_eq!(timing.pop(&pending(&["a", "b"])), Some("b".into()));
        timing.promote(at(now, 21));
        assert_eq!(timing.pop(&pending(&["a"])), Some("a".into()));
        timing.expedite(["b", "a"].into_iter(), at(now, 30));
        assert_eq!(timing.pop(&pending(&["a", "b"])), Some("a".into()));
    }
    #[test]
    fn shared_reload_gates_a_but_later_edit_only_extends_b() {
        let now = Instant::now();
        let mut timing = Schedule::default();
        timing.edit("a", now, Duration::from_millis(20));
        timing.languages(
            ["a", "b"].into_iter(),
            at(now, 10),
            Duration::from_millis(100),
        );
        timing.edit("b", at(now, 50), Duration::from_millis(100));
        assert_eq!(timing.next(true), Some(at(now, 110)));
        timing.loaded();
        timing.promote(at(now, 110));
        assert_eq!(timing.pop(&pending(&["a", "b"])), Some("a".into()));
        assert_eq!(timing.pop(&pending(&["b"])), None);
        assert_eq!(timing.next(false), Some(at(now, 150)));
    }
    #[test]
    fn target_expedite_preserves_neighbors_but_all_barrier_drains_them() {
        let now = Instant::now();
        let mut timing = Schedule::default();
        for uri in ["a", "b"] {
            timing.edit(uri, now, Duration::from_secs(60));
        }
        timing.expedite(["a"].into_iter(), at(now, 10));
        assert_eq!(timing.pop(&pending(&["a", "b"])), Some("a".into()));
        assert_eq!(timing.pop(&pending(&["b"])), None);
        timing.expedite(["b"].into_iter(), at(now, 20));
        assert_eq!(timing.pop(&pending(&["b"])), Some("b".into()));
        assert_eq!(timing.next(false), None);
    }
    #[test]
    fn close_and_pending_pruning_remove_timing_state() {
        let now = Instant::now();
        let mut timing = Schedule::default();
        timing.edit("old", now, Duration::from_millis(10));
        timing.promote(at(now, 10));
        timing.forget("old");
        timing.edit("future", now, Duration::from_secs(60));
        timing.retain(&pending(&[]), false);
        assert_eq!(timing.pop(&pending(&[])), None);
        assert_eq!(timing.next(false), None);
    }
    #[test]
    fn language_change_resets_ready_work_and_preserves_later_doc_deadlines() {
        let now = Instant::now();
        let mut timing = Schedule::default();
        timing.edit("ready", now, Duration::from_millis(1));
        timing.promote(at(now, 1));
        timing.edit("later", now, Duration::from_millis(200));
        timing.languages(
            ["ready", "later"].into_iter(),
            at(now, 10),
            Duration::from_millis(20),
        );
        assert_eq!(timing.pop(&pending(&["ready", "later"])), None);
        timing.loaded();
        timing.promote(at(now, 30));
        assert_eq!(
            timing.pop(&pending(&["ready", "later"])),
            Some("ready".into())
        );
        assert_eq!(timing.next(false), Some(at(now, 200)));
    }
}
