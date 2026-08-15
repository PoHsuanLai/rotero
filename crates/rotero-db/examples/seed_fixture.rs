//! Seeds a throwaway library used to capture the documentation screenshots.
//!
//! Run via the `docs-screenshots` recipe, or directly:
//!
//! ```sh
//! cargo run -p rotero-db --example seed_fixture -- /tmp/rotero-fixture
//! ```
//!
//! The papers are invented. Only "Trace-based Just-in-Time Type Specialization"
//! is real, because it is the paper behind the PDF fixture the reader
//! screenshots open (`crates/rotero-pdf/tests/fixtures/pdfs/tracemonkey.pdf`).
//! Everything else is written to look plausible without attributing work to
//! anyone who did not do it.

use std::path::{Path, PathBuf};

use chrono::{Duration, Utc};
use rotero_db::Database;
use rotero_models::{Annotation, AnnotationType, Collection, Creator, Note, Paper};

/// A paper to seed, in the order the fixtures are declared.
struct Seed {
    title: &'static str,
    authors: &'static [&'static str],
    year: i32,
    journal: &'static str,
    doi: &'static str,
    item_type: &'static str,
    abstract_text: &'static str,
    /// Days before now, so "Recently Added" has a stable ordering.
    age_days: i64,
    is_favorite: bool,
    is_read: bool,
    citation_count: i64,
    /// Collection path, e.g. `["Methods", "Optimization"]`.
    collection: &'static [&'static str],
    tags: &'static [&'static str],
    /// Set for the one paper that gets the real PDF attached.
    pdf: bool,
}

const TAG_COLORS: &[(&str, &str)] = &[
    ("to-read", "#e8a33d"),
    ("foundational", "#d9534f"),
    ("methods", "#5cb85c"),
    ("benchmarks", "#4a90d9"),
    ("survey", "#9b59b6"),
    ("reproducible", "#4dbdb0"),
];

