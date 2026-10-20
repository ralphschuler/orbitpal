use super::*;
use std::sync::{Arc, Barrier};

fn database() -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE bot_profiles(id TEXT PRIMARY KEY,profile TEXT NOT NULL);INSERT INTO bot_profiles VALUES('nova','{}'),('james','{}');").unwrap();
    initialize(&mut db, 20_746).unwrap();
    db
}
fn context(bot: &str) -> Context {
    Context::new(bot, Origin::Autopilot)
}
fn set(db: &mut Connection, change: impl FnOnce(&mut Limits)) {
    let mut value = Limits::default();
    change(&mut value);
    configure(db, &value, 20_746).unwrap();
}
fn view(db: &mut Connection) -> Snapshot {
    snapshot(db, 20_746).unwrap()
}

#[test]
fn defaults_are_finite_and_settings_do_not_reset_spend() {
    let mut db = database();
    let defaults = view(&mut db);
    assert_eq!(defaults.day, "2026-10-20");
    assert_eq!(defaults.total, Totals::default());
    let ticket = reserve(
        &mut db,
        &context("nova"),
        Cost::dialog(1, 200, None),
        20_746,
    )
    .unwrap();
    settle(&mut db, &ticket, None).unwrap();
    set(&mut db, |v| v.daily_dialog_requests = 1);
    assert_eq!(view(&mut db).total.dialog_requests, 1);
    assert!(matches!(
        reserve(&mut db, &context("james"), Cost::dialog(0, 0, None), 20_746),
        Err(Denial::DialogLimit)
    ));
    assert_eq!(view(&mut db).last_denial, Some(Denial::DialogLimit));
    assert_eq!(view(&mut db).bots[0].bot_id, "nova");
    set(&mut db, |v| v.daily_dialog_requests = 2);
    assert!(reserve(&mut db, &context("james"), Cost::dialog(0, 0, None), 20_746).is_ok());
}

#[test]
fn two_actual_sqlite_connections_cannot_spend_one_remaining_slot() {
    let path =
        std::env::temp_dir().join(format!("orbitpal-budget-{}.sqlite", uuid::Uuid::new_v4()));
    let mut db = Connection::open(&path).unwrap();
    db.execute_batch("CREATE TABLE bot_profiles(id TEXT PRIMARY KEY,profile TEXT NOT NULL);INSERT INTO bot_profiles VALUES('nova','{}'),('james','{}');").unwrap();
    initialize(&mut db, 20_746).unwrap();
    set(&mut db, |v| v.daily_dialog_requests = 1);
    let barrier = Arc::new(Barrier::new(3));
    let mut workers = Vec::new();
    for bot in ["nova", "james"] {
        let path = path.clone();
        let barrier = barrier.clone();
        workers.push(std::thread::spawn(move || {
            let mut db = Connection::open(path).unwrap();
            db.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
            barrier.wait();
            reserve(&mut db, &context(bot), Cost::dialog(0, 0, None), 20_746)
        }));
    }
    barrier.wait();
    let results = workers
        .into_iter()
        .map(|w| w.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, Err(Denial::DialogLimit)))
            .count(),
        1
    );
    assert_eq!(view(&mut db).total.dialog_requests, 1);
    drop(db);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn every_mocked_followup_is_admitted_separately_and_denial_prevents_dispatch() {
    // Actual accounting module + mocked dispatch; not a Tauri provider test.
    let mut db = database();
    set(&mut db, |v| v.daily_dialog_requests = 2);
    let mut sent = Vec::new();
    for round in ["tool-request", "tool-followup", "final-answer"] {
        let attempt = reserve(&mut db, &context("nova"), Cost::dialog(1, 10, None), 20_746);
        let Ok(ticket) = attempt else {
            assert_eq!(round, "final-answer");
            break;
        };
        sent.push(round);
        settle(&mut db, &ticket, Some(30)).unwrap();
    }
    assert_eq!(sent, vec!["tool-request", "tool-followup"]);
    let totals = view(&mut db).total;
    assert_eq!(totals.dialog_requests, 2);
    assert_eq!(totals.observed_tokens, 60);
    assert_eq!(totals.image_count, 2);
    assert_eq!(totals.image_bytes, 20);
}

