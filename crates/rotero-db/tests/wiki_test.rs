//! Concept pages, claims, edges, and reference stubs.

mod common;

use rotero_db::Database;
use rotero_models::{
    CitationRecord, ClaimDraft, ClaimStatus, ConceptKind, Paper, PaperId, WikiRel,
};

async fn paper(db: &Database, title: &str, doi: Option<&str>) -> String {
    db.insert_paper(&Paper {
        title: title.into(),
        doi: doi.map(str::to_string),
        ..Default::default()
    })
    .await
    .unwrap()
}

fn draft(paper_id: &str, statement: &str, quote: &str, status: ClaimStatus) -> ClaimDraft {
    ClaimDraft {
        paper_id: paper_id.to_string(),
        statement: statement.to_string(),
        quote: quote.to_string(),
        page: None,
        annotation_id: None,
        status,
    }
}

#[tokio::test]
async fn upsert_merges_on_kind_and_slug_and_revives_a_tombstone() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;

    let created = db
        .upsert_concept(ConceptKind::Method, "Low-Rank Adaptation", Some("first"))
        .await
        .unwrap();
    let again = db
        .upsert_concept(ConceptKind::Method, "low rank adaptation", Some("second"))
        .await
        .unwrap();
    assert_eq!(created.id, again.id);
    assert_eq!(again.body, "second");
    assert_eq!(again.slug, "low-rank-adaptation");

    let id = again.id.clone().unwrap();
    db.tombstone("concepts", rotero_db::clock::Pk::Single(&id))
        .await
        .unwrap();
    assert!(db.get_concept(&id).await.unwrap().is_none());

    let revived = db
        .upsert_concept(ConceptKind::Method, "Low-Rank Adaptation", None)
        .await
        .unwrap();
    assert_eq!(revived.id.as_deref(), Some(id.as_str()));
    assert_eq!(revived.body, "second");

    let err = db
        .upsert_concept(ConceptKind::Idea, "   ", None)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("empty"), "{err}");

    let err = db
        .upsert_concept(ConceptKind::Idea, "!!!", None)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("letters"), "{err}");
}

#[tokio::test]
async fn file_claim_protects_a_quotation_from_chat() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;
    let paper_id = paper(&db, "Source", None).await;

    let err = db
        .file_claim(&draft(&paper_id, "It works.", "", ClaimStatus::Extracted))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("quotation"), "{err}");

    let filed = db
        .file_claim(&ClaimDraft {
            page: Some(4),
            annotation_id: Some("ann-1".into()),
            ..draft(&paper_id, "It  works.", "It works.", ClaimStatus::Extracted)
        })
        .await
        .unwrap();
    assert_eq!(filed.quote, "It works.");
    assert_eq!(filed.page, Some(4));

    let untouched = db
        .file_claim(&draft(
            &paper_id,
            "It works.",
            "a different sentence",
            ClaimStatus::FromChat,
        ))
        .await
        .unwrap();
    assert_eq!(untouched.id, filed.id);
    assert_eq!(untouched.quote, "It works.");
    assert_eq!(untouched.status, ClaimStatus::Extracted);

    let chat_only = db
        .file_claim(&draft(&paper_id, "Maybe later.", "", ClaimStatus::FromChat))
        .await
        .unwrap();
    assert_eq!(chat_only.status, ClaimStatus::FromChat);
    assert!(chat_only.quote.is_empty());

    let upgraded = db
        .file_claim(&ClaimDraft {
            page: Some(2),
            ..draft(
                &paper_id,
                "Maybe later.",
                "Maybe later.",
                ClaimStatus::Extracted,
            )
        })
        .await
        .unwrap();
    assert_eq!(upgraded.id, chat_only.id);
    assert_eq!(upgraded.quote, "Maybe later.");
    assert_eq!(upgraded.page, Some(2));
    assert_eq!(upgraded.status, ClaimStatus::Extracted);

    let confirmed = db
        .file_claim(&draft(
            &paper_id,
            "It works.",
            "It works.",
            ClaimStatus::Confirmed,
        ))
        .await
        .unwrap();
    assert_eq!(confirmed.status, ClaimStatus::Confirmed);
    let still = db
        .file_claim(&draft(
            &paper_id,
            "It works.",
            "It works on page 9.",
            ClaimStatus::Extracted,
        ))
        .await
        .unwrap();
    assert_eq!(still.status, ClaimStatus::Confirmed);
    assert_eq!(still.quote, "It works.");
}

#[tokio::test]
async fn links_reject_a_self_edge_and_store_concepts_once() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;
    let paper_id = paper(&db, "Source", None).await;
    let claim = db
        .file_claim(&draft(
            &paper_id,
            "Sparse routing helps.",
            "Sparse routing helps.",
            ClaimStatus::Extracted,
        ))
        .await
        .unwrap();
    let claim_id = claim.id.unwrap();
    let err = db
        .link_claims(&claim_id, &claim_id, WikiRel::Supports)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("itself"), "{err}");
    let err = db
        .link_claims(&claim_id, &claim_id, WikiRel::About)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("supports"), "{err}");

    let a = db
        .upsert_concept(ConceptKind::Method, "Mixture of Experts", None)
        .await
        .unwrap()
        .id
        .unwrap();
    let b = db
        .upsert_concept(ConceptKind::Idea, "Sparse routing", None)
        .await
        .unwrap()
        .id
        .unwrap();
    let forward = db.link_concepts(&a, &b).await.unwrap();
    let backward = db.link_concepts(&b, &a).await.unwrap();
    assert_eq!(forward, backward);

    db.link_claim_concept(&claim_id, &a).await.unwrap();
    let page = db.read_concept(&a).await.unwrap();
    assert_eq!(page.claims.len(), 1);
    assert_eq!(page.related.len(), 1);
}

