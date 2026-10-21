//! Pure advisory self-care selection. Nothing here starts or acknowledges work.
//! The native caller must supply confirmed Life state and currently admissible
//! offers, then independently recheck session, world, capability and ownership.
//! Offer presence and CurrentChoice are observations, never authority or proof
//! of station use. Snack/social execution, need changes and persistence are absent.
use crate::life_domain::{ActorId, LifeState, Needs, WorldId, MAX_COUNTER};
use crate::world_state::SlotId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Versioned operator product data. This is preference data, never activation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CareConfig {
    pub version: u16,
    pub decay_per_minute: [u16; 4],
    pub restore_units: [u16; 4],
    pub start_below: [u16; 4],
    pub satisfied_at: [u16; 4],
    pub weights: [u16; 4],
    pub priorities: [u16; 4],
    pub travel_cost_weight: u16,
    pub minimum_duration_ms: u64,
    pub hysteresis: u64,
    pub cooldown_ms: u64,
    pub backoff_ms: u64,
}
impl Default for CareConfig {
    fn default() -> Self {
        Self {
            version: 1,
            decay_per_minute: [180, 250, 220, 150],
            restore_units: [500, 1500, 500, 1000],
            start_below: [6500; 4],
            satisfied_at: [8500; 4],
            weights: [100; 4],
            priorities: [100; 4],
            travel_cost_weight: 10,
            minimum_duration_ms: 4000,
            hysteresis: 1_000_000,
            cooldown_ms: 4000,
            backoff_ms: 15_000,
        }
    }
}
impl CareConfig {
    pub fn validate(&self) -> Result<(), Error> {
        if self.version != 1
            || self.decay_per_minute.iter().any(|rate| *rate > 1000)
            || self
                .restore_units
                .iter()
                .any(|units| !(1..=5000).contains(units))
        {
            return Err(Error::Policy);
        }
        self.policy()?;
        self.preferences()?;
        Ok(())
    }
    pub fn policy(&self) -> Result<Policy, Error> {
        let mut rules = [Rule::new(1, 2, 1)?; 4];
        for (index, rule) in rules.iter_mut().enumerate() {
            *rule = Rule::new(
                self.start_below[index],
                self.satisfied_at[index],
                self.weights[index],
            )?;
        }
        Policy::new(
            rules,
            self.minimum_duration_ms,
            self.hysteresis,
            self.cooldown_ms,
            self.backoff_ms,
        )
    }
    pub fn preferences(&self) -> Result<Preferences, Error> {
        Preferences::new(self.priorities, self.travel_cost_weight)
    }
    /// Called only for elapsed time admitted by the current explicitly started
    /// simulation. Restart/offline recovery retains its existing clock-only cap.
    pub fn decay(&self, elapsed_ms: u64) -> Result<crate::life_domain::NeedEffect, Error> {
        self.validate()?;
        if elapsed_ms > crate::life_domain::MAX_ELAPSED_MS {
            return Err(Error::Clock);
        }
        let values = self
            .decay_per_minute
            .map(|rate| -((u64::from(rate) * elapsed_ms / 60_000) as i32));
        crate::life_domain::NeedEffect::new(values[0], values[1], values[2], values[3])
            .map_err(|_| Error::Policy)
    }
    pub fn chooser(&self, state: &LifeState) -> Result<Chooser, Error> {
        self.validate()?;
        Ok(Chooser::new(state, self.policy()?, self.preferences()?))
    }
}