#[test]
fn unknown_error_cancel_and_restart_preserve_possibly_sent_reservations() {
    let mut db = database();
    set(&mut db, |v| {
        v.daily_dialog_requests = 2;
        v.hard_token_limit = Some(2_000);
    });
    let failed = reserve(
        &mut db,
        &context("nova"),
        Cost::dialog(0, 0, Some(1_000)),
        20_746,
    )
    .unwrap();
    settle(&mut db, &failed, None).unwrap();
    let _crashed = reserve(
        &mut db,
        &context("james"),
        Cost::dialog(1, 20, Some(1_000)),
        20_746,
    )
    .unwrap();
    let pending = view(&mut db).total;
    assert_eq!(pending.pending_requests, 1);
    assert_eq!(pending.reserved_tokens, 1_000);
    initialize(&mut db, 20_746).unwrap();
    let loaded = view(&mut db).total;
    assert_eq!(loaded.dialog_requests, 2);
    assert_eq!(loaded.charged_tokens, 2_000);
    assert_eq!(loaded.observed_tokens, 0);
    assert_eq!(loaded.unknown_token_requests, 2);
    assert_eq!(loaded.pending_requests, 0);
    assert_eq!(loaded.reserved_tokens, 0);
    assert!(matches!(
        reserve(
            &mut db,
            &context("nova"),
            Cost::dialog(0, 0, Some(1)),
            20_746
        ),
        Err(Denial::DialogLimit)
    ));
}

#[test]
fn known_usage_releases_only_unused_upper_and_duplicate_receipts_are_noops() {
    let mut db = database();
    set(&mut db, |v| v.hard_token_limit = Some(1_000));
    let ticket = reserve(
        &mut db,
        &context("nova"),
        Cost::dialog(0, 0, Some(1_000)),
        20_746,
    )
    .unwrap();
    assert!(matches!(
        reserve(
            &mut db,
            &context("james"),
            Cost::dialog(0, 0, Some(1)),
            20_746
        ),
        Err(Denial::TokenLimit)
    ));
    settle(&mut db, &ticket, Some(120)).unwrap();
    let before = view(&mut db).total;
    assert_eq!(before.charged_tokens, 120);
    assert_eq!(before.observed_tokens, 120);
    assert_eq!(before.unknown_token_requests, 0);
    settle(&mut db, &ticket, Some(1)).unwrap();
    assert_eq!(view(&mut db).total, before);
    assert!(reserve(
        &mut db,
        &context("james"),
        Cost::dialog(0, 0, Some(880)),
        20_746
    )
    .is_ok());
}

#[test]
fn unsupported_hard_tokens_fail_before_reservation_and_preflight_does_not_spend() {
    let mut db = database();
    set(&mut db, |v| v.hard_token_limit = Some(1_000));
    assert_eq!(
        preflight(&mut db, &context("nova"), true, None, 20_746),
        Err(Denial::TokenUnsupported)
    );
    assert!(matches!(
        reserve(&mut db, &context("nova"), Cost::dialog(0, 0, None), 20_746),
        Err(Denial::TokenUnsupported)
    ));
    assert_eq!(view(&mut db).total, Totals::default());
    preflight(&mut db, &context("nova"), true, Some(500), 20_746).unwrap();
    assert_eq!(view(&mut db).total, Totals::default());
    let mut manual = context("nova");
    manual.origin = Origin::Manual;
    preflight(&mut db, &manual, true, None, 20_746).unwrap();
}

