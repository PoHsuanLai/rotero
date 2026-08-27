//! Properties the sync engine must hold under any schedule.
//!
//! The hand-written suite next door checks convergence on orderings someone
//! thought of. That is worth having — a named test states an invariant in ten
//! seconds — but it only ever covers the schedules already imagined, and the
//! most recent one to be added claimed "any order" while testing two of seven
//! hundred and twenty. These tests generate the schedule instead.
//!
//! The properties are split into two groups by whether they depend on timing.
//! Local writes stamp `updated_at` from the wall clock, so two operations in the
//! same millisecond tie and the tie is broken by device id; a property that
//! depends on which of two concurrent edits won can therefore give a different
//! answer on a fast machine than a slow one. The properties that do not depend
//! on that — a deletion staying deleted, an operation not erroring — are the
//! load-bearing ones here, and they are the ones that catch the bugs this file
//! was written for.

mod common;

use common::sync_harness::{block_on, canonical, first_difference, quiesce, run};
use common::sync_model::{Scenario, as_rust_literal, budget, scenario_strategy};
use proptest::prelude::*;

/// Build the proptest configuration from the active budget.
fn config() -> ProptestConfig {
    ProptestConfig {
        cases: budget().cases,
        // The default is `cases * 4`, which at 48 cases is too few passes to
        // reduce a fourteen-event scenario to the two or three events that
        // actually matter. Shrinking only runs on failure, so a generous
        // ceiling costs nothing on a green run.
        max_shrink_iters: 2048,
        // A generated scenario that somehow becomes pathological should fail
        // loudly rather than hold a CI job open.
        timeout: 30_000,
        ..ProptestConfig::default()
    }
}

/// Run a scenario, returning a description of the first property it violated.
fn check(scenario: &Scenario) -> Result<(), String> {
    let shared = tempfile::tempdir().map_err(|e| e.to_string())?;
    let outcome = block_on(run(scenario, shared.path(), budget().max_papers));

    // P6 — no operation errors. Nearly free, and it is how a key predicate
    // built with the wrong number of placeholders surfaces at all: the data can
    // still be correct because another statement already covered it, leaving
    // the broken call silently failing.
    if let Some(first) = outcome.errors.first() {
        return Err(format!("an operation failed: {first}"));
    }

    let states = block_on(async {
        let mut v = Vec::new();
        for d in &outcome.devices {
            v.push(canonical(&d.db).await);
        }
        v
    });

    // P5 — a deletion stays deleted.
    //
    // The expected set comes from the op log, not from reading tombstones back
    // out of a database. When a delete writes no tombstone at all the database
    // has nothing to report, so a database-derived expectation would be empty
    // and this property would pass by having nothing to check — which is
    // exactly the shape of the bug it exists to catch.
    for (i, state) in states.iter().enumerate() {
        for intent in &outcome.tombstone_intents {
            let key = (intent.0, intent.1.clone());
            match state.get(&key) {
                // Gone entirely is fine on a device that never learned of the
                // row: there is nothing to resurrect.
                None => continue,
                Some(row) if row.deleted => continue,
                Some(row) => {
                    return Err(format!(
                        "device-{i}: {}{:?} was deleted but is live again \
                         (updated_at {}, updated_by {})",
                        intent.0, intent.1, row.updated_at, row.updated_by
                    ));
                }
            }
        }
    }

    // P8 — `tags.name` is UNIQUE across live and dead rows alike, so two live
    // tags sharing a name is a state the schema itself forbids.
    for (i, state) in states.iter().enumerate() {
        let mut seen = std::collections::BTreeMap::new();
        for ((table, key), row) in state {
            if *table != "tags" || row.deleted {
                continue;
            }
            let Some(name) = row.values.as_ref().and_then(|v| v.get("name")) else {
                continue;
            };
            if let Some(prev) = seen.insert(name.clone(), key.clone()) {
                return Err(format!(
                    "device-{i}: tags {prev:?} and {key:?} both live with name {name:?}"
                ));
            }
        }
    }

    Ok(())
}

proptest! {
    #![proptest_config(config())]

    /// A deletion stays deleted, no operation errors, and no two live tags
    /// share a name — whatever the schedule.
    #[test]
    fn deletions_survive_any_schedule(
        scenario in scenario_strategy(budget().devices, budget().events)
    ) {
        if let Err(why) = check(&scenario) {
            // Print the scenario as source so it can be lifted straight out of
            // proptest and into a named test.
            prop_assert!(
                false,
                "{why}\n\nscenario:\n{}\n",
                as_rust_literal(&scenario)
            );
        }
    }
}