pub(crate) const MAX_OFFERS: usize = 32;
const MAX_DELAY_MS: u64 = 600_000;
const MAX_SCORE: u64 = 10_000_000_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Error {
    Policy,
    Preferences,
    Offer,
    Capacity,
    Clock,
    Snapshot,
    CurrentChoice,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum Need {
    Energy,
    Nourishment,
    Engagement,
    Social,
}
impl Need {
    fn index(self) -> usize {
        self as usize
    }
    fn level(self, needs: Needs) -> u16 {
        match self {
            Self::Energy => needs.energy(),
            Self::Nourishment => needs.nourishment(),
            Self::Engagement => needs.engagement(),
            Self::Social => needs.social(),
        }
        .units()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Rule {
    start_below: u16,
    satisfied_at: u16,
    weight: u16,
}
impl Rule {
    pub(crate) fn new(start_below: u16, satisfied_at: u16, weight: u16) -> Result<Self, Error> {
        if start_below == 0
            || start_below >= satisfied_at
            || satisfied_at > 10_000
            || !(1..=1000).contains(&weight)
        {
            return Err(Error::Policy);
        }
        Ok(Self {
            start_below,
            satisfied_at,
            weight,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Policy {
    rules: [Rule; 4],
    minimum_duration_ms: u64,
    hysteresis: u64,
    cooldown_ms: u64,
    backoff_ms: u64,
}
impl Policy {
    pub(crate) fn new(
        rules: [Rule; 4],
        minimum_duration_ms: u64,
        hysteresis: u64,
        cooldown_ms: u64,
        backoff_ms: u64,
    ) -> Result<Self, Error> {
        if [minimum_duration_ms, cooldown_ms, backoff_ms]
            .iter()
            .any(|ms| !(1..=MAX_DELAY_MS).contains(ms))
            || hysteresis > MAX_SCORE
        {
            return Err(Error::Policy);
        }
        Ok(Self {
            rules,
            minimum_duration_ms,
            hysteresis,
            cooldown_ms,
            backoff_ms,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Preferences {
    priorities: [u16; 4],
    travel_cost_weight: u16,
}
impl Preferences {
    pub(crate) fn new(priorities: [u16; 4], travel_cost_weight: u16) -> Result<Self, Error> {
        if priorities.iter().any(|value| !(1..=1000).contains(value)) || travel_cost_weight > 1000 {
            return Err(Error::Preferences);
        }
        Ok(Self {
            priorities,
            travel_cost_weight,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Target {
    Slot(SlotId),
    Actor(ActorId),
    Here,
}
impl Ord for Target {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            (Self::Slot(left), Self::Slot(right)) => left.cmp(right),
            (Self::Actor(left), Self::Actor(right)) => left.as_str().cmp(right.as_str()),
            (Self::Slot(_), Self::Actor(_)) => std::cmp::Ordering::Less,
            (Self::Actor(_), Self::Slot(_)) => std::cmp::Ordering::Greater,
            (Self::Here, Self::Here) => std::cmp::Ordering::Equal,
            (Self::Here, _) => std::cmp::Ordering::Greater,
            (_, Self::Here) => std::cmp::Ordering::Less,
        }
    }
}
impl PartialOrd for Target {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct OfferKey {
    need: Need,
    target: Target,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Offer {
    key: OfferKey,
    travel_cost: u16,
}
impl Offer {
    pub(crate) fn new(need: Need, target: Target, travel_cost: u16) -> Result<Self, Error> {
        if travel_cost > 1000 || matches!(&target, Target::Actor(_)) && need != Need::Social {
            return Err(Error::Offer);
        }
        Ok(Self {
            key: OfferKey { need, target },
            travel_cost,
        })
    }
    pub(crate) fn key(&self) -> &OfferKey {
        &self.key
    }
    pub(crate) fn need(&self) -> Need {
        self.key.need
    }
    pub(crate) fn target(&self) -> &Target {
        &self.key.target
    }
}

/// Caller-reported current choice in the caller's monotonic policy clock.
/// The clock must exclude suspended simulation time; this is not active-frame
/// evidence. Reusing the same key with a new start requires observing its end.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CurrentChoice {
    key: OfferKey,
    since_ms: u64,
}
impl CurrentChoice {
    pub(crate) fn new(key: OfferKey, since_ms: u64) -> Result<Self, Error> {
        if since_ms > MAX_COUNTER {
            return Err(Error::Clock);
        }
        Ok(Self { key, since_ms })
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Selection {
    New,
    MinimumDuration,
    Hysteresis,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Proposal {
    offer: Offer,
    score: u64,
    selection: Selection,
}
impl Proposal {
    pub(crate) fn offer(&self) -> &Offer {
        &self.offer
    }
    pub(crate) fn selection(&self) -> Selection {
        self.selection
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum End {
    Finished,
    Interrupted,
    Unavailable,
}

/// One actor/world's bounded, process-only preference memory, not saved state.
/// Slot IDs have no epoch/revision authority. Recreate this chooser when native
/// World context changes; a later host must bind proposals to that exact context.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Chooser {
    actor: ActorId,
    world: WorldId,
    revision: u64,
    needs: Needs,
    policy: Policy,
    preferences: Preferences,
    last_ms: u64,
    cooldown_until: [u64; 4],
    blocked: Vec<(OfferKey, u64)>,
    current: Option<CurrentChoice>,
}
impl Chooser {
    pub(crate) fn new(confirmed: &LifeState, policy: Policy, preferences: Preferences) -> Self {
        Self {
            actor: confirmed.actor_id().clone(),
            world: confirmed.world_id().clone(),
            revision: confirmed.revision(),
            needs: confirmed.needs(),
            policy,
            preferences,
            last_ms: 0,
            cooldown_until: [0; 4],
            blocked: Vec::new(),
            current: None,
        }
    }
    fn transact<T>(
        &mut self,
        now: u64,
        operation: impl FnOnce(&mut Self) -> Result<T, Error>,
    ) -> Result<T, Error> {
        if now < self.last_ms || now > MAX_COUNTER {
            return Err(Error::Clock);
        }
        let mut next = self.clone();
        next.last_ms = now;
        next.blocked.retain(|(_, until)| *until > now);
        let result = operation(&mut next)?;
        *self = next;
        Ok(result)
    }
    fn deadline(now: u64, delay: u64) -> Result<u64, Error> {
        now.checked_add(delay)
            .filter(|until| *until <= MAX_COUNTER)
            .ok_or(Error::Clock)
    }
    fn available(&self, offer: &Offer, now: u64) -> bool {
        self.cooldown_until[offer.need().index()] <= now
            && !self.blocked.iter().any(|(key, _)| key == offer.key())
    }
    fn score(&self, needs: Needs, offer: &Offer) -> u64 {
        let index = offer.need().index();
        let rule = self.policy.rules[index];
        let pressure = u64::from(rule.satisfied_at.saturating_sub(offer.need().level(needs)));
        (pressure * u64::from(rule.weight) * u64::from(self.preferences.priorities[index]))
            .saturating_sub(
                u64::from(offer.travel_cost) * u64::from(self.preferences.travel_cost_weight),
            )
    }
    pub(crate) fn choose(
        &mut self,
        now: u64,
        confirmed: &LifeState,
        offers: &[Offer],
        current: Option<&CurrentChoice>,
    ) -> Result<Option<Proposal>, Error> {
        if offers.len() > MAX_OFFERS {
            return Err(Error::Capacity);
        }
        let mut unique = BTreeSet::new();
        if offers.iter().any(|offer| !unique.insert(offer.key())) {
            return Err(Error::Offer);
        }
        if confirmed.actor_id() != &self.actor
            || confirmed.world_id() != &self.world
            || confirmed.revision() < self.revision
            || confirmed.revision() == self.revision && confirmed.needs() != self.needs
        {
            return Err(Error::Snapshot);
        }
        if current.is_some_and(|choice| {
            choice.since_ms > now
                || self
                    .current
                    .as_ref()
                    .is_some_and(|old| old.key == choice.key && old != choice)
        }) {
            return Err(Error::CurrentChoice);
        }
        self.transact(now, |next| {
            next.revision = confirmed.revision();
            next.needs = confirmed.needs();
            next.current = current.cloned();
            let needs = confirmed.needs();
            let mut best: Option<(&Offer, u64)> = None;
            for offer in offers {
                if !next.available(offer, now)
                    || offer.need().level(needs)
                        >= next.policy.rules[offer.need().index()].start_below
                {
                    continue;
                }
                let score = next.score(needs, offer);
                if score > 0
                    && best.is_none_or(|(old, value)| {
                        score > value || score == value && offer.key() < old.key()
                    })
                {
                    best = Some((offer, score));
                }
            }
            if let Some(choice) = current {
                if let Some(offer) = offers.iter().find(|offer| {
                    offer.key() == &choice.key
                        && next.available(offer, now)
                        && offer.need().level(needs)
                            < next.policy.rules[offer.need().index()].satisfied_at
                }) {
                    let score = next.score(needs, offer);
                    let selection = if now - choice.since_ms < next.policy.minimum_duration_ms {
                        Some(Selection::MinimumDuration)
                    } else if best
                        .is_none_or(|(_, challenger)| challenger <= score + next.policy.hysteresis)
                    {
                        Some(Selection::Hysteresis)
                    } else {
                        None
                    };
                    if let Some(selection) = selection {
                        return Ok(Some(Proposal {
                            offer: offer.clone(),
                            score,
                            selection,
                        }));
                    }
                }
            }
            Ok(best.map(|(offer, score)| Proposal {
                offer: offer.clone(),
                score,
                selection: Selection::New,
            }))
        })
    }
    /// Observe a real host-owned terminal outcome only to update preference
    /// memory. This does not commit a Life outcome, reward or resource release.
    pub(crate) fn record_end(&mut self, now: u64, key: &OfferKey, end: End) -> Result<(), Error> {
        self.transact(now, |next| {
            let cooldown = Self::deadline(now, next.policy.cooldown_ms)?;
            if end != End::Finished {
                let until = Self::deadline(now, next.policy.backoff_ms)?;
                if let Some((_, existing)) = next.blocked.iter_mut().find(|(old, _)| old == key) {
                    *existing = until;
                } else {
                    if next.blocked.len() == MAX_OFFERS {
                        return Err(Error::Capacity);
                    }
                    next.blocked.push((key.clone(), until));
                }
            }
            next.cooldown_until[key.need.index()] = cooldown;
            if next
                .current
                .as_ref()
                .is_some_and(|choice| &choice.key == key)
            {
                next.current = None;
            }
            Ok(())
        })
    }
}

#[cfg(test)]
#[path = "life_self_care/tests.rs"]
mod tests;
