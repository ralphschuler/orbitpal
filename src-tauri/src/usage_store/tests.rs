//! Actual Store + production metered dispatch seam, with local mock futures.
//! No AppHandle, model network, CLI process or native credential store is used.
use super::*;
use crate::usage_budget::{Origin, Totals};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("orbitpal-usage-store-{}", uuid::Uuid::new_v4())))
    }
    fn store(&self) -> Store {
        Store::new(self.0.clone()).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn context(origin: Origin) -> Context {
    Context::new("nova", origin)
}

#[tokio::test]
async fn production_dispatch_seam_admits_each_mocked_round_before_polling_transport() {
    let fixture = Fixture::new();
    let store = fixture.store();
    let limits = Limits {
        daily_dialog_requests: 2,
        ..Limits::default()
    };
    store.set_usage_limits(&limits).unwrap();
    let sent = AtomicUsize::new(0);
    for round in 0..3 {
        let result = store
            .metered_usage(
                &context(Origin::Autopilot),
                Cost::dialog(1, 10, Some(500)),
                async {
                    sent.fetch_add(1, Ordering::SeqCst);
                    (Ok("mock response"), Some(20))
                },
            )
            .await;
        if round < 2 {
            assert_eq!(result.unwrap(), "mock response");
        } else {
            assert_eq!(result.unwrap_err(), Denial::DialogLimit.message());
        }
    }
    assert_eq!(sent.load(Ordering::SeqCst), 2);
    let totals = store.usage_snapshot().unwrap().total;
    assert_eq!(totals.dialog_requests, 2);
    assert_eq!(totals.image_count, 2);
    assert_eq!(totals.image_bytes, 20);
    assert_eq!(totals.charged_tokens, 40);
}

#[tokio::test]
async fn dropped_dispatch_future_keeps_charge_and_reopen_does_not_resume_it() {
    let fixture = Fixture::new();
    let store = fixture.store();
    store
        .set_usage_limits(&Limits {
            daily_dialog_requests: 1,
            hard_token_limit: Some(500),
            ..Limits::default()
        })
        .unwrap();
    let polled = AtomicUsize::new(0);
    let ctx = context(Origin::AutomaticChat);
    let work = store.metered_usage::<()>(&ctx, Cost::dialog(0, 0, Some(500)), async {
        polled.fetch_add(1, Ordering::SeqCst);
        std::future::pending::<(Result<(), String>, Option<u64>)>().await
    });
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(5), work)
            .await
            .is_err()
    );
    let totals = store.usage_snapshot().unwrap().total;
    assert_eq!(totals.pending_requests, 1);
    assert_eq!(totals.charged_tokens, 500);
    drop(store);
    let reopened = fixture.store();
    let totals = reopened.usage_snapshot().unwrap().total;
    assert_eq!(totals.pending_requests, 0);
    assert_eq!(totals.unknown_token_requests, 1);
    assert_eq!(totals.charged_tokens, 500);
    let result = reopened
        .metered_usage(
            &context(Origin::Autopilot),
            Cost::dialog(0, 0, Some(1)),
            async {
                polled.fetch_add(1, Ordering::SeqCst);
                (Ok(()), Some(1))
            },
        )
        .await;
    assert!(result.is_err());
    assert_eq!(polled.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn optional_corruption_never_disables_manual_profile_repair_or_charges_manual_work() {
    let fixture = Fixture::new();
    let store = fixture.store();
    let ticket = store
        .start_usage(&context(Origin::Autopilot), Cost::dialog(0, 0, None))
        .unwrap();
    store.finish_usage(ticket.as_ref(), None).unwrap();
    store
        .db
        .lock()
        .unwrap()
        .execute(
            "UPDATE usage_budget_settings SET settings_json='invalid-private-marker'",
            [],
        )
        .unwrap();
    drop(store);
    let store = fixture.store();
    assert_eq!(store.profile().unwrap()["id"], "nova");
    assert!(store.usage_snapshot().is_err());
    let polled = AtomicUsize::new(0);
    assert!(store
        .metered_usage(
            &context(Origin::Autopilot),
            Cost::dialog(0, 0, None),
            async {
                polled.fetch_add(1, Ordering::SeqCst);
                (Ok(()), None)
            }
        )
        .await
        .is_err());
    store
        .metered_usage(&context(Origin::Manual), Cost::dialog(0, 0, None), async {
            polled.fetch_add(1, Ordering::SeqCst);
            (Ok(()), None)
        })
        .await
        .unwrap();
    assert_eq!(polled.load(Ordering::SeqCst), 1);
    assert_eq!(
        store
            .set_usage_limits(&Limits::default())
            .unwrap()
            .total
            .dialog_requests,
        1
    );
    store
        .db
        .lock()
        .unwrap()
        .execute("UPDATE usage_budget_meta SET version=99", [])
        .unwrap();
    drop(store);
    let store = fixture.store();
    assert!(store.usage_snapshot().is_err());
    assert!(store.set_usage_limits(&Limits::default()).is_err());
    assert_eq!(store.profile().unwrap()["id"], "nova");
    store
        .metered_usage(&context(Origin::Manual), Cost::memory(false), async {
            (Ok(()), None)
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn scoped_bot_identity_is_checked_before_dispatch_and_exports_have_no_budget_ledger() {
    let fixture = Fixture::new();
    let store = fixture.store();
    let other = store.create_bot("James", true).unwrap();
    let id = other["id"].as_str().unwrap();
    let scoped = store.scoped("nova").unwrap();
    let polled = AtomicUsize::new(0);
    assert!(scoped
        .metered_usage(
            &Context::new(id, Origin::Autopilot),
            Cost::memory(false),
            async {
                polled.fetch_add(1, Ordering::SeqCst);
                (Ok(()), None)
            }
        )
        .await
        .is_err());
    assert_eq!(polled.load(Ordering::SeqCst), 0);
    store
        .metered_usage(
            &context(Origin::Autopilot),
            Cost::dialog(0, 0, Some(100)),
            async { (Ok(()), Some(5)) },
        )
        .await
        .unwrap();
    assert!(scoped.usage_snapshot().is_err());
    assert!(scoped.set_usage_limits(&Limits::default()).is_err());
    let output = fixture.0.join("fixture-export.json");
    store.export(&output).unwrap();
    let package: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap();
    for forbidden in [
        "usageBudget",
        "chargedTokens",
        "requestAccounting",
        "usage_budget_reservations",
    ] {
        assert!(!package.to_string().contains(forbidden));
    }
    assert_eq!(store.usage_snapshot().unwrap().total.dialog_requests, 1);
}

#[tokio::test]
async fn strict_cli_preflight_and_unknown_history_survive_real_store_reopen() {
    use crate::usage_budget::RequestAccounting;
    let fixture = Fixture::new();
    let store = fixture.store();
    store
        .set_usage_limits(&Limits {
            request_accounting: RequestAccounting::Direct,
            ..Limits::default()
        })
        .unwrap();
    let cli = serde_json::json!({"provider":"codex","model":"default"});
    assert_eq!(
        crate::providers::preflight_budget(&store, &context(Origin::AutomaticChat), &cli)
            .unwrap_err(),
        Denial::RequestAccountingUnsupported.message()
    );
    assert_eq!(store.usage_snapshot().unwrap().total, Totals::default());
    store.set_usage_limits(&Limits::default()).unwrap();
    store
        .metered_usage(
            &context(Origin::AutomaticChat),
            Cost::cli_dialog(0, 0),
            async { (Ok(()), None) },
        )
        .await
        .unwrap();
    drop(store);
    let store = fixture.store();
    let snapshot = store
        .set_usage_limits(&Limits {
            request_accounting: RequestAccounting::Direct,
            hard_token_limit: Some(1_000_000),
            ..Limits::default()
        })
        .unwrap();
    assert_eq!(snapshot.total.dialog_requests, 1);
    let direct = serde_json::json!({"provider":"openai","model":"gpt-4o-2024-08-06"});
    assert_eq!(
        crate::providers::preflight_budget(&store, &context(Origin::Autopilot), &direct)
            .unwrap_err(),
        Denial::RequestHistoryUnbounded.message()
    );
    store
        .set_usage_limits(&Limits {
            hard_token_limit: Some(1_000_000),
            ..Limits::default()
        })
        .unwrap();
    assert_eq!(
        crate::providers::preflight_budget(&store, &context(Origin::Autopilot), &direct)
            .unwrap_err(),
        Denial::TokenHistoryUnbounded.message()
    );
}

#[tokio::test]
async fn possibly_sent_mock_transport_error_keeps_unknown_upper_charged() {
    let fixture = Fixture::new();
    let store = fixture.store();
    let error = store
        .metered_usage::<()>(
            &context(Origin::Autopilot),
            Cost::dialog(0, 0, Some(500)),
            async { (Err("synthetic transport failure".into()), None) },
        )
        .await
        .unwrap_err();
    assert_eq!(error, "synthetic transport failure");
    let total = store.usage_snapshot().unwrap().total;
    assert_eq!(total.dialog_requests, 1);
    assert_eq!(total.charged_tokens, 500);
    assert_eq!(total.unknown_token_requests, 1);
    assert_eq!(total.pending_requests, 0);
}