/// Every device ends up holding the same rows, and one more exchange after that
/// changes nothing.
///
/// Separated from the properties above because both halves depend on timing.
/// Convergence can hinge on which of two same-millisecond writes won, and the
/// fixed-point check can hinge on a merge that stamps rows with the local clock.
/// Keeping them apart means a flake here does not obscure the deletion
/// properties, which do not have that exposure.
fn check_convergence(scenario: &Scenario) -> Result<(), String> {
    let shared = tempfile::tempdir().map_err(|e| e.to_string())?;
    let outcome = block_on(run(scenario, shared.path(), budget().max_papers));

    let before = block_on(async {
        let mut v = Vec::new();
        for d in &outcome.devices {
            v.push(canonical(&d.db).await);
        }
        v
    });

    // P1 — every device agrees, tombstones and clocks included. Two devices
    // showing the same library but disagreeing about when a row was last
    // written will diverge on the next edit, so comparing only what the user
    // can see would report success one step before the failure.
    for i in 1..before.len() {
        if let Some(diff) = first_difference(&before[0], &before[i]) {
            return Err(format!("device-0 and device-{i} disagree: {diff}"));
        }
    }

    // P2 — quiescence is a fixed point. One more full exchange must change
    // nothing. A merge that writes rows stamped with the merging device's own
    // clock would keep producing new state on every round, so a system that
    // converges but never settles fails here and nowhere else.
    let settling = block_on(quiesce(&outcome.devices));
    if let Some(first) = settling.first() {
        return Err(format!("a further exchange failed: {first}"));
    }
    let after = block_on(async {
        let mut v = Vec::new();
        for d in &outcome.devices {
            v.push(canonical(&d.db).await);
        }
        v
    });

    for (i, (b, a)) in before.iter().zip(after.iter()).enumerate() {
        if let Some(diff) = first_difference(b, a) {
            return Err(format!(
                "device-{i} kept changing after it had settled: {diff}"
            ));
        }
    }

    Ok(())
}

proptest! {
    #![proptest_config(config())]

    /// Every device converges, and stays converged.
    #[test]
    fn devices_converge_and_settle(
        scenario in scenario_strategy(budget().devices, budget().events)
    ) {
        if let Err(why) = check_convergence(&scenario) {
            prop_assert!(
                false,
                "{why}\n\nscenario:\n{}\n",
                as_rust_literal(&scenario)
            );
        }
    }
}

/// A strategy for one snapshot row, with text that is not merely ASCII.
///
/// This is where arbitrary Unicode belongs. The scenario generator deliberately
/// draws from a tiny pool of names, because there the point is to force
/// collisions; here the point is the opposite — that a title with a newline, a
/// NUL, or an astral-plane character survives being gzipped, written as one
/// line of JSON, and read back.
fn snapshot_row_strategy() -> impl Strategy<Value = rotero_db::snapshot::SnapshotRow> {
    use proptest::collection::{btree_map, vec};
    (
        proptest::sample::select(vec!["papers", "tags", "collections", "notes"]),
        vec(any::<String>(), 1..=2),
        btree_map(
            any::<String>(),
            any::<String>().prop_map(serde_json::Value::String),
            0..4,
        ),
        any::<i64>(),
        any::<String>(),
        any::<bool>(),
    )
        .prop_map(|(t, k, v, ua, ub, d)| rotero_db::snapshot::SnapshotRow {
            t: t.to_string(),
            k,
            // A tombstone carries no payload, so generating one with values
            // would be generating a row the writer never produces.
            v: (!d).then_some(v),
            ua,
            ub,
            d,
        })
}

proptest! {
    // Pure and in-memory: no database, no files, so it can afford far more
    // cases than the scenario properties.
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    /// A snapshot survives being written and read back, whatever is in it.
    #[test]
    fn a_snapshot_round_trips(rows in proptest::collection::vec(snapshot_row_strategy(), 0..12)) {
        let bytes = common::sync_harness::encode_snapshot("device-0", &rows);
        let (header, parsed) = rotero_db::snapshot::parse_snapshot(&bytes)
            .map_err(|e| TestCaseError::fail(format!("failed to parse: {e}")))?;

        prop_assert_eq!(header.rows, rows.len());
        prop_assert_eq!(parsed.len(), rows.len());
        for (before, after) in rows.iter().zip(parsed.iter()) {
            prop_assert_eq!(&before.t, &after.t);
            prop_assert_eq!(&before.k, &after.k);
            prop_assert_eq!(&before.v, &after.v);
            prop_assert_eq!(before.ua, after.ua);
            prop_assert_eq!(&before.ub, &after.ub);
            prop_assert_eq!(before.d, after.d);
        }
    }

    /// A snapshot cut short is refused rather than half-applied.
    ///
    /// Half a peer's library applied is worse than none of it, because nothing
    /// downstream can tell the difference — the rows that did land look like
    /// the whole truth.
    #[test]
    fn a_truncated_snapshot_is_refused(
        rows in proptest::collection::vec(snapshot_row_strategy(), 1..8),
        cut in 0.0f64..1.0,
    ) {
        let bytes = common::sync_harness::encode_snapshot("device-0", &rows);
        let keep = ((bytes.len() as f64) * cut) as usize;
        prop_assume!(keep < bytes.len());

        prop_assert!(
            rotero_db::snapshot::parse_snapshot(&bytes[..keep]).is_err(),
            "a snapshot cut to {keep} of {} bytes parsed anyway",
            bytes.len()
        );
    }
}
