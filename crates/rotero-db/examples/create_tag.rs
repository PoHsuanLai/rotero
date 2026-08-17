//! Create a tag in a library and print its id.
//!
//! The bundle smoke test needs a tag to attach, and the connector API can only
//! reference existing ones (`/api/save` takes `tag_ids`, and nothing there
//! creates a tag). A throwaway library starts empty, so without this the
//! tag-survives-a-restart assertion — the one the script exists for — would skip
//! itself and report nothing.
//!
//! Opens the library properly rather than attaching read-only: this is seeding a
//! fixture, so the write has to be tracked exactly as the app would track it.
//!
//! Usage: `cargo run -p rotero-db --example create_tag -- <library-dir> <name>`

use rotero_db::Database;

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(dir), Some(name)) = (args.next(), args.next()) else {
        eprintln!("usage: create_tag <library-dir> <name>");
        std::process::exit(2);
    };

    let db = match Database::open(std::path::PathBuf::from(&dir)).await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("failed to open {dir}: {e}");
            std::process::exit(2);
        }
    };

    match db.get_or_create_tag(&name, None).await {
        Ok(id) => println!("{id}"),
        Err(e) => {
            eprintln!("failed to create tag: {e}");
            std::process::exit(2);
        }
    }
}