const SEEDS: &[Seed] = &[
    Seed {
        title: "Trace-based Just-in-Time Type Specialization for Dynamic Languages",
        authors: &[
            "Andreas Gal",
            "Brendan Eich",
            "Mike Shaver",
            "David Anderson",
        ],
        year: 2009,
        journal: "PLDI",
        doi: "10.1145/1542476.1542528",
        item_type: "conferencePaper",
        abstract_text: "Dynamic languages such as JavaScript are more difficult to compile than statically typed ones. We present a trace-based compilation technique that records and compiles frequently executed loop traces, specializing them on the observed types.",
        age_days: 2,
        is_favorite: true,
        is_read: true,
        citation_count: 812,
        collection: &["Compilers"],
        tags: &["foundational", "methods"],
        pdf: true,
    },
    Seed {
        title: "Incremental Region Inference for Latency-Sensitive Runtimes",
        authors: &["Marta Feld", "Junichi Oyama"],
        year: 2023,
        journal: "Journal of Systems Research",
        doi: "10.5281/zenodo.7741002",
        item_type: "journalArticle",
        abstract_text: "Region inference typically runs as a whole-program pass, which is a poor fit for runtimes that must stay responsive. We derive an incremental formulation that recomputes only the regions affected by an edit.",
        age_days: 4,
        is_favorite: true,
        is_read: false,
        citation_count: 27,
        collection: &["Compilers", "Optimization"],
        tags: &["to-read", "methods"],
        pdf: false,
    },
    Seed {
        title: "A Survey of Deoptimization Strategies in Managed Runtimes",
        authors: &["Priya Raghavan", "Tomas Lindqvist", "Chen Wei"],
        year: 2022,
        journal: "ACM Computing Surveys",
        doi: "10.1145/3510000.3510114",
        item_type: "journalArticle",
        abstract_text: "We survey twenty years of deoptimization design, classifying approaches by when guards are checked, what state is reconstructed, and how much of the optimized frame survives the transition.",
        age_days: 7,
        is_favorite: false,
        is_read: true,
        citation_count: 143,
        collection: &["Compilers"],
        tags: &["survey", "foundational"],
        pdf: false,
    },
    Seed {
        title: "Measuring Tail Latency Without Coordinated Omission",
        authors: &["Ana Sørensen", "Devin Blackwood"],
        year: 2021,
        journal: "USENIX ATC",
        doi: "10.5555/3488000.3488042",
        item_type: "conferencePaper",
        abstract_text: "Load generators that pause when the system under test slows down systematically understate tail latency. We quantify the error and present a generator that maintains an open request model.",
        age_days: 11,
        is_favorite: false,
        is_read: false,
        citation_count: 96,
        collection: &["Measurement"],
        tags: &["benchmarks", "methods"],
        pdf: false,
    },
    Seed {
        title: "Reproducibility of Performance Claims in Systems Papers",
        authors: &["Hana Kowalski", "Ruth Adeyemi", "Pablo Restrepo"],
        year: 2024,
        journal: "SIGPLAN Notices",
        doi: "10.1145/3620000.3620088",
        item_type: "journalArticle",
        abstract_text: "We attempted to reproduce the headline speedup of sixty systems papers. Forty-one shipped artifacts; of those, twenty-eight rebuilt without intervention, and nineteen landed within ten percent of the published figure.",
        age_days: 14,
        is_favorite: true,
        is_read: false,
        citation_count: 58,
        collection: &["Measurement"],
        tags: &["reproducible", "benchmarks", "to-read"],
        pdf: false,
    },
    Seed {
        title: "Cache-Oblivious Layouts for Sparse Adjacency Structures",
        authors: &["Ingrid Halvorsen", "Yusuf Demir"],
        year: 2020,
        journal: "Journal of Experimental Algorithmics",
        doi: "10.1145/3400000.3400031",
        item_type: "journalArticle",
        abstract_text: "We describe a family of layouts for sparse graphs that require no tuning to the memory hierarchy, and show they stay within a constant factor of hand-tuned blocking across four architectures.",
        age_days: 19,
        is_favorite: false,
        is_read: true,
        citation_count: 204,
        collection: &["Data Structures"],
        tags: &["foundational"],
        pdf: false,
    },
    Seed {
        title: "Succinct Indexes for Versioned Document Stores",
        authors: &["Leon Fairbairn", "Mei-Ling Chou"],
        year: 2023,
        journal: "VLDB",
        doi: "10.14778/3590000.3590017",
        item_type: "conferencePaper",
        abstract_text: "Versioned stores keep every revision, so index size grows with history rather than with the live corpus. We give a succinct index whose space tracks the live corpus plus the edit distance between revisions.",
        age_days: 23,
        is_favorite: false,
        is_read: false,
        citation_count: 41,
        collection: &["Data Structures"],
        tags: &["to-read"],
        pdf: false,
    },
    Seed {
        title: "On the Statistical Power of Microbenchmark Suites",
        authors: &["Gregor Antic", "Sinead O'Rourke"],
        year: 2022,
        journal: "Empirical Software Engineering",
        doi: "10.1007/s10664-022-10142-5",
        item_type: "journalArticle",
        abstract_text: "Most microbenchmark suites run too few iterations to detect the effect sizes their authors claim. We compute achieved power for nine widely used suites and recommend minimum iteration counts.",
        age_days: 28,
        is_favorite: false,
        is_read: true,
        citation_count: 77,
        collection: &["Measurement"],
        tags: &["benchmarks", "reproducible"],
        pdf: false,
    },
    Seed {
        title: "Escape Analysis Without Whole-Program Assumptions",
        authors: &["Nils Brekke", "Aditi Varma"],
        year: 2021,
        journal: "OOPSLA",
        doi: "10.1145/3485000.3485063",
        item_type: "conferencePaper",
        abstract_text: "Separate compilation defeats classical escape analysis. We propose a modular summary that is sound under arbitrary linking and recovers most of the stack allocation opportunities of the whole-program version.",
        age_days: 33,
        is_favorite: false,
        is_read: false,
        citation_count: 112,
        collection: &["Compilers", "Optimization"],
        tags: &["methods"],
        pdf: false,
    },
    Seed {
        title: "Energy Accounting for Interpreted Workloads",
        authors: &["Camille Duforest", "Owen Mbeki"],
        year: 2024,
        journal: "IEEE Transactions on Computers",
        doi: "10.1109/TC.2024.3361120",
        item_type: "journalArticle",
        abstract_text: "We attribute package energy to individual interpreter operations using a regression over hardware counters, and validate the model against an external power meter across three workloads.",
        age_days: 38,
        is_favorite: false,
        is_read: false,
        citation_count: 19,
        collection: &["Measurement"],
        tags: &["to-read", "benchmarks"],
        pdf: false,
    },
    Seed {
        title: "Type Feedback Collection at Sub-Percent Overhead",
        authors: &["Bea Lindstrom", "Kwame Osei", "Tara Nomura"],
        year: 2023,
        journal: "CGO",
        doi: "10.1145/3579000.3579041",
        item_type: "conferencePaper",
        abstract_text: "Type feedback is usually collected by instrumenting every call site. We sample instead, and show that a one-in-64 sampling rate preserves enough of the type profile to reach the same peak performance.",
        age_days: 44,
        is_favorite: false,
        is_read: true,
        citation_count: 63,
        collection: &["Compilers", "Optimization"],
        tags: &["methods", "benchmarks"],
        pdf: false,
    },
    Seed {
        title: "Garbage Collection Pauses as a Scheduling Problem",
        authors: &["Emil Novak", "Fatima Zahra"],
        year: 2020,
        journal: "ISMM",
        doi: "10.1145/3381898.3397213",
        item_type: "conferencePaper",
        abstract_text: "We reframe collector scheduling as a real-time admission problem, which yields pause bounds that hold under adversarial allocation rather than only in expectation.",
        age_days: 51,
        is_favorite: false,
        is_read: false,
        citation_count: 158,
        collection: &["Runtime"],
        tags: &["foundational", "to-read"],
        pdf: false,
    },
    Seed {
        title: "A Calculus for Partial Evaluation of Effectful Programs",
        authors: &["Sofia Marchetti", "Henrik Dahl"],
        year: 2022,
        journal: "ICFP",
        doi: "10.1145/3547000.3547034",
        item_type: "conferencePaper",
        abstract_text: "Partial evaluation is well understood for pure languages. We extend the standard binding-time analysis to a calculus with algebraic effects and prove the residual program preserves effect ordering.",
        age_days: 58,
        is_favorite: false,
        is_read: true,
        citation_count: 88,
        collection: &["Theory"],
        tags: &["foundational"],
        pdf: false,
    },
    Seed {
        title: "Deterministic Replay for Concurrent Interpreters",
        authors: &["Ravi Chandrasekar", "Lotte Visser"],
        year: 2021,
        journal: "EuroSys",
        doi: "10.1145/3447786.3456251",
        item_type: "conferencePaper",
        abstract_text: "We record only the interleaving of lock acquisitions, which is enough to replay interpreter-level data races deterministically at under four percent overhead.",
        age_days: 66,
        is_favorite: false,
        is_read: false,
        citation_count: 71,
        collection: &["Runtime"],
        tags: &["methods", "reproducible"],
        pdf: false,
    },
    Seed {
        title: "Profile-Guided Layout for Instruction Cache Locality",
        authors: &["Tobias Reinhardt", "Ngozi Eze"],
        year: 2019,
        journal: "MICRO",
        doi: "10.1145/3352460.3358294",
        item_type: "conferencePaper",
        abstract_text: "Function reordering based on a call profile reduces instruction cache misses, but the gains are fragile under profile drift. We characterize the drift and propose a layout that degrades gracefully.",
        age_days: 74,
        is_favorite: false,
        is_read: true,
        citation_count: 231,
        collection: &["Compilers", "Optimization"],
        tags: &["foundational", "benchmarks"],
        pdf: false,
    },
    Seed {
        title: "Bounded Staleness in Replicated Reference Libraries",
        authors: &["Ida Bergqvist", "Samuel Achebe"],
        year: 2024,
        journal: "PaPoC",
        doi: "10.1145/3642000.3642011",
        item_type: "conferencePaper",
        abstract_text: "Conflict-free replicated data types give eventual convergence but say little about how stale a replica may be. We add a bounded staleness guarantee that survives arbitrary partition duration.",
        age_days: 83,
        is_favorite: true,
        is_read: false,
        citation_count: 12,
        collection: &["Distributed"],
        tags: &["to-read", "methods"],
        pdf: false,
    },
    Seed {
        title: "Merge Semantics for Structured Annotation Data",
        authors: &["Paulo Ferreira", "Anneke de Vries"],
        year: 2023,
        journal: "CSCW",
        doi: "10.1145/3579500.3579531",
        item_type: "conferencePaper",
        abstract_text: "When two devices annotate the same document offline, naive merge either duplicates highlights or drops them. We define a merge that is idempotent under repeated synchronization.",
        age_days: 91,
        is_favorite: false,
        is_read: false,
        citation_count: 34,
        collection: &["Distributed"],
        tags: &["methods"],
        pdf: false,
    },
    Seed {
        title: "Citation Graph Construction from Unstructured References",
        authors: &["Wei Zhang", "Marguerite Boucher", "Idris Salah"],
        year: 2022,
        journal: "JCDL",
        doi: "10.1145/3529372.3530917",
        item_type: "conferencePaper",
        abstract_text: "Reference strings vary enough across venues that string matching alone links only two thirds of citations. We combine a sequence labeler with identifier lookup to raise linkage to ninety-four percent.",
        age_days: 104,
        is_favorite: false,
        is_read: true,
        citation_count: 119,
        collection: &["Bibliometrics"],
        tags: &["methods", "survey"],
        pdf: false,
    },
    Seed {
        title: "What Makes a Reference Manager Fast?",
        authors: &["Elise Marchand"],
        year: 2024,
        journal: "Journal of Open Research Software",
        doi: "10.5334/jors.512",
        item_type: "journalArticle",
        abstract_text: "We profile four reference managers on a library of ten thousand items and find that startup time is dominated by eager metadata parsing rather than by database access.",
        age_days: 118,
        is_favorite: false,
        is_read: false,
        citation_count: 8,
        collection: &["Bibliometrics"],
        tags: &["to-read", "benchmarks"],
        pdf: false,
    },
    Seed {
        title: "Open Access Coverage Across Disciplines, 2010-2023",
        authors: &["Nadia Haddad", "Jonas Wirth"],
        year: 2024,
        journal: "Quantitative Science Studies",
        doi: "10.1162/qss_a_00291",
        item_type: "journalArticle",
        abstract_text: "Using a corpus of twelve million records we track the share of openly available versions by discipline, and find the gap between physics and the clinical sciences has widened rather than closed.",
        age_days: 132,
        is_favorite: false,
        is_read: false,
        citation_count: 23,
        collection: &["Bibliometrics"],
        tags: &["survey", "reproducible"],
        pdf: false,
    },
];

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let target = match std::env::args().nth(1) {
        Some(dir) => PathBuf::from(dir),
        None => {
            eprintln!("usage: seed_fixture <target-dir>");
            std::process::exit(2);
        }
    };

    // A stale library would silently merge with the new one and make the
    // screenshots depend on run order.
    if target.exists() {
        std::fs::remove_dir_all(&target)?;
    }
    std::fs::create_dir_all(target.join("pdfs"))?;

    let db = Database::open(target.clone()).await?;

    let mut tag_ids = Vec::new();
    for (name, color) in TAG_COLORS {
        tag_ids.push((*name, db.get_or_create_tag(name, Some(color)).await?));
    }

    let pdf_path = install_pdf(&target)?;
    let now = Utc::now();
    let mut first_paper_id = String::new();

    for seed in SEEDS {
        let collection_id = ensure_collection_path(&db, seed.collection).await?;

        let added = now - Duration::days(seed.age_days);
        let mut paper = Paper::new(seed.title.to_string());
        paper.item_type = seed.item_type.to_string();
        paper.creators = seed
            .authors
            .iter()
            .map(|a| Creator::author_from_display(a))
            .collect();
        paper.year = Some(seed.year);
        paper.doi = Some(seed.doi.to_string());
        paper.abstract_text = Some(seed.abstract_text.to_string());
        paper.publication.journal = Some(seed.journal.to_string());
        paper.status.date_added = added;
        paper.status.date_modified = added;
        paper.status.is_favorite = seed.is_favorite;
        paper.status.is_read = seed.is_read;
        paper.citation.citation_count = Some(seed.citation_count);
        if seed.pdf {
            paper.links.pdf_path = Some(pdf_path.clone());
        }

        let paper_id = db.insert_paper(&paper).await?;
        if seed.pdf {
            first_paper_id = paper_id.clone();
        }

        if let Some(cid) = collection_id {
            db.add_paper_to_collection(&paper_id, &cid).await?;
        }
        for tag in seed.tags {
            let id = tag_ids
                .iter()
                .find(|(name, _)| name == tag)
                .map(|(_, id)| id.clone())
                .expect("fixture tag declared in TAG_COLORS");
            db.add_tag_to_paper(&paper_id, &id).await?;
        }
    }

    seed_annotations_and_notes(&db, &first_paper_id).await?;
    write_config(&target)?;

    println!("Seeded {} papers into {}", SEEDS.len(), target.display());
    Ok(())
}

