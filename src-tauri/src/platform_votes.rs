//! Ephemeral native-message tally. No sender IDs/text, persistence, replies or action authority.
use crate::experience::{valid_id, ImpulseKind, MAX_TIME};
use std::collections::BTreeSet;
pub(super) const WINDOW_MS: u64 = 15_000;
pub(super) const MAX_VOTERS: usize = 256;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Choice {
    Rest,
    Play,
}
#[derive(Clone)]
pub(super) struct Window {
    opened: u64,
    closes: u64,
    expires: u64,
    last: u64,
    valid: bool,
    first_event: String,
    voters: BTreeSet<String>,
    events: BTreeSet<String>,
    rest: u16,
    play: u16,
}
pub(super) struct Winner {
    pub kind: ImpulseKind,
    pub opened: u64,
    pub expires: u64,
    pub first_event: String,
}
impl Window {
    pub(super) fn new(now: u64, expires: u64, first_event: &str) -> Option<Self> {
        let closes = now
            .checked_add(WINDOW_MS)
            .filter(|n| *n < expires && *n <= MAX_TIME)?;
        if expires > MAX_TIME || expires.saturating_sub(now) > 30_000 || !valid_id(first_event) {
            return None;
        }
        Some(Self {
            opened: now,
            closes,
            expires,
            last: now,
            valid: true,
            first_event: first_event.into(),
            voters: BTreeSet::new(),
            events: BTreeSet::new(),
            rest: 0,
            play: 0,
        })
    }
    fn advance(&mut self, now: u64) -> bool {
        if now < self.last || now > MAX_TIME {
            self.valid = false;
        }
        self.last = self.last.max(now);
        self.valid
    }
    pub(super) fn cast(
        &mut self,
        voter: &str,
        event: &str,
        choice: Choice,
        now: u64,
        expires: u64,
    ) -> bool {
        if !self.advance(now)
            || now >= self.closes
            || expires <= self.closes
            || expires > MAX_TIME
            || expires.saturating_sub(now) > 30_000
            || !valid_id(voter)
            || !valid_id(event)
            || self.voters.len() >= MAX_VOTERS
            || self.voters.contains(voter)
            || self.events.contains(event)
        {
            return false;
        }
        self.voters.insert(voter.into());
        self.events.insert(event.into());
        self.expires = self.expires.min(expires);
        match choice {
            Choice::Rest => self.rest += 1,
            Choice::Play => self.play += 1,
        };
        true
    }
    /// Removal is due at a fixed deadline or immediately after clock regression.
    pub(super) fn due(&mut self, now: u64) -> bool {
        !self.advance(now) || now >= self.closes
    }
    pub(super) fn finish(mut self, now: u64) -> Option<Winner> {
        if !self.advance(now) || now < self.closes || now >= self.expires || self.rest == self.play
        {
            return None;
        }
        Some(Winner {
            kind: if self.rest > self.play {
                ImpulseKind::VoteRest
            } else {
                ImpulseKind::VotePlay
            },
            opened: self.opened,
            expires: self.expires,
            first_event: self.first_event,
        })
    }
    pub(super) fn counts(&self, now: u64) -> Option<(u16, u16, u32)> {
        (self.valid && now >= self.last && now < self.closes)
            .then(|| (self.rest, self.play, (self.closes - now) as u32))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn window() -> Window {
        Window::new(100, 30_100, "message_first").unwrap()
    }
    #[test]
    fn fixed_window_accepts_only_one_vote_per_voter_and_event() {
        let mut w = window();
        assert!(w.cast("voter_a", "message_a", Choice::Play, 100, 30_100));
        assert!(!w.cast("voter_a", "message_b", Choice::Rest, 200, 30_100));
        assert!(!w.cast("voter_b", "message_a", Choice::Rest, 300, 30_100));
        assert_eq!(w.counts(400), Some((0, 1, 14_700)));
        let winner = w.finish(15_100).unwrap();
        assert_eq!(winner.kind, ImpulseKind::VotePlay);
        assert_eq!(winner.opened, 100);
        assert_eq!(winner.expires, 30_100);
        assert_eq!(winner.first_event, "message_first");
    }
    #[test]
    fn repeated_votes_never_move_the_deadline() {
        let mut w = window();
        for n in 0..20 {
            w.cast(
                &format!("voter_{n}"),
                &format!("message_{n}"),
                Choice::Rest,
                100 + n * 100,
                30_100,
            );
        }
        assert_eq!(w.counts(15_099), Some((20, 0, 1)));
        assert!(w.due(15_100));
        assert!(!w.cast("late", "late_event", Choice::Play, 15_100, 30_100));
        assert_eq!(w.finish(15_101).unwrap().kind, ImpulseKind::VoteRest);
    }
    #[test]
    fn ties_empty_early_expired_and_regressed_windows_never_propose_actions() {
        assert!(window().finish(15_100).is_none());
        let mut tie = window();
        tie.cast("a", "event_a", Choice::Rest, 100, 30_100);
        tie.cast("b", "event_b", Choice::Play, 101, 30_100);
        assert!(tie.finish(15_100).is_none());
        let mut early = window();
        early.cast("a", "event_a", Choice::Play, 100, 30_100);
        assert!(early.clone().finish(15_099).is_none());
        assert!(early.clone().finish(30_100).is_none());
        assert!(early.due(99));
        assert!(early.finish(15_100).is_none());
    }
    #[test]
    fn bounded_voters_and_expiry_do_not_grow_with_chat_volume() {
        let mut w = window();
        for n in 0..MAX_VOTERS {
            assert!(w.cast(
                &format!("voter_{n}"),
                &format!("event_{n}"),
                Choice::Play,
                100,
                30_100
            ));
        }
        assert!(!w.cast("overflow", "overflow", Choice::Rest, 100, 30_100));
        assert_eq!(w.counts(100), Some((0, 256, 15_000)));
        assert!(Window::new(100, 15_100, "event").is_none());
        assert!(Window::new(MAX_TIME - 100, MAX_TIME, "event").is_none());
    }
    #[test]
    fn each_accepted_vote_retains_its_real_expiry_and_malformed_votes_do_not_alter_counts() {
        let mut w = window();
        assert!(w.cast("a", "event_a", Choice::Rest, 100, 30_100));
        assert!(w.cast("b", "event_b", Choice::Rest, 101, 16_000));
        assert!(!w.cast("c", "event_c", Choice::Play, 102, 15_100));
        assert!(!w.cast("bad/sender", "event_d", Choice::Play, 103, 30_100));
        assert_eq!(w.finish(15_100).unwrap().expires, 16_000);
    }
}
