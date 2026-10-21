use super::{ActionError, Goal, Presentation};
use crate::life_domain::{Activity, NeedEffect, Needs};

/// Fixed native policy. This is neither model input nor renderer receipt data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Effect {
    energy: i16,
    nourishment: i16,
    engagement: i16,
    social: i16,
}
impl Effect {
    pub(crate) fn care(activity: Activity, goal: &Goal, config: &crate::life_self_care::CareConfig) -> Result<Self, ActionError> {
        config.validate().map_err(|_| ActionError::UnsupportedCapability)?;
        let [energy, nourishment, engagement, social] = config.restore_units.map(|v| v as i16);
        let mut effect = match activity {
            Activity::Rest => Self::new(energy, 0, 0, 0),
            Activity::Recharge => Self::new(energy * 2, 0, 0, 0),
            Activity::Snack => Self::new(0, nourishment, 0, 0),
            Activity::Play => Self::new(0, 0, engagement, 0),
            Activity::Socialize => Self::new(0, 0, 0, social),
            _ => return Err(ActionError::UnsupportedCapability),
        };
        if matches!(goal, Goal::Here(_)) {
            effect = Self::new(effect.energy / 2, effect.nourishment / 2, effect.engagement / 2, effect.social / 2);
        }
        Ok(effect)
    }
    const fn new(energy: i16, nourishment: i16, engagement: i16, social: i16) -> Self {
        Self {
            energy,
            nourishment,
            engagement,
            social,
        }
    }
    pub fn event_value(self) -> NeedEffect {
        NeedEffect::new(
            self.energy.into(),
            self.nourishment.into(),
            self.engagement.into(),
            self.social.into(),
        )
        .expect("fixed bounded native effect")
    }
    /// Verify a trusted Store receipt; this never mutates a LifeState or applies
    /// an additional reward. The actual reducer/SQLite transaction owns effects.
    pub fn matches(self, before: Needs, after: Needs) -> bool {
        fn level(value: u16, delta: i16) -> u16 {
            (i32::from(value) + i32::from(delta)).clamp(0, 10_000) as u16
        }
        after.energy().units() == level(before.energy().units(), self.energy)
            && after.nourishment().units() == level(before.nourishment().units(), self.nourishment)
            && after.engagement().units() == level(before.engagement().units(), self.engagement)
            && after.social().units() == level(before.social().units(), self.social)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Policy {
    pub activity: Activity,
    pub presentation: Presentation,
    pub minimum_active_ms: u64,
    pub maximum_run_ms: u64,
    pub effect: Effect,
}
impl Policy {
    pub fn for_request(
        activity: Activity,
        goal: &Goal,
        presentation: Presentation,
    ) -> Result<Self, ActionError> {
        let (minimum_active_ms, maximum_run_ms, mut effect) = match activity {
            Activity::MoveTo => (0, 30_000, Effect::new(0, 0, 0, 0)),
            Activity::Rest => (10_000, 30_000, Effect::new(500, 0, 0, 0)),
            Activity::Recharge => (15_000, 40_000, Effect::new(1000, 0, 0, 0)),
            Activity::Snack => (8000, 30_000, Effect::new(0, 1500, 0, 0)),
            Activity::Play => (1000, 15_000, Effect::new(0, 0, 500, 0)),
            Activity::Socialize => (4000, 20_000, Effect::new(0, 0, 0, 1000)),
            Activity::LookAt => (200, 5000, Effect::new(0, 0, 0, 0)),
            Activity::Wave => (200, 8000, Effect::new(0, 0, 0, 0)),
            Activity::Pet => (200, 30_000, Effect::new(0, 0, 0, 0)),
        };
        let valid_goal = match activity {
            Activity::MoveTo => matches!(goal, Goal::Point(_) | Goal::Slot { .. }),
            Activity::Rest | Activity::Recharge => matches!(goal, Goal::Here(_)) || matches!(
                goal,
                Goal::Slot {
                    kind: crate::world_state::AnchorKind::Rest,
                    ..
                }
            ),
            Activity::Snack => matches!(goal, Goal::Here(_)) || matches!(goal, Goal::Slot { kind: crate::world_state::AnchorKind::Snack, .. }),
            Activity::Socialize => matches!(goal, Goal::Here(_)) || matches!(goal, Goal::Slot { kind: crate::world_state::AnchorKind::Meeting, .. }),
            Activity::Play => matches!(goal, Goal::Here(_)) || matches!(
                goal,
                Goal::Slot {
                    kind: crate::world_state::AnchorKind::Play,
                    ..
                }
            ),
            Activity::LookAt => matches!(goal, Goal::Point(_) | Goal::Actor { .. }),
            Activity::Wave | Activity::Pet => matches!(goal, Goal::Here(_)),
        };
        if presentation == Presentation::NativeRestDwell {
            if activity != Activity::Rest || !matches!(goal, Goal::Here(_)) {
                return Err(ActionError::UnsupportedCapability);
            }
            return Ok(Self {
                activity,
                presentation,
                minimum_active_ms: 10_000,
                maximum_run_ms: 30_000,
                effect: Effect::new(0, 0, 0, 0),
            });
        }
        if !valid_goal {
            return Err(ActionError::InvalidGoal);
        }
        if matches!(goal, Goal::Here(_)) && matches!(activity, Activity::Rest | Activity::Recharge | Activity::Snack | Activity::Play | Activity::Socialize) {
            effect = Effect::new(effect.energy / 2, effect.nourishment / 2, effect.engagement / 2, effect.social / 2);
        }
        Ok(Self {
            activity,
            presentation,
            minimum_active_ms,
            maximum_run_ms,
            effect,
        })
    }
}
