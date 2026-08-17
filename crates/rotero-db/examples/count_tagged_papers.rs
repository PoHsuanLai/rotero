//! Print how many papers carry at least one tag, without initializing anything.
//!
//! The bundle smoke test's central assertion. A tag is a junction row, and the
//! shipped bug committed that row and *then* failed change tracking — so the tag
//! appeared to save, and was gone on the next launch. Reading it back from disk
//! after the process has exited is the only way to see that from outside.
//!
//! Attaches rather than opens, so the check cannot repair the very defect it is
//! looking for.
//!
//! Usage: `cargo run -p rotero-db --example count_tagged_papers -- <library-dir>`

use rotero_db::Database;

#[tokio::main]
async fn main() {
    let Some(dir) = std::env::args().nth(1) else {
        eprintln!("usage: count_tagged_papers <library-dir>");
        std::process::exit(2);
    };

    let db = match Database::attach_readonly(std::path::PathBuf::from(&dir)).await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("failed to attach {dir}: {e}");
            std::process::exit(2);
        }
    };

    let mut rows = match db
        .conn()
        .query("SELECT COUNT(DISTINCT paper_id) FROM paper_tags", ())
        .await
    {
        Ok(rows) => rows,
        Err(e) => {
            eprintln!("failed to count tagged papers: {e}");
            std::process::exit(2);
        }
    };

    let count = match rows.next().await {
        Ok(Some(row)) => row
            .get_value(0)
            .ok()
            .and_then(|v| v.as_integer().copied())
            .unwrap_or(0),
        _ => 0,
    };

    println!("{count}");
}
