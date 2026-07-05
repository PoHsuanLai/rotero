//! Differential harness: in-process registry vs. the Node translation server on
//! `:1969`, over a URL corpus, diffed field-by-field.
//!
//! This is a **measurement tool, not a pass/fail gate** — the Node server is the
//! reference and is expected to be richer on many sites. It answers "which
//! fields does the in-process engine drop or diverge on, per site?" so the
//! swarm (2e) can target the worst offenders.
//!
//! `#[ignore]` and feature-gated: it needs live network access and a running
//! Node server, so it never runs in CI. Run it explicitly:
//!
//! ```sh
//! # with the app's Node server already up on :1969 (e.g. via `just run`):
//! cargo test -p rotero-translate --features translator-engine \
//!     --test differential -- --ignored --nocapture
//! ```
#![cfg(feature = "translator-engine")]

use rotero_models::Paper;
use rotero_translate::translators::{fetch_context, TranslatorRegistry};
use rotero_translate::TranslationServer;

/// URLs to compare. Chosen to span translator shapes: a pure-XPath site, a
/// citation-meta hub, a delegating site, and a preprint server.
const CORPUS: &[&str] = &[
    "http://theoryofcomputing.org/articles/v009a013/",
    "https://journals.plos.org/plosone/article?id=10.1371/journal.pone.0173664",
    "https://www.nature.com/articles/nature12373",
    "https://arxiv.org/abs/1706.03762",
];

/// The fields compared, each as an extractor returning a normalized string.
fn fields(p: &Paper) -> Vec<(&'static str, String)> {
    vec![
        ("title", norm(&p.title)),
        ("authors", p.authors.iter().map(|a| norm(a)).collect::<Vec<_>>().join(" | ")),
        ("year", p.year.map(|y| y.to_string()).unwrap_or_default()),
        ("doi", p.doi.clone().unwrap_or_default().to_lowercase()),
        ("journal", opt(&p.publication.journal)),
        ("volume", opt(&p.publication.volume)),
        ("issue", opt(&p.publication.issue)),
        ("pages", opt(&p.publication.pages)),
        // Abstracts vary in whitespace/truncation between paths; compare presence.
        ("abstract?", if p.abstract_text.is_some() { "yes" } else { "" }.to_string()),
    ]
}

fn opt(s: &Option<String>) -> String {
    s.as_deref().map(norm).unwrap_or_default()
}

/// Normalize for comparison: collapse whitespace, trim, lowercase.
fn norm(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

#[tokio::test]
#[ignore = "needs live network + Node server on :1969; measurement tool, run manually"]
async fn differential_report() {
    // The Node server must already be running (the app starts it). If it isn't
    // reachable, bail with a clear message rather than a confusing failure.
    let server = TranslationServer::new(
        1969,
        "node".into(),
        "npm".into(),
    );
    if server.translate_web(CORPUS[0]).await.is_err()
        && !server_reachable().await
    {
        eprintln!(
            "\n  Node server not reachable on :1969 — start it (e.g. `just run`) \
             and retry.\n  Skipping differential report."
        );
        return;
    }

    let registry = TranslatorRegistry::with_builtins();

    // Per-field tallies across the corpus.
    let field_names: Vec<&str> = fields(&Paper::default()).iter().map(|(n, _)| *n).collect();
    let mut agree = vec![0usize; field_names.len()];
    let mut compared = vec![0usize; field_names.len()];

    println!("\n=== Differential: in-process registry vs Node :1969 ===\n");

    for url in CORPUS {
        println!("URL {url}");

        let engine_paper = match fetch_context(url).await {
            Ok(ctx) => registry
                .translate_context(&ctx)
                .await
                .and_then(|items| items.into_iter().find_map(|i| i.into_paper())),
            Err(e) => {
                println!("  fetch failed: {e}\n");
                continue;
            }
        };
        let node_paper = server
            .translate_web(url)
            .await
            .ok()
            .and_then(|items| items.into_iter().find_map(|i| i.into_paper()));

        match (&engine_paper, &node_paper) {
            (Some(e), Some(n)) => {
                let ef = fields(e);
                let nf = fields(n);
                for (i, ((name, ev), (_, nv))) in ef.iter().zip(nf.iter()).enumerate() {
                    // Only tally fields where the reference (Node) has a value.
                    if nv.is_empty() {
                        continue;
                    }
                    compared[i] += 1;
                    let ok = ev == nv;
                    if ok {
                        agree[i] += 1;
                    }
                    let mark = if ok { "ok " } else { "DIFF" };
                    println!("  {mark} {name:<10} engine={ev:?} node={nv:?}");
                }
            }
            (Some(_), None) => println!("  engine extracted, Node did not"),
            (None, Some(_)) => println!("  Node extracted, engine did not (MISS)"),
            (None, None) => println!("  neither extracted"),
        }
        println!();
    }

    println!("=== Per-field agreement (of fields where Node had a value) ===");
    for (i, name) in field_names.iter().enumerate() {
        let pct = if compared[i] == 0 {
            "n/a".to_string()
        } else {
            format!("{:.0}%", agree[i] as f64 / compared[i] as f64 * 100.0)
        };
        println!("  {name:<10} {}/{} {pct}", agree[i], compared[i]);
    }
}

/// A lightweight health check against the Node server's root.
async fn server_reachable() -> bool {
    reqwest::Client::new()
        .get("http://127.0.0.1:1969/")
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .await
        .is_ok()
}