#[test]
fn utc_rollover_settles_original_day_and_backwards_clock_cannot_reset_current_day() {
    let mut db = database();
    set(&mut db, |v| v.daily_dialog_requests = 1);
    let yesterday = reserve(
        &mut db,
        &context("nova"),
        Cost::dialog(0, 0, Some(500)),
        20_746,
    )
    .unwrap();
    assert_eq!(snapshot(&mut db, 20_747).unwrap().total.dialog_requests, 0);
    settle(&mut db, &yesterday, Some(100)).unwrap();
    assert_eq!(snapshot(&mut db, 20_747).unwrap().total.dialog_requests, 0);
    reserve(&mut db, &context("james"), Cost::dialog(0, 0, None), 20_747).unwrap();
    let back = snapshot(&mut db, 1).unwrap();
    assert!(back.clock_adjusted);
    assert_eq!(back.day, "2026-10-21");
    assert_eq!(back.total.dialog_requests, 1);
    assert!(matches!(
        reserve(&mut db, &context("nova"), Cost::dialog(0, 0, None), 2),
        Err(Denial::DialogLimit)
    ));
    assert_eq!(
        snapshot(&mut db, 20_747 + 100_000)
            .unwrap()
            .total
            .dialog_requests,
        0
    );
}

#[test]
fn ancillary_limits_are_separate_global_and_count_each_actual_attempt() {
    let mut db = database();
    set(&mut db, |v| {
        v.daily_tts_characters = 3;
        v.daily_hindsight_requests = 2;
        v.daily_image_count = 2;
        v.daily_image_bytes = 30;
    });
    let tts = reserve(
        &mut db,
        &context("nova"),
        Cost::tts("ä🪐x".chars().count() as u64),
        20_746,
    )
    .unwrap();
    settle(&mut db, &tts, None).unwrap();
    assert!(matches!(
        reserve(&mut db, &context("james"), Cost::tts(1), 20_746),
        Err(Denial::TtsLimit)
    ));
    for retain in [false, true] {
        let ticket = reserve(
            &mut db,
            &context(if retain { "james" } else { "nova" }),
            Cost::memory(retain),
            20_746,
        )
        .unwrap();
        settle(&mut db, &ticket, None).unwrap();
    }
    assert!(matches!(
        reserve(&mut db, &context("nova"), Cost::memory(false), 20_746),
        Err(Denial::HindsightLimit)
    ));
    reserve(&mut db, &context("nova"), Cost::dialog(1, 20, None), 20_746).unwrap();
    assert!(matches!(
        reserve(
            &mut db,
            &context("james"),
            Cost::dialog(1, 11, None),
            20_746
        ),
        Err(Denial::ImageBytesLimit)
    ));
    reserve(
        &mut db,
        &context("james"),
        Cost::dialog(1, 10, None),
        20_746,
    )
    .unwrap();
    assert!(matches!(
        reserve(&mut db, &context("nova"), Cost::dialog(1, 1, None), 20_746),
        Err(Denial::ImageCountLimit)
    ));
    let total = view(&mut db).total;
    assert_eq!(total.tts_requests, 1);
    assert_eq!(total.tts_characters, 3);
    assert_eq!(total.hindsight_recall_requests, 1);
    assert_eq!(total.hindsight_retain_requests, 1);
    assert_eq!(total.image_bytes, 30);
}

#[test]
fn write_failure_rolls_back_charge_and_never_exposes_database_payload() {
    let mut db = database();
    db.execute_batch("CREATE TRIGGER fail_budget BEFORE INSERT ON usage_budget_reservations BEGIN SELECT RAISE(ABORT,'synthetic-private-marker'); END;").unwrap();
    let error = reserve(&mut db, &context("nova"), Cost::dialog(0, 0, None), 20_746).unwrap_err();
    assert_eq!(error, Denial::AccountingUnavailable);
    assert!(!error.message().contains("private-marker"));
    assert_eq!(view(&mut db).total, Totals::default());
}

#[test]
fn schema_settings_and_corrupt_rows_fail_closed_instead_of_resetting_usage() {
    let mut db = database();
    reserve(&mut db, &context("nova"), Cost::dialog(0, 0, None), 20_746).unwrap();
    db.execute(
        "UPDATE usage_budget_settings SET settings_json='invalid-private-payload'",
        [],
    )
    .unwrap();
    assert!(snapshot(&mut db, 20_746).is_err());
    configure(&mut db, &Limits::default(), 20_746).unwrap();
    assert_eq!(view(&mut db).total.dialog_requests, 1);
    db.execute("UPDATE usage_budget_meta SET version=2", [])
        .unwrap();
    assert!(initialize(&mut db, 20_746).is_err());
    assert!(reserve(&mut db, &context("nova"), Cost::dialog(0, 0, None), 20_746).is_err());
    db.execute("UPDATE usage_budget_meta SET version=1", [])
        .unwrap();
    db.execute("UPDATE usage_budget_reservations SET token_charge=-1", [])
        .unwrap();
    assert!(snapshot(&mut db, 20_746).is_err());
}

