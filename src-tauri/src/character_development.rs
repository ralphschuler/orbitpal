//! Bounded character data. Only native confirmed activities earn progress.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HobbyKind {
    Play,
    Explore,
    Company,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Traits {
    pub initiative: u16,
    pub sociability: u16,
    pub patience: u16,
    pub rest_preference: u16,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Interests {
    pub play: u16,
    pub explore: u16,
    pub company: u16,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Hobby {
    pub kind: HobbyKind,
    pub experience: u16,
    pub variation: u8,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GoalStatus {
    Active,
    Paused,
    Achieved,
    Discarded,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Goal {
    pub id: String,
    pub kind: HobbyKind,
    pub target: u16,
    pub progress: u16,
    pub status: GoalStatus,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Routine {
    pub start_minute: u16,
    pub end_minute: u16,
    pub kind: HobbyKind,
    pub weight: u16,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Character {
    pub version: u8,
    pub traits: Traits,
    pub interests: Interests,
    pub hobbies: Vec<Hobby>,
    pub goal: Option<Goal>,
    pub routines: Vec<Routine>,
}
impl Default for Character {
    fn default() -> Self {
        Self {
            version: 1,
            traits: Traits {
                initiative: 500,
                sociability: 500,
                patience: 500,
                rest_preference: 500,
            },
            interests: Interests {
                play: 500,
                explore: 500,
                company: 500,
            },
            hobbies: [HobbyKind::Play, HobbyKind::Explore, HobbyKind::Company]
                .into_iter()
                .map(|kind| Hobby {
                    kind,
                    experience: 0,
                    variation: 0,
                })
                .collect(),
            goal: None,
            routines: vec![],
        }
    }
}
impl Character {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.version != 1
            || [
                self.traits.initiative,
                self.traits.sociability,
                self.traits.patience,
                self.traits.rest_preference,
                self.interests.play,
                self.interests.explore,
                self.interests.company,
            ]
            .iter()
            .any(|v| *v > 1000)
            || self.hobbies.len() != 3
            || self.routines.len() > 8
        {
            return Err("invalid character data".into());
        }
        let mut kinds = BTreeSet::new();
        for hobby in &self.hobbies {
            if !kinds.insert(hobby.kind)
                || hobby.experience > 10000
                || hobby.variation != variation(hobby.experience)
            {
                return Err("invalid hobby data".into());
            }
        }
        if self.routines.iter().any(|r| {
            r.start_minute >= 1440
                || r.end_minute >= 1440
                || r.start_minute == r.end_minute
                || r.weight == 0
                || r.weight > 1000
        }) {
            return Err("invalid routine data".into());
        }
        if let Some(g) = &self.goal {
            if !crate::storage::valid_id(&g.id)
                || g.target == 0
                || g.target > 100
                || g.progress > g.target
                || (g.status == GoalStatus::Achieved && g.progress != g.target)
            {
                return Err("invalid development goal".into());
            }
        }
        Ok(())
    }
    pub(crate) fn preference(&self, kind: HobbyKind, minute: u16) -> u16 {
        let base = match kind {
            HobbyKind::Play => self.interests.play,
            HobbyKind::Explore => self.interests.explore,
            HobbyKind::Company => self.interests.company,
        };
        let routine = self
            .routines
            .iter()
            .filter(|r| {
                r.kind == kind
                    && if r.start_minute < r.end_minute {
                        minute >= r.start_minute && minute < r.end_minute
                    } else {
                        minute >= r.start_minute || minute < r.end_minute
                    }
            })
            .map(|r| r.weight)
            .max()
            .unwrap_or(0);
        base.saturating_add(routine).min(2000)
    }
    /// Called only inside the Store transaction after its confirmed-event dedup.
    pub(crate) fn credit(&mut self, kind: HobbyKind) {
        if let Some(h) = self.hobbies.iter_mut().find(|h| h.kind == kind) {
            h.experience = h.experience.saturating_add(25).min(10000);
            h.variation = variation(h.experience);
        }
        let interest = match kind {
            HobbyKind::Play => &mut self.interests.play,
            HobbyKind::Explore => &mut self.interests.explore,
            HobbyKind::Company => &mut self.interests.company,
        };
        *interest = interest.saturating_add(1).min(1000);
        if let Some(g) = self
            .goal
            .as_mut()
            .filter(|g| g.kind == kind && g.status == GoalStatus::Active)
        {
            g.progress = g.progress.saturating_add(1).min(g.target);
            if g.progress == g.target {
                g.status = GoalStatus::Achieved;
            }
        }
    }
    pub(crate) fn reconcile_goal(&mut self, available: &[HobbyKind]) {
        if let Some(g) = self.goal.as_mut() {
            if matches!(g.status, GoalStatus::Active | GoalStatus::Paused) {
                g.status = if available.contains(&g.kind) {
                    GoalStatus::Active
                } else {
                    GoalStatus::Paused
                };
                return;
            }
            if g.status == GoalStatus::Achieved {
                return;
            }
        }
        if let Some(kind) = available
            .iter()
            .copied()
            .max_by_key(|kind| (self.preference(*kind, 0), std::cmp::Reverse(*kind)))
        {
            self.goal = Some(Goal {
                id: format!("practice_{kind:?}").to_lowercase(),
                kind,
                target: 5,
                progress: 0,
                status: GoalStatus::Active,
            });
        }
    }
    /// User edits may influence traits/interests/routines or discard a goal, never forge earned progress.
    pub(crate) fn edit(&mut self, requested: Character) -> Result<(), String> {
        requested.validate()?;
        if requested.hobbies != self.hobbies {
            return Err("earned development cannot be edited".into());
        }
        if requested.goal != self.goal
            && requested.goal.as_ref().is_some_and(|g| {
                self.goal.as_ref().is_none_or(|old| {
                    g.id != old.id
                        || g.kind != old.kind
                        || g.target != old.target
                        || g.progress != old.progress
                        || g.status != GoalStatus::Discarded
                })
            })
        {
            return Err("goal progress cannot be edited".into());
        }
        self.traits = requested.traits;
        self.interests = requested.interests;
        self.routines = requested.routines;
        self.goal = requested.goal;
        Ok(())
    }
    pub(crate) fn reset_development(&mut self) {
        for h in &mut self.hobbies {
            h.experience = 0;
            h.variation = 0;
        }
        self.goal = None;
    }
}
fn variation(experience: u16) -> u8 {
    if experience >= 5000 {
        3
    } else if experience >= 1000 {
        2
    } else if experience >= 250 {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn different_traits_and_soft_routines_change_preferences() {
        let mut a = Character::default();
        a.interests.play = 900;
        let mut b = a.clone();
        b.interests.play = 100;
        b.interests.company = 900;
        assert!(a.preference(HobbyKind::Play, 0) > b.preference(HobbyKind::Play, 0));
        a.routines.push(Routine {
            start_minute: 1380,
            end_minute: 60,
            kind: HobbyKind::Company,
            weight: 900,
        });
        assert!(a.preference(HobbyKind::Company, 30) > a.preference(HobbyKind::Company, 120));
    }
    #[test]
    fn successful_hobby_unlocks_variation_without_absence_loss() {
        let mut c = Character::default();
        c.reconcile_goal(&[HobbyKind::Play]);
        for _ in 0..10 {
            c.credit(HobbyKind::Play);
        }
        assert_eq!(c.hobbies[0].variation, 1);
        assert_eq!(c.goal.as_ref().unwrap().status, GoalStatus::Achieved);
        let encoded = serde_json::to_string(&c).unwrap();
        assert_eq!(serde_json::from_str::<Character>(&encoded).unwrap(), c);
    }
    #[test]
    fn unreachable_goal_pauses_without_blocking_other_choices() {
        let mut c = Character::default();
        c.reconcile_goal(&[HobbyKind::Play]);
        c.reconcile_goal(&[HobbyKind::Explore]);
        assert_eq!(c.goal.as_ref().unwrap().status, GoalStatus::Paused);
        assert!(c.preference(HobbyKind::Explore, 0) > 0);
    }
    #[test]
    fn invalid_and_forged_progress_edits_are_refused() {
        let mut c = Character::default();
        let mut bad = c.clone();
        bad.traits.initiative = 1001;
        assert!(c.edit(bad).is_err());
        let mut bad = c.clone();
        bad.hobbies[0].experience = 250;
        bad.hobbies[0].variation = 1;
        assert!(c.edit(bad).is_err());
    }
}
