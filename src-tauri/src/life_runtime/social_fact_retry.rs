//! Optional album/object persistence after actual paired terminal commit and cleanup.
//! Retained events are native immutable receipts, never new completion evidence.
use crate::social::{Outcome, SocialEvent};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

const MAX_PENDING: usize = 32;
const MAX_RECENT: usize = 256;
const MAX_ATTEMPTS: u8 = 3;

struct Pending {
    event: SocialEvent,
    fences: [Option<super::super::action_state::LocalFence>; 2],
    settled: [bool; 2],
    attempts: [u8; 2],
}
#[derive(Default)]
pub(super) struct Report {
    pub changed: BTreeSet<String>,
    pub failed: BTreeSet<String>,
    /// A late optional failure may affect only its original native action and
    /// session, never an actor's replacement session or a newer pending source.
    pub failed_sources: BTreeMap<String, (crate::life_domain::ActionId, Option<super::super::action_state::LocalFence>)>,
}
#[derive(Default)]
pub(super) struct SocialFactRetry {
    pending: BTreeMap<String, Pending>,
    recent: VecDeque<String>,
}
impl SocialFactRetry {
    /// Only called after the same event's actual social terminal transaction.
    /// Store still revalidates its memory and both retained LIVE action sources.
    #[cfg(test)]
    pub fn submit(
        &mut self,
        event: &SocialEvent,
        append: impl FnMut(&str, &SocialEvent) -> Result<bool, String>,
    ) -> Report {
        self.submit_scoped(event, [None, None], append)
    }
    pub fn submit_scoped(
        &mut self,
        event: &SocialEvent,
        fences: [Option<super::super::action_state::LocalFence>; 2],
        mut append: impl FnMut(&str, &SocialEvent) -> Result<bool, String>,
    ) -> Report {
        if self.pending.contains_key(event.id()) || self.recent.iter().any(|id| id == event.id()) {
            return Report::default();
        }
        if event.outcome() != Outcome::Completed
            || event.actions().is_none()
            || event.source_events().is_none()
            || self.pending.len() >= MAX_PENDING
        {
            let mut report = Report {
                failed: event.participants().iter().cloned().collect(),
                ..Default::default()
            };
            if let Some(actions) = event.actions() {
                for (index, bot) in event.participants().iter().enumerate() {
                    report.failed_sources.insert(bot.clone(), (actions[index].clone(), fences[index].clone()));
                }
            }
            return report;
        }
        let mut pending = Pending {
            event: event.clone(),
            fences,
            settled: [false; 2],
            attempts: [0; 2],
        };
        let mut report = Report::default();
        Self::attempt(&mut pending, &mut append, &mut report);
        if pending.settled.iter().all(|done| *done) {
            self.remember(event.id());
        } else {
            self.pending.insert(event.id().into(), pending);
        }
        report
    }
    /// One attempt per outstanding actor per native pulse, with no lifetime or
    /// attempt reset on replay. Successful actors are never submitted again.
    pub fn retry(
        &mut self,
        mut append: impl FnMut(&str, &SocialEvent) -> Result<bool, String>,
    ) -> Report {
        let mut report = Report::default();
        for (id, mut pending) in std::mem::take(&mut self.pending) {
            Self::attempt(&mut pending, &mut append, &mut report);
            if pending.settled.iter().all(|done| *done) {
                self.remember(&id);
            } else {
                self.pending.insert(id, pending);
            }
        }
        report
    }
    fn attempt(
        pending: &mut Pending,
        append: &mut impl FnMut(&str, &SocialEvent) -> Result<bool, String>,
        report: &mut Report,
    ) {
        for (index, bot) in pending.event.participants().iter().enumerate() {
            if pending.settled[index] {
                continue;
            }
            pending.attempts[index] += 1;
            match append(bot, &pending.event) {
                Ok(changed) => {
                    pending.settled[index] = true;
                    if changed {
                        report.changed.insert(bot.clone());
                    }
                }
                Err(_) if pending.attempts[index] >= MAX_ATTEMPTS => {
                    pending.settled[index] = true;
                    report.failed.insert(bot.clone());
                    if let Some(actions) = pending.event.actions() {
                        report.failed_sources.insert(bot.clone(), (actions[index].clone(), pending.fences[index].clone()));
                    }
                }
                Err(_) => (),
            }
        }
    }
    fn remember(&mut self, id: &str) {
        if self.recent.len() >= MAX_RECENT {
            self.recent.pop_front();
        }
        self.recent.push_back(id.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn completed(
        sequence: u64,
    ) -> (
        crate::social::Coordinator,
        crate::social::SceneView,
        SocialEvent,
    ) {
        let (coordinator, scene, events, _) = crate::social::tests::completed(
            sequence,
            ["nova", "blue"],
            crate::social::tests::states(["nova", "blue"]),
        );
        let event = events
            .into_iter()
            .find(|event| event.outcome() == Outcome::Completed)
            .unwrap();
        (coordinator, scene, event)
    }
    #[test]
    fn retries_only_the_failed_actor_and_retains_the_exact_confirmed_event() {
        let (_, _, event) = completed(1);
        let expected = serde_json::to_value(&event).unwrap();
        let mut queue = SocialFactRetry::default();
        let mut calls = BTreeMap::<String, usize>::new();
        let mut append = |bot: &str, actual: &SocialEvent| {
            assert_eq!(serde_json::to_value(actual).unwrap(), expected);
            let count = calls.entry(bot.into()).or_default();
            *count += 1;
            if bot == "blue" && *count == 1 {
                Err("controlled storage failure".into())
            } else {
                Ok(true)
            }
        };
        let first = queue.submit(&event, &mut append);
        assert_eq!(first.changed, BTreeSet::from(["nova".into()]));
        assert!(first.failed.is_empty());
        assert!(queue.submit(&event, &mut append).changed.is_empty());
        let second = queue.retry(&mut append);
        assert_eq!(second.changed, BTreeSet::from(["blue".into()]));
        assert!(second.failed.is_empty());
        assert!(queue.pending.is_empty());
        assert!(queue.submit(&event, &mut append).changed.is_empty());
        assert!(queue.retry(&mut append).failed.is_empty());
        assert_eq!(
            calls,
            BTreeMap::from([("nova".into(), 1), ("blue".into(), 2)])
        );
    }
    #[test]
    fn three_total_attempts_report_only_the_permanently_failed_owner_once() {
        let (_, _, event) = completed(2);
        let mut queue = SocialFactRetry::default();
        let mut calls = BTreeMap::<String, usize>::new();
        let mut append = |bot: &str, _: &SocialEvent| {
            *calls.entry(bot.into()).or_default() += 1;
            if bot == "blue" {
                Err("controlled failure".into())
            } else {
                Ok(false)
            }
        };
        assert!(queue.submit(&event, &mut append).failed.is_empty());
        assert!(queue.retry(&mut append).failed.is_empty());
        assert_eq!(
            queue.retry(&mut append).failed,
            BTreeSet::from(["blue".into()])
        );
        assert!(queue.retry(&mut append).failed.is_empty());
        assert!(queue.submit(&event, &mut append).failed.is_empty());
        assert_eq!(
            calls,
            BTreeMap::from([("nova".into(), 1), ("blue".into(), 3)])
        );
    }
    #[test]
    fn optional_object_failure_preserves_confirmed_outcome_and_original_failure_fence() {
        use super::super::super::action_state::LocalFence;
        let (mut coordinator, scene, event) = completed(42);
        let fence = LocalFence { session:"original-session".into(), revision:7,
            system_generation:2, ui_generation:3 };
        let mut queue = SocialFactRetry::default();
        let mut album = BTreeSet::<String>::new();
        let mut object = BTreeSet::<String>::new();
        let mut append = |bot:&str, actual:&SocialEvent| {
            assert_eq!(actual.outcome(), Outcome::Completed);
            album.insert(bot.into()); // Earlier album TX is already idempotent.
            if bot == "blue" { return Err("controlled corrupt object preferences".into()); }
            Ok(object.insert(bot.into())) // Object-only change remains visible.
        };
        let report = queue.submit_scoped(&event, [Some(fence.clone()), Some(fence.clone())], &mut append);
        assert_eq!(report.changed, BTreeSet::from(["nova".into()]));
        coordinator.released(&scene.id).unwrap();
        assert!(!coordinator.busy("nova")); assert!(!coordinator.busy("blue"));
        // Replaying this event with a replacement fence cannot change the
        // retained original ownership or restart its three-attempt budget.
        let mut replacement = fence.clone(); replacement.session = "successor-session".into();
        assert!(queue.submit_scoped(&event, [Some(replacement.clone()), Some(replacement)], &mut append).failed.is_empty());
        assert!(queue.retry(&mut append).failed.is_empty());
        let final_report = queue.retry(&mut append);
        assert_eq!(final_report.failed, BTreeSet::from(["blue".into()]));
        let (failed_action, original) = &final_report.failed_sources["blue"];
        assert_eq!(failed_action, &event.actions().unwrap()[1]);
        assert!(original.as_ref() == Some(&fence));
        assert!(queue.pending.is_empty());
        assert_eq!(event.outcome(), Outcome::Completed);
        assert_eq!(album, BTreeSet::from(["nova".into(), "blue".into()]));
        assert_eq!(object, BTreeSet::from(["nova".into()]));
    }
    #[test]
    fn album_failure_does_not_hold_real_coordinator_cleanup_or_actor_exclusivity() {
        let (mut coordinator, scene, event) = completed(3);
        let mut queue = SocialFactRetry::default();
        queue.submit(&event, |_, _| Err("controlled failure".into()));
        assert_eq!(queue.pending.len(), 1);
        assert!(coordinator.busy("nova"));
        coordinator.released(&scene.id).unwrap();
        assert!(!coordinator.busy("nova"));
        assert!(!coordinator.busy("blue"));
        assert_eq!(queue.pending.len(), 1);
        assert_eq!(queue.retry(|_, _| Ok(true)).changed.len(), 2);
    }
    #[test]
    fn bounded_queue_refuses_overflow_without_running_the_append_and_drains() {
        let mut queue = SocialFactRetry::default();
        for sequence in 1..=MAX_PENDING as u64 {
            queue.submit(&completed(sequence).2, |_, _| {
                Err("controlled failure".into())
            });
        }
        let mut called = false;
        let report = queue.submit(&completed(33).2, |_, _| {
            called = true;
            Ok(true)
        });
        assert!(!called);
        assert_eq!(
            report.failed,
            BTreeSet::from(["nova".into(), "blue".into()])
        );
        assert_eq!(queue.pending.len(), MAX_PENDING);
        queue.retry(|_, _| Ok(false));
        assert!(queue.pending.is_empty());
        for sequence in 34..=34 + MAX_RECENT as u64 {
            queue.submit(&completed(sequence).2, |_, _| Ok(false));
        }
        assert_eq!(queue.recent.len(), MAX_RECENT);
    }
    #[test]
    fn invitation_is_never_promoted_to_an_album_completion() {
        let (_, _, events, _) = crate::social::tests::completed(
            4,
            ["nova", "blue"],
            crate::social::tests::states(["nova", "blue"]),
        );
        let event = events
            .iter()
            .find(|event| event.outcome() == Outcome::Invited)
            .unwrap();
        let mut queue = SocialFactRetry::default();
        let mut called = false;
        let report = queue.submit(event, |_, _| {
            called = true;
            Ok(true)
        });
        assert!(!called);
        assert_eq!(report.failed.len(), 2);
        assert!(queue.pending.is_empty());
    }
}
