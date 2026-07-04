//! Verifies `list_paper_ids_in_subtree` aggregates papers across a collection
//! and all of its descendants (the Zotero-style parent-view model), and that
//! the recursive CTE executes correctly on turso.

use rotero_db::{collections, papers, schema};
use rotero_models::{Collection, Paper};

async fn open_test_db(dir: &std::path::Path) -> rotero_db::turso::Connection {
    std::fs::create_dir_all(dir).unwrap();
    let db_path = dir.join("test.db");
    let db = rotero_db::turso::Builder::new_local(&db_path.to_string_lossy())
        .experimental_index_method(true)
        .build()
        .await
        .unwrap();
    let conn = db.connect().unwrap();
    schema::initialize_db(&conn).await.unwrap();
    conn
}

async fn new_collection(
    conn: &rotero_db::turso::Connection,
    name: &str,
    parent: Option<&str>,
) -> String {
    let mut coll = Collection::new(name.to_string());
    coll.parent_id = parent.map(|s| s.to_string());
    collections::insert_collection(conn, &coll).await.unwrap()
}

async fn new_paper(conn: &rotero_db::turso::Connection, title: &str) -> String {
    papers::insert_paper(conn, &Paper::new(title.to_string()))
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
    let conn = open_test_db(dir.path()).await;

    // Tree:  ML
    //         ├── NLP
    //         │    └── LLM
    //         └── Vision
    //        Unrelated (separate root)
    let ml = new_collection(&conn, "ML", None).await;
    let nlp = new_collection(&conn, "NLP", Some(&ml)).await;
    let llm = new_collection(&conn, "LLM", Some(&nlp)).await;
    let vision = new_collection(&conn, "Vision", Some(&ml)).await;
    let other = new_collection(&conn, "Unrelated", None).await;

    let p_ml = new_paper(&conn, "directly in ML").await;
    let p_nlp = new_paper(&conn, "in NLP").await;
    let p_llm = new_paper(&conn, "deep in LLM").await;
    let p_vision = new_paper(&conn, "in Vision").await;
    let p_shared = new_paper(&conn, "in both NLP and Vision").await;
    let p_other = new_paper(&conn, "unrelated").await;

    collections::add_paper_to_collection(&conn, &p_ml, &ml).await.unwrap();
    collections::add_paper_to_collection(&conn, &p_nlp, &nlp).await.unwrap();
    collections::add_paper_to_collection(&conn, &p_llm, &llm).await.unwrap();
    collections::add_paper_to_collection(&conn, &p_vision, &vision).await.unwrap();
    // Shared paper lives in two subtree collections -> must be deduped
    collections::add_paper_to_collection(&conn, &p_shared, &nlp).await.unwrap();
    collections::add_paper_to_collection(&conn, &p_shared, &vision).await.unwrap();
    collections::add_paper_to_collection(&conn, &p_other, &other).await.unwrap();

    // ML: own paper + every descendant, deduped, excluding the unrelated root.
    let got = sorted(collections::list_paper_ids_in_subtree(&conn, &ml).await.unwrap());
    let want = sorted(vec![
        p_ml.clone(),
        p_nlp.clone(),
        p_llm.clone(),
        p_vision.clone(),
        p_shared.clone(),
    ]);
    assert_eq!(got, want, "ML subtree = own + all descendant papers, deduped");
    assert!(!got.contains(&p_other), "unrelated root's paper must be excluded");

    // NLP: its own paper, the shared one, plus its LLM child — but not Vision's.
    let nlp_got = sorted(collections::list_paper_ids_in_subtree(&conn, &nlp).await.unwrap());
    let nlp_want = sorted(vec![p_nlp.clone(), p_llm.clone(), p_shared.clone()]);
    assert_eq!(nlp_got, nlp_want, "NLP subtree includes LLM child but not Vision");
    assert!(!nlp_got.contains(&p_vision), "Vision's paper must not appear under NLP");

    // Vision leaf: only its two direct papers.
    let vision_got = sorted(collections::list_paper_ids_in_subtree(&conn, &vision).await.unwrap());
    assert_eq!(vision_got, sorted(vec![p_vision, p_shared]));

    // A collection id that doesn't exist yields nothing (no panic on empty CTE seed).
    let none = collections::list_paper_ids_in_subtree(&conn, "does-not-exist")
        .await
        .unwrap();
    assert!(none.is_empty());
}
