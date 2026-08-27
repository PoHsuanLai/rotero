//! The generated shape of a multi-device sync scenario.
//!
//! A scenario is a flat, totally-ordered list of events: local edits on some
//! device, and the two halves of a sync — publishing a snapshot and merging the
//! peers'. Keeping the list flat is what lets proptest shrink it, since it can
//! drop any single element and still have a coherent program; per-device
//! sequences plus a separate interleaving are two structures that shrink
//! independently into states that never happened.
//!
//! Splitting `Export` from `Import` is the point of the whole model. A single
//! combined "sync" event can only ever produce the fully-connected case where
//! everyone sees everything at once, which is what the hand-written tests
//! already cover. Split, the generator reaches stale reads, a device that never
//! imports, and rows that arrive at a third device by way of a second.

use proptest::prelude::*;

/// An abstract reference to an entity, resolved when the op runs.
///
/// Ids are UUIDs minted inside `insert_*`, so a generator cannot name one. It
/// names a slot instead, taken modulo however many exist on that device at that
/// moment. Resolving per-device is deliberate: the same op sequence names
/// different rows on different devices, which is how a generated schedule
/// produces the same-name-different-id tag collision without being asked to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sym(pub usize);

/// One local mutation.
///
/// Payloads are small integers rather than strings so that collisions are the
/// common case: two devices writing the same title is what forces a
/// last-writer-wins decision, and a six-name tag pool is what exercises the
/// `tags.name` UNIQUE path. They also shrink to something readable — `title: 0`
/// rather than an arbitrary Unicode escape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    InsertPaper { title: u8 },
    InsertCollection { name: u8 },
    GetOrCreateTag { name: u8 },
    InsertNote { paper: Sym, body: u8 },
    InsertAnnotation { paper: Sym, page: u8 },

    RetitlePaper { paper: Sym, title: u8 },
    SetFavorite { paper: Sym, on: bool },
    RenameTag { tag: Sym, name: u8 },

    AddTagToPaper { paper: Sym, tag: Sym },
    RemoveTagFromPaper { paper: Sym, tag: Sym },
    AddPaperToCollection { paper: Sym, coll: Sym },
    RemovePaperFromCollection { paper: Sym, coll: Sym },
    InsertCitation { citing: Sym, cited: Sym },

    DeletePaper { paper: Sym },
    DeleteCollection { coll: Sym },
    DeleteTag { tag: Sym },
}

/// One step of a scenario.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// A device edits its own library.
    Local { device: u8, op: Op },
    /// A device publishes its snapshot to the shared folder.
    Export { device: u8 },
    /// A device merges every peer snapshot currently in the folder.
    Import { device: u8 },
}

/// A complete generated scenario.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scenario {
    /// How many devices share the folder.
    pub devices: usize,
    /// What happens, in order.
    pub events: Vec<Event>,
}

/// Render a title from its generated index.
pub fn title_of(n: u8) -> String {
    format!("P{n}")
}

/// Render a tag or collection name from its generated index.
pub fn name_of(n: u8) -> String {
    format!("t{n}")
}

/// How many distinct titles and names the generator draws from.
///
/// Deliberately tiny. A large pool would make every write touch a different
/// row, and a merge that never has to choose a winner proves nothing about the
/// rule it uses to choose one.
const POOL: u8 = 6;

