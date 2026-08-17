//! Print how many papers a library contains, without initializing it.
//!
//! Used by the bundle smoke test to assert that data written over the connector
//! API actually persisted. Attaches rather than opens so the check cannot repair
//! or alter the library it is inspecting.
//!
//! Usage: `cargo run -p rotero-db --example count_papers -- <library-dir>`

use rotero_db::Database;

#[tokio::main]
async fn main() {
    let Some(dir) = std::env::args().nth(1) else {
        eprintln!("usage: count_papers <library-dir>");
        std::process::exit(2);
    };

    let db = match Database::attach_readonly(std::path::PathBuf::from(&dir)).await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("failed to attach {dir}: {e}");
            std::process::exit(2);
        }
    };

    match db.list_papers().await {
        Ok(papers) => println!("{}", papers.len()),
        Err(e) => {
            eprintln!("failed to list papers: {e}");
            std::process::exit(2);
        }
    }
}
