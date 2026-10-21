//! Revalidation under the same actual Life SQLite transaction as the Plan/phase.
use crate::{
    life_domain::Activity, life_runtime::content_host::ContentUse, world_state::AnchorKind,
};
pub(crate) fn current(
    db: &rusqlite::Connection,
    usage: &ContentUse,
    kind: Option<AnchorKind>,
    activity: Activity,
) -> bool {
    kind.zip(super::experience_store::read_content_catalog(db).ok())
        .is_some_and(|(kind, packs)| {
            crate::life_runtime::content_host::definition_current(&packs, usage, kind, activity)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn phase_transaction_rejects_deactivation_and_reactivation_of_the_same_template() {
        let root =
            std::env::temp_dir().join(format!("orbitpal-content-use-{}", uuid::Uuid::new_v4()));
        let store = crate::storage::Store::new(root.clone()).unwrap();
        store
            .install_content_pack(
                crate::content_packs::Manifest::parse(include_str!(
                    "../../public/content-packs/desk-garden.json"
                ))
                .unwrap(),
            )
            .unwrap();
        let packs = store
            .set_content_pack_active("desk-garden", 1, true)
            .unwrap();
        let reference = crate::content_packs::ContentRef {
            package_id: "desk-garden".into(),
            package_version: 1,
            template_id: packs[0]
                .manifest
                .objects
                .iter()
                .find(|object| object.kind == crate::content_packs::ObjectKind::Play)
                .unwrap()
                .id
                .clone(),
        };
        let usage = crate::life_runtime::content_host::resolve(
            &packs,
            &reference,
            AnchorKind::Play,
            Activity::Play,
        )
        .unwrap();
        {
            let db = store.db.lock().unwrap();
            assert!(current(&db, &usage, Some(AnchorKind::Play), Activity::Play));
        }
        store
            .set_content_pack_active("desk-garden", 2, false)
            .unwrap();
        {
            let db = store.db.lock().unwrap();
            assert!(!current(
                &db,
                &usage,
                Some(AnchorKind::Play),
                Activity::Play
            ));
        }
        let packs = store
            .set_content_pack_active("desk-garden", 3, true)
            .unwrap();
        {
            let db = store.db.lock().unwrap();
            assert!(!current(
                &db,
                &usage,
                Some(AnchorKind::Play),
                Activity::Play
            ));
        }
        assert!(crate::life_runtime::content_host::resolve(
            &packs,
            &reference,
            AnchorKind::Play,
            Activity::Play
        )
        .is_some());
        drop(store);
        let _ = std::fs::remove_dir_all(root);
    }
}