#[tokio::test]
async fn deleting_a_paper_hides_its_claims_and_leaves_shared_concepts() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;
    let gone = paper(&db, "Gone", None).await;
    let stays = paper(&db, "Stays", None).await;
    let concept = db
        .upsert_concept(ConceptKind::Dataset, "ImageNet", Some("photos"))
        .await
        .unwrap();
    let concept_id = concept.id.unwrap();

    let doomed = db
        .file_claim(&draft(
            &gone,
            "ImageNet is large.",
            "ImageNet is large.",
            ClaimStatus::Extracted,
        ))
        .await
        .unwrap();
    let doomed_id = doomed.id.clone().unwrap();
    db.link_claim_concept(&doomed_id, &concept_id)
        .await
        .unwrap();

    let kept = db
        .file_claim(&draft(
            &stays,
            "ImageNet is still used.",
            "ImageNet is still used.",
            ClaimStatus::Extracted,
        ))
        .await
        .unwrap();
    db.link_claim_concept(kept.id.as_deref().unwrap(), &concept_id)
        .await
        .unwrap();

    db.note_external_citation(&gone, "https://doi.org/10.1000/xyz")
        .await
        .unwrap();
    assert_eq!(db.list_stubs(Some(&gone)).await.unwrap().len(), 1);

    db.delete_paper(&gone).await.unwrap();
    assert!(db.list_claims_for_paper(&gone).await.unwrap().is_empty());
    assert!(db.list_stubs(Some(&gone)).await.unwrap().is_empty());
    let page = db.read_concept(&concept_id).await.unwrap();
    assert_eq!(page.concept.title, "ImageNet");
    assert_eq!(page.claims.len(), 1);
    assert_eq!(page.claims[0].paper_id, stays);
}

#[tokio::test]
async fn an_unresolved_identifier_becomes_a_stub_and_a_known_one_a_citation() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;
    let citing = paper(&db, "Citing", Some("10.1000/citing")).await;

    let ignored = db
        .note_external_citation(&citing, "https://example.com/no-id")
        .await
        .unwrap();
    assert_eq!(ignored, CitationRecord::Ignored);
    assert!(db.list_stubs(None).await.unwrap().is_empty());

    let first = db
        .note_external_citation(&citing, "https://doi.org/10.1000/missing")
        .await
        .unwrap();
    let CitationRecord::Stub { id } = first else {
        panic!("expected a stub, got {first:?}");
    };
    let second = db
        .note_external_citation(&citing, "https://doi.org/10.1000/missing")
        .await
        .unwrap();
    assert_eq!(second, CitationRecord::Stub { id: id.clone() });
    assert_eq!(db.list_stubs(Some(&citing)).await.unwrap().len(), 1);

    let cited = paper(&db, "Missing", Some("10.1000/missing")).await;
    assert!(
        db.list_stubs(None).await.unwrap().is_empty(),
        "adding the paper retires the stub"
    );
    let again = db
        .note_external_citation(&citing, "https://doi.org/10.1000/missing")
        .await
        .unwrap();
    assert_eq!(
        again,
        CitationRecord::Citation {
            cited_id: cited.clone()
        }
    );
    let cited_ids = db.list_cited_by_paper(&citing).await.unwrap();
    assert_eq!(cited_ids, vec![cited]);

    let stored = PaperId::parse("10.1000/missing")
        .unwrap()
        .to_stored_string();
    assert_eq!(stored, "10.1000/missing");
}

#[tokio::test]
async fn a_snapshot_carries_a_concept_and_a_claim() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let a = common::open_test_db(dir_a.path()).await;
    let b = common::open_test_db(dir_b.path()).await;

    let paper_id = paper(&a, "Synced", None).await;
    a.upsert_concept(
        ConceptKind::Task,
        "Language modeling",
        Some("predict the next token"),
    )
    .await
    .unwrap();
    a.file_claim(&draft(
        &paper_id,
        "Scale helps.",
        "Scale helps.",
        ClaimStatus::Extracted,
    ))
    .await
    .unwrap();

    b.merge_snapshot(&a.write_snapshot().await.unwrap())
        .await
        .unwrap();

    let concepts = b
        .list_concepts(Some(ConceptKind::Task), None)
        .await
        .unwrap();
    assert_eq!(concepts.len(), 1);
    assert_eq!(concepts[0].title, "Language modeling");
    let claims = b.list_claims_for_paper(&paper_id).await.unwrap();
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0].quote, "Scale helps.");
}