#[test]
fn provider_exceeding_a_verified_upper_is_recorded_and_blocks_further_autonomy() {
    let mut db = database();
    let ticket = reserve(
        &mut db,
        &context("nova"),
        Cost::dialog(0, 0, Some(100)),
        20_746,
    )
    .unwrap();
    assert_eq!(
        settle(&mut db, &ticket, Some(101)),
        Err(Denial::AccountingUnavailable)
    );
    assert_eq!(view(&mut db).total.observed_tokens, 101);
    assert!(matches!(
        reserve(&mut db, &context("james"), Cost::dialog(0, 0, None), 20_746),
        Err(Denial::AccountingUnavailable)
    ));
    configure(&mut db, &Limits::default(), 20_746).unwrap();
    assert!(preflight(&mut db, &context("nova"), true, Some(1), 20_746).is_err());
}

#[test]
fn unrelated_bots_invalid_costs_and_noninteger_settings_cannot_allocate_budget_rows() {
    let mut db = database();
    assert!(reserve(
        &mut db,
        &context("missing"),
        Cost::dialog(0, 0, None),
        20_746
    )
    .is_err());
    assert!(reserve(
        &mut db,
        &context("../nova"),
        Cost::dialog(0, 0, None),
        20_746
    )
    .is_err());
    assert!(reserve(&mut db, &context("nova"), Cost::dialog(5, 1, None), 20_746).is_err());
    assert!(reserve(
        &mut db,
        &context("nova"),
        Cost::dialog(0, 0, Some(u64::MAX)),
        20_746
    )
    .is_err());
    let mut json = serde_json::to_value(Limits::default()).unwrap();
    json["dailyDialogRequests"] = serde_json::json!(1.5);
    assert!(serde_json::from_value::<Limits>(json).is_err());
    assert_eq!(view(&mut db).total, Totals::default());
}

#[test]
fn strict_request_mode_rejects_cli_before_reserving_and_mode_changes_never_reset_history() {
    let mut db = database();
    let cli = reserve(&mut db, &context("nova"), Cost::cli_dialog(1, 10), 20_746).unwrap();
    settle(&mut db, &cli, None).unwrap();
    set(&mut db, |v| {
        v.request_accounting = RequestAccounting::Direct
    });
    assert_eq!(
        preflight(&mut db, &context("james"), false, None, 20_746),
        Err(Denial::RequestAccountingUnsupported)
    );
    assert!(matches!(
        reserve(&mut db, &context("james"), Cost::cli_dialog(0, 0), 20_746),
        Err(Denial::RequestAccountingUnsupported)
    ));
    assert_eq!(
        preflight(&mut db, &context("james"), true, Some(100), 20_746),
        Err(Denial::RequestHistoryUnbounded)
    );
    assert!(matches!(
        reserve(
            &mut db,
            &context("james"),
            Cost::dialog(0, 0, Some(100)),
            20_746
        ),
        Err(Denial::RequestHistoryUnbounded)
    ));
    assert_eq!(view(&mut db).total.dialog_requests, 1);
    initialize(&mut db, 20_746).unwrap();
    assert_eq!(
        preflight(&mut db, &context("nova"), true, None, 20_746),
        Err(Denial::RequestHistoryUnbounded)
    );
    set(&mut db, |_| {});
    assert_eq!(view(&mut db).total.dialog_requests, 1);
    set(&mut db, |v| {
        v.request_accounting = RequestAccounting::Direct
    });
    assert_eq!(
        preflight(&mut db, &context("james"), true, None, 20_746),
        Err(Denial::RequestHistoryUnbounded)
    );
    // A new UTC day, unlike settings edits or restart, starts a fresh bucket.
    preflight(&mut db, &context("james"), true, None, 20_747).unwrap();
    reserve(&mut db, &context("james"), Cost::dialog(0, 0, None), 20_747).unwrap();
    assert_eq!(snapshot(&mut db, 20_747).unwrap().total.dialog_requests, 1);
    assert!(matches!(
        reserve(&mut db, &context("james"), Cost::cli_dialog(0, 0), 20_747),
        Err(Denial::RequestAccountingUnsupported)
    ));
}

