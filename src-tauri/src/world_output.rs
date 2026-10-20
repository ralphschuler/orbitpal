//! Ephemeral, Core-owned Studio World output observations. Registrations are
//! never authority to start Life, allocate an action or mutate persistent state.
#[path = "world_output/state.rs"]
mod state;
#[path = "world_output/wire.rs"]
pub(crate) mod wire;
pub(crate) use state::*;
pub(crate) use wire::*;

#[path = "world_output/execution.rs"]
mod execution;
pub(crate) use execution::VisualExecution;
#[cfg(test)]
mod tests;
#[cfg(test)]
#[path = "world_output/native_policy_tests.rs"]
mod native_policy_tests;
