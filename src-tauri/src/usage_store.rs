//! Storage binding for installation budgets. Manual operations stay usable if
//! optional accounting is unavailable; autonomous consumers always fail closed.
use super::Store;
use crate::usage_budget::{self, Context, Cost, Denial, Limits, Reservation, Snapshot};

impl Store {
    fn usage_ready(&self) -> Result<(), String> {
        self.usage_error
            .map(|error| error.message().to_owned())
            .map_or(Ok(()), Err)
    }
    pub(crate) fn usage_snapshot(&self) -> Result<Snapshot, String> {
        self.usage_ready()?;
        if self.fixed_profile.is_some() {
            return Err(Denial::AccountingUnavailable.message().into());
        }
        usage_budget::snapshot(
            &mut *self
                .db
                .lock()
                .map_err(|_| Denial::AccountingUnavailable.message())?,
            usage_budget::today(),
        )
        .map_err(|e| e.message().into())
    }
    pub(crate) fn set_usage_limits(&self, limits: &Limits) -> Result<Snapshot, String> {
        self.usage_ready()?;
        if self.fixed_profile.is_some() {
            return Err(Denial::AccountingUnavailable.message().into());
        }
        limits
            .validate()
            .map_err(|_| "Ungültige Einstellungen für das autonome Verbrauchsbudget.".to_owned())?;
        usage_budget::configure(
            &mut *self
                .db
                .lock()
                .map_err(|_| Denial::AccountingUnavailable.message())?,
            limits,
            usage_budget::today(),
        )
        .map_err(|e| e.message().into())
    }
    pub(crate) fn start_usage(
        &self,
        context: &Context,
        cost: Cost,
    ) -> Result<Option<Reservation>, String> {
        if !context.origin.autonomous() {
            return Ok(None);
        }
        self.usage_ready()?;
        if self
            .fixed_profile
            .as_deref()
            .is_some_and(|id| id != context.bot_id)
        {
            return Err(Denial::AccountingUnavailable.message().into());
        }
        usage_budget::reserve(
            &mut *self
                .db
                .lock()
                .map_err(|_| Denial::AccountingUnavailable.message())?,
            context,
            cost,
            usage_budget::today(),
        )
        .map(Some)
        .map_err(|e| e.message().into())
    }
    pub(crate) fn preflight_usage(
        &self,
        context: &Context,
        direct: bool,
        upper: Option<u64>,
    ) -> Result<(), String> {
        if !context.origin.autonomous() {
            return Ok(());
        }
        self.usage_ready()?;
        if self
            .fixed_profile
            .as_deref()
            .is_some_and(|id| id != context.bot_id)
        {
            return Err(Denial::AccountingUnavailable.message().into());
        }
        usage_budget::preflight(
            &mut *self
                .db
                .lock()
                .map_err(|_| Denial::AccountingUnavailable.message())?,
            context,
            direct,
            upper,
            usage_budget::today(),
        )
        .map_err(|e| e.message().into())
    }
    pub(crate) fn preflight_usage_cost(&self, context: &Context, cost: Cost) -> Result<(), String> {
        if !context.origin.autonomous() {
            return Ok(());
        }
        self.usage_ready()?;
        if self
            .fixed_profile
            .as_deref()
            .is_some_and(|id| id != context.bot_id)
        {
            return Err(Denial::AccountingUnavailable.message().into());
        }
        usage_budget::preflight_cost(
            &mut *self
                .db
                .lock()
                .map_err(|_| Denial::AccountingUnavailable.message())?,
            context,
            cost,
            usage_budget::today(),
        )
        .map_err(|e| e.message().into())
    }
    pub(crate) fn finish_usage(
        &self,
        ticket: Option<&Reservation>,
        observed: Option<u64>,
    ) -> Result<(), String> {
        let Some(ticket) = ticket else {
            return Ok(());
        };
        self.usage_ready()?;
        usage_budget::settle(
            &mut *self
                .db
                .lock()
                .map_err(|_| Denial::AccountingUnavailable.message())?,
            ticket,
            observed,
        )
        .map_err(|e| e.message().into())
    }
    /// Production dispatch seam used by dialog, TTS and memory adapters. The
    /// transport future cannot be polled until its durable reservation exists.
    /// Dropping this future leaves its possibly-sent charge intact for restart.
    pub(crate) async fn metered_usage<T>(
        &self,
        context: &Context,
        cost: Cost,
        operation: impl std::future::Future<Output = (Result<T, String>, Option<u64>)>,
    ) -> Result<T, String> {
        let ticket = self.start_usage(context, cost)?;
        let (result, observed) = operation.await;
        self.finish_usage(ticket.as_ref(), observed)?;
        result
    }
}

#[cfg(test)]
#[path = "usage_store/tests.rs"]
mod tests;