#[test]
fn enabling_hard_tokens_after_unbounded_history_blocks_until_next_day_even_with_a_receipt() {
    for observed in [None, Some(50)] {
        let mut db = database();
        let unbounded =
            reserve(&mut db, &context("nova"), Cost::dialog(0, 0, None), 20_746).unwrap();
        settle(&mut db, &unbounded, observed).unwrap();
        set(&mut db, |v| v.hard_token_limit = Some(1_000));
        assert_eq!(
            preflight(&mut db, &context("james"), true, Some(100), 20_746),
            Err(Denial::TokenHistoryUnbounded)
        );
        assert!(matches!(
            reserve(
                &mut db,
                &context("james"),
                Cost::dialog(0, 0, Some(100)),
                20_746
            ),
            Err(Denial::TokenHistoryUnbounded)
        ));
        initialize(&mut db, 20_746).unwrap();
        set(&mut db, |_| {});
        set(&mut db, |v| v.hard_token_limit = Some(1_000));
        let day = view(&mut db);
        assert_eq!(day.total.dialog_requests, 1);
        assert_eq!(day.total.observed_tokens, observed.unwrap_or(0));
        assert_eq!(
            preflight(&mut db, &context("james"), true, Some(100), 20_746),
            Err(Denial::TokenHistoryUnbounded)
        );
        preflight(&mut db, &context("james"), true, Some(100), 20_747).unwrap();
        reserve(
            &mut db,
            &context("james"),
            Cost::dialog(0, 0, Some(100)),
            20_747,
        )
        .unwrap();
        assert!(matches!(
            reserve(&mut db, &context("james"), Cost::cli_dialog(0, 0), 20_747),
            Err(Denial::TokenUnsupported)
        ));
    }
}

#[test]
fn strict_request_mode_accepts_known_host_sends_without_fabricating_token_guarantees() {
    let mut db = database();
    set(&mut db, |v| {
        v.daily_dialog_requests = 2;
        v.request_accounting = RequestAccounting::Direct;
    });
    // Direct compatible HTTP is host-metered even when its model has no hard bound.
    let ticket = reserve(&mut db, &context("nova"), Cost::dialog(0, 0, None), 20_746).unwrap();
    settle(&mut db, &ticket, None).unwrap();
    preflight(&mut db, &context("james"), true, None, 20_746).unwrap();
    reserve(&mut db, &context("james"), Cost::dialog(0, 0, None), 20_746).unwrap();
    assert!(matches!(
        reserve(&mut db, &context("nova"), Cost::dialog(0, 0, None), 20_746),
        Err(Denial::DialogLimit)
    ));
    let legacy = serde_json::to_value(Limits::default()).unwrap();
    let mut absent = legacy.clone();
    absent.as_object_mut().unwrap().remove("requestAccounting");
    assert_eq!(
        serde_json::from_value::<Limits>(absent)
            .unwrap()
            .request_accounting,
        RequestAccounting::Dispatch
    );
    let mut invalid = legacy;
    invalid["requestAccounting"] = serde_json::json!("unmetered");
    assert!(serde_json::from_value::<Limits>(invalid).is_err());
}