/// The strategy for one local operation.
///
/// Weighted rather than uniform. Creates have to outnumber edits or the
/// registry stays empty and most ops resolve to nothing; junction and delete
/// ops are weighted up because that is the thinnest part of the engine and
/// where the confirmed bugs live.
fn op_strategy() -> impl Strategy<Value = Op> {
    // A fresh strategy per use: proptest strategies are not `Copy`, and the
    // range is deliberately small so an index shrinks toward `Sym(0)`.
    fn sym() -> impl Strategy<Value = Sym> {
        (0usize..8).prop_map(Sym)
    }

    prop_oneof![
        3 => (0u8..POOL).prop_map(|title| Op::InsertPaper { title }),
        2 => (0u8..4).prop_map(|name| Op::InsertCollection { name }),
        2 => (0u8..POOL).prop_map(|name| Op::GetOrCreateTag { name }),
        1 => (sym(), 0u8..4).prop_map(|(paper, body)| Op::InsertNote { paper, body }),
        1 => (sym(), 0u8..3).prop_map(|(paper, page)| Op::InsertAnnotation { paper, page }),

        3 => (sym(), 0u8..POOL).prop_map(|(paper, title)| Op::RetitlePaper { paper, title }),
        1 => (sym(), any::<bool>()).prop_map(|(paper, on)| Op::SetFavorite { paper, on }),
        2 => (sym(), 0u8..POOL).prop_map(|(tag, name)| Op::RenameTag { tag, name }),

        4 => (sym(), sym()).prop_map(|(paper, tag)| Op::AddTagToPaper { paper, tag }),
        4 => (sym(), sym()).prop_map(|(paper, tag)| Op::RemoveTagFromPaper { paper, tag }),
        3 => (sym(), sym()).prop_map(|(paper, coll)| Op::AddPaperToCollection { paper, coll }),
        3 => (sym(), sym()).prop_map(|(paper, coll)| Op::RemovePaperFromCollection { paper, coll }),
        2 => (sym(), sym()).prop_map(|(citing, cited)| Op::InsertCitation { citing, cited }),

        2 => sym().prop_map(|paper| Op::DeletePaper { paper }),
        1 => sym().prop_map(|coll| Op::DeleteCollection { coll }),
        2 => sym().prop_map(|tag| Op::DeleteTag { tag }),
    ]
}

/// The strategy for a whole scenario.
///
/// Local edits dominate and sync events are punctuation: too many exchanges and
/// every case is trivially converged, and each one costs a full snapshot round
/// trip, which is what the runtime budget is spent on.
pub fn scenario_strategy(
    devices: std::ops::RangeInclusive<usize>,
    events: std::ops::RangeInclusive<usize>,
) -> impl Strategy<Value = Scenario> {
    devices.prop_flat_map(move |n| {
        let dev = 0u8..(n as u8);
        let event = prop_oneof![
            6 => (dev.clone(), op_strategy()).prop_map(|(device, op)| Event::Local { device, op }),
            2 => dev.clone().prop_map(|device| Event::Export { device }),
            2 => dev.prop_map(|device| Event::Import { device }),
        ];
        proptest::collection::vec(event, events.clone())
            .prop_map(move |events| Scenario { devices: n, events })
    })
}

/// Scenario size, scaled by `ROTERO_PROPTEST`.
///
/// The same test runs in both tiers rather than the deep one being `#[ignore]`d.
/// An ignored test still compiles but never runs, so it rots unnoticed and needs
/// its own invocation to exercise; running the cheap tier on every push is what
/// keeps this harness honest.
pub struct Budget {
    /// How many scenarios to generate.
    pub cases: u32,
    /// How many devices share the folder.
    pub devices: std::ops::RangeInclusive<usize>,
    /// How many events one scenario holds.
    pub events: std::ops::RangeInclusive<usize>,
    /// The ceiling on papers, which is what actually bounds the runtime.
    pub max_papers: usize,
}

/// The active budget.
pub fn budget() -> Budget {
    match std::env::var("ROTERO_PROPTEST").as_deref() {
        Ok("heavy") => Budget {
            cases: 512,
            devices: 2..=4,
            events: 10..=40,
            max_papers: 24,
        },
        _ => Budget {
            cases: 48,
            devices: 2..=3,
            events: 4..=14,
            max_papers: 12,
        },
    }
}

/// Render a scenario as Rust source.
///
/// A shrunk counterexample is only useful if it can leave proptest behind:
/// debugging inside the harness means re-deriving the failing state on every
/// run, where a literal can be pasted into a plain `#[tokio::test]` and stepped
/// through. Every counterexample this suite finds is meant to graduate into
/// `sync_robustness_test.rs` as a named test, and this is what makes that a
/// copy-paste rather than a transcription.
pub fn as_rust_literal(s: &Scenario) -> String {
    let mut out = format!(
        "Scenario {{\n    devices: {},\n    events: vec![\n",
        s.devices
    );
    for e in &s.events {
        let line = match e {
            Event::Local { device, op } => {
                format!("        Event::Local {{ device: {device}, op: Op::{op:?} }},")
            }
            Event::Export { device } => format!("        Event::Export {{ device: {device} }},"),
            Event::Import { device } => format!("        Event::Import {{ device: {device} }},"),
        };
        out.push_str(&line);
        out.push('\n');
    }
    out.push_str("    ],\n}");
    out
}