/// Creates each level of a collection path, returning the innermost id.
async fn ensure_collection_path(
    db: &Database,
    path: &[&str],
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let mut parent: Option<String> = None;
    for name in path {
        let existing = db
            .list_collections()
            .await?
            .into_iter()
            .find(|c| c.name == *name && c.parent_id == parent);
        parent = Some(match existing {
            Some(c) => c.id.unwrap_or_default(),
            None => {
                let mut coll = Collection::new(name.to_string());
                coll.parent_id = parent.clone();
                db.insert_collection(&coll).await?
            }
        });
    }
    Ok(parent)
}

/// Copies the PDF fixture into the library so the reader screenshots have a
/// real document, with real citations, to open.
fn install_pdf(target: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../rotero-pdf/tests/fixtures/pdfs/tracemonkey.pdf");
    let dest = target.join("pdfs").join("tracemonkey.pdf");
    std::fs::copy(&source, &dest)?;
    Ok(dest.to_string_lossy().into_owned())
}

/// Highlights and a note on the PDF-backed paper, so the annotation panel and
/// the notes section are populated rather than showing their empty states.
async fn seed_annotations_and_notes(
    db: &Database,
    paper_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if paper_id.is_empty() {
        return Ok(());
    }

    // Geometry is in rendered-page pixels, matching what the drawing gestures
    // produce (see `ui::pdf::annotation_render`).
    let annotations = [
        (
            1,
            AnnotationType::Highlight,
            "#f7d774",
            "Trace trees specialize on observed types rather than declared ones.",
            (96.0, 302.0, 404.0, 15.0),
        ),
        (
            1,
            AnnotationType::Underline,
            "#4a90d9",
            "Compilation is triggered only for loops that actually run hot.",
            (96.0, 486.0, 372.0, 13.0),
        ),
        (
            2,
            AnnotationType::Highlight,
            "#5cb85c",
            "Guards let the runtime fall back when a type assumption breaks.",
            (312.0, 208.0, 388.0, 15.0),
        ),
        (
            3,
            AnnotationType::Note,
            "#e8a33d",
            "Compare this recording strategy against the sampling approach in Lindstrom 2023.",
            (128.0, 154.0, 20.0, 20.0),
        ),
    ];

    for (page, ann_type, color, content, (x, y, w, h)) in annotations {
        let now = Utc::now();
        db.insert_annotation(&Annotation {
            id: None,
            paper_id: paper_id.to_string(),
            page,
            ann_type,
            color: color.to_string(),
            content: Some(content.to_string()),
            geometry: serde_json::json!({ "x": x, "y": y, "width": w, "height": h }),
            created_at: now,
            modified_at: now,
        })
        .await?;
    }

    let mut note = Note::new(paper_id.to_string(), "Reading notes".to_string());
    note.body = "## Why this matters\n\nThe trace-based approach sidesteps whole-method type \
inference: it records what *actually* executed, so specialization is driven by observed \
behavior.\n\n- Guards are the safety valve — every type assumption is checked\n- Bailout cost \
is what bounds how aggressive specialization can be\n\n> Worth re-reading section 4 alongside \
the deoptimization survey."
        .to_string();
    db.insert_note(&note).await?;

    Ok(())
}

/// Pins the display settings the screenshots assume, so a capture run does not
/// depend on whatever the developer last selected.
fn write_config(target: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // `SyncConfig` serializes flat, and unknown or missing keys fall back to
    // defaults — so this only needs the fields the screenshots depend on.
    let config = serde_json::json!({
        "dark_mode": false,
        "ui_scale": "default",
        "sync_enabled": false,
        "connector_enabled": false,
        "auto_check_updates": false,
        "auto_fetch_metadata": false,
    });
    std::fs::write(
        target.join("config.json"),
        serde_json::to_string_pretty(&config)?,
    )?;
    Ok(())
}