#[test]
fn malformed_or_future_ledger_days_cannot_hide_a_charge_or_be_pruned_on_restart() {
    for day_sql in [
        "-1",
        "'not-an-integer'",
        "20746.5",
        "20747",
        "9999999999999",
        "X'01'",
    ] {
        let mut db = database();
        let ticket = reserve(
            &mut db,
            &context("nova"),
            Cost::dialog(0, 0, Some(100)),
            20_746,
        )
        .unwrap();
        db.execute_batch(&format!(
            "UPDATE usage_budget_reservations SET day={day_sql}"
        ))
        .unwrap();
        assert_eq!(
            initialize(&mut db, 20_747),
            Err(Denial::AccountingUnavailable),
            "{day_sql}"
        );
        assert!(snapshot(&mut db, 20_747).is_err(), "{day_sql}");
        assert!(
            reserve(
                &mut db,
                &context("james"),
                Cost::dialog(0, 0, Some(100)),
                20_747
            )
            .is_err(),
            "{day_sql}"
        );
        assert!(settle(&mut db, &ticket, Some(1)).is_err(), "{day_sql}");
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM usage_budget_reservations", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap(),
            1
        );
        assert_eq!(
            db.query_row("SELECT state FROM usage_budget_reservations", [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap(),
            "pending"
        );
        assert_eq!(
            db.query_row("SELECT last_day FROM usage_budget_meta", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            20_746
        );
    }
}

#[test]
fn ancillary_preflight_is_nonspending_but_rechecks_new_limits_before_admission() {
    let mut db = database();
    set(&mut db, |v| {
        v.daily_tts_characters = 0;
        v.daily_hindsight_requests = 0;
    });
    assert_eq!(
        preflight_cost(&mut db, &context("nova"), Cost::tts(1), 20_746),
        Err(Denial::TtsLimit)
    );
    assert_eq!(
        preflight_cost(&mut db, &context("nova"), Cost::memory(true), 20_746),
        Err(Denial::HindsightLimit)
    );
    assert_eq!(view(&mut db).total, Totals::default());
    set(&mut db, |v| v.daily_tts_characters = 1);
    preflight_cost(&mut db, &context("nova"), Cost::tts(1), 20_746).unwrap();
    set(&mut db, |v| v.daily_tts_characters = 0);
    assert!(matches!(
        reserve(&mut db, &context("nova"), Cost::tts(1), 20_746),
        Err(Denial::TtsLimit)
    ));
}

#[test]
fn lowering_image_caps_keeps_existing_spend_but_does_not_block_unrelated_components() {
    let mut db = database();
    let ticket = reserve(
        &mut db,
        &context("nova"),
        Cost::dialog(1, 100, None),
        20_746,
    )
    .unwrap();
    settle(&mut db, &ticket, None).unwrap();
    set(&mut db, |v| {
        v.daily_image_count = 0;
        v.daily_image_bytes = 0;
    });
    reserve(&mut db, &context("nova"), Cost::tts(1), 20_746).unwrap();
    reserve(&mut db, &context("james"), Cost::memory(false), 20_746).unwrap();
    reserve(&mut db, &context("james"), Cost::dialog(0, 0, None), 20_746).unwrap();
    assert!(matches!(
        reserve(&mut db, &context("nova"), Cost::dialog(1, 1, None), 20_746),
        Err(Denial::ImageCountLimit)
    ));
    assert_eq!(view(&mut db).total.image_count, 1);
    assert_eq!(view(&mut db).total.image_bytes, 100);
}

#[test]
fn oversized_or_nontext_denial_metadata_is_not_silently_projected_as_absent() {
    for value in [
        serde_json::Value::String("x".repeat(100)),
        serde_json::json!(""),
    ] {
        let mut db = database();
        db.execute(
            "UPDATE usage_budget_meta SET last_denial=?1,last_denial_day=NULL",
            [value.as_str().unwrap()],
        )
        .unwrap();
        assert_eq!(
            initialize(&mut db, 20_746),
            Err(Denial::AccountingUnavailable)
        );
        assert!(snapshot(&mut db, 20_746).is_err());
        assert!(reserve(&mut db, &context("nova"), Cost::dialog(0, 0, None), 20_746).is_err());
    }
    let mut db = database();
    db.execute(
        "UPDATE usage_budget_meta SET last_denial=?1,last_denial_day=NULL",
        [vec![b'x'; 100]],
    )
    .unwrap();
    assert!(snapshot(&mut db, 20_746).is_err());
}
