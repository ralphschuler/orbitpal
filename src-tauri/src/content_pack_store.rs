//! Revalidation under the same actual Life SQLite transaction as the Plan/phase.
use crate::{life_domain::Activity,world_state::AnchorKind,life_runtime::content_host::ContentUse};
pub(crate) fn current(db:&rusqlite::Connection,usage:&ContentUse,kind:Option<AnchorKind>,activity:Activity)->bool {
    kind.zip(super::experience_store::read_content_catalog(db).ok()).is_some_and(|(kind,packs)|crate::life_runtime::content_host::definition_current(&packs,usage,kind,activity))
}
