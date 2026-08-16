//! Check an existing library directory's structural invariants from outside the app.
//!
//! The bundle smoke test launches the real release artifact, then runs this
//! against the data directory the app produced. Asserting on the resulting
//! database rather than on the UI keeps the check independent of how startup is
//! organized, and lets it run on CI runners with no display.
//!
//! Usage: `cargo run -p rotero-db --example verify_health -- <library-dir>`
//! Exits 0 when healthy, 1 when not, 2 on a usage or open error.

use rotero_db::Database;
use rotero_db::health::verify_database_health;

#[tokio::main]
async fn main() {
    let Some(dir) = std::env::args().nth(1) else {
        eprintln!("usage: verify_health <library-dir>");
        std::process::exit(2);
    };

    let path = std::path::PathBuf::from(&dir);
    if !path.join("rotero.db").exists() {
        eprintln!("no rotero.db in {dir}");
        std::process::exit(2);
    }

    // Deliberately NOT `Database::open`: that calls `crr.init()`, which recreates
    // any missing metadata and would repair the exact defect this is meant to
    // detect. Attach to the database as-is so the check reports what the app
    // actually left on disk.
    let db = match Database::attach_readonly(path).await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("failed to open {dir}: {e}");
            std::process::exit(2);
        }
    };

    let issues = verify_database_health(&db).await;
    if issues.is_empty() {
        println!("healthy: {dir}");
        return;
    }

    eprintln!("unhealthy: {dir}");
    for issue in &issues {
        eprintln!("  - {issue}");
    }
    std::process::exit(1);
}
