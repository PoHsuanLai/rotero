//! Verifies `list_paper_ids_in_subtree` aggregates papers across a collection
//! and all of its descendants (the Zotero-style parent-view model), and that
//! the recursive CTE executes correctly on turso.

use rotero_db::Database;
use rotero_models::{Collection, Paper};

async fn open_test_db(dir: &std::path::Path) -> Database {
    Database::open(dir.to_path_buf()).await.unwrap()
}

async fn new_collection(db: &Database, name: &str, parent: Option<&str>) -> String {
    let mut coll = Collection::new(name.to_string());
    coll.parent_id = parent.map(|s| s.to_string());
    db.insert_collection(&coll).await.unwrap()
}

async fn new_paper(db: &Database, title: &str) -> String {
    db.insert_paper(&Paper::new(title.to_string()))
        .await
        .unwrap()
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

#[tokio::test]
async fn subtree_aggregates_descendants_and_dedupes() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_test_db(dir.path()).await;

    // Tree:  ML
    //         ├── NLP
    //         │    └── LLM
    //         └── Vision
    //        Unrelated (separate root)
    let ml = new_collection(&db, "ML", None).await;
    let nlp = new_collection(&db, "NLP", Some(&ml)).await;
    let llm = new_collection(&db, "LLM", Some(&nlp)).await;
    let vision = new_collection(&db, "Vision", Some(&ml)).await;
    let other = new_collection(&db, "Unrelated", None).await;

    let p_ml = new_paper(&db, "directly in ML").await;
    let p_nlp = new_paper(&db, "in NLP").await;
    let p_llm = new_paper(&db, "deep in LLM").await;
    let p_vision = new_paper(&db, "in Vision").await;
    let p_shared = new_paper(&db, "in both NLP and Vision").await;
    let p_other = new_paper(&db, "unrelated").await;

    db.add_paper_to_collection(&p_ml, &ml).await.unwrap();
    db.add_paper_to_collection(&p_nlp, &nlp).await.unwrap();
    db.add_paper_to_collection(&p_llm, &llm).await.unwrap();
    db.add_paper_to_collection(&p_vision, &vision)
        .await
        .unwrap();
    // Shared paper lives in two subtree collections -> must be deduped
    db.add_paper_to_collection(&p_shared, &nlp).await.unwrap();
    db.add_paper_to_collection(&p_shared, &vision)
        .await
        .unwrap();
    db.add_paper_to_collection(&p_other, &other).await.unwrap();

    // ML: own paper + every descendant, deduped, excluding the unrelated root.
    let got = sorted(db.list_paper_ids_in_subtree(&ml).await.unwrap());
    let want = sorted(vec![
        p_ml.clone(),
        p_nlp.clone(),
        p_llm.clone(),
        p_vision.clone(),
        p_shared.clone(),
    ]);
    assert_eq!(
        got, want,
        "ML subtree = own + all descendant papers, deduped"
    );
    assert!(
        !got.contains(&p_other),
        "unrelated root's paper must be excluded"
    );

    // NLP: its own paper, the shared one, plus its LLM child — but not Vision's.
    let nlp_got = sorted(db.list_paper_ids_in_subtree(&nlp).await.unwrap());
    let nlp_want = sorted(vec![p_nlp.clone(), p_llm.clone(), p_shared.clone()]);
    assert_eq!(
        nlp_got, nlp_want,
        "NLP subtree includes LLM child but not Vision"
    );
    assert!(
        !nlp_got.contains(&p_vision),
        "Vision's paper must not appear under NLP"
    );

    // Vision leaf: only its two direct papers.
    let vision_got = sorted(db.list_paper_ids_in_subtree(&vision).await.unwrap());
    assert_eq!(vision_got, sorted(vec![p_vision, p_shared]));

    // A collection id that doesn't exist yields nothing (no panic on empty CTE seed).
    let none = db
        .list_paper_ids_in_subtree("does-not-exist")
        .await
        .unwrap();
    assert!(none.is_empty());
}
