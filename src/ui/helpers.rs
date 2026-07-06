use dioxus::prelude::*;

use crate::sync::engine::SyncConfig;

/// Update a config field and persist to disk.
/// Avoids the AlreadyBorrowed panic from `config.read().save()` right after `config.with_mut()`.
pub fn save_config(config: &mut Signal<SyncConfig>, f: impl FnOnce(&mut SyncConfig)) {
    config.with_mut(|c| {
        f(c);
        let _ = c.save();
    });
}

/// The short, human display label for a Zotero item type, shown in the type
/// badge (e.g. `journalArticle` → "Article", `conferencePaper` → "Conf").
pub fn item_type_label(item_type: &str) -> &'static str {
    match item_type {
        "journalArticle" => "Article",
        "conferencePaper" => "Conf",
        "book" => "Book",
        "bookSection" => "Chapter",
        "thesis" => "Thesis",
        "report" => "Report",
        "dataset" => "Dataset",
        "patent" => "Patent",
        "computerProgram" => "Software",
        "webpage" => "Web",
        "blogPost" => "Blog",
        "magazineArticle" => "Magazine",
        "newspaperArticle" => "News",
        "manuscript" => "Manuscript",
        "letter" => "Letter",
        "presentation" => "Talk",
        "audioRecording" => "Audio",
        "videoRecording" => "Video",
        "film" => "Film",
        "artwork" => "Artwork",
        "map" => "Map",
        "case" => "Case",
        "statute" => "Statute",
        "bill" => "Bill",
        "hearing" => "Hearing",
        "encyclopediaArticle" => "Encyclopedia",
        "dictionaryEntry" => "Dictionary",
        "document" => "Document",
        _ => "Item",
    }
}

/// A Bootstrap Icons class for a Zotero item type, shown alongside the badge
/// label. Falls back to a generic file glyph.
pub fn item_type_icon(item_type: &str) -> &'static str {
    match item_type {
        "book" | "bookSection" | "encyclopediaArticle" | "dictionaryEntry" => "bi-book",
        "conferencePaper" | "presentation" => "bi-easel",
        "thesis" => "bi-mortarboard",
        "report" => "bi-clipboard-data",
        "dataset" => "bi-database",
        "patent" => "bi-award",
        "computerProgram" => "bi-code-square",
        "webpage" | "blogPost" => "bi-globe",
        "magazineArticle" | "newspaperArticle" => "bi-newspaper",
        "audioRecording" => "bi-music-note-beamed",
        "videoRecording" | "film" => "bi-camera-reels",
        "artwork" => "bi-palette",
        "map" => "bi-geo-alt",
        "case" | "statute" | "bill" | "hearing" => "bi-bank",
        _ => "bi-file-earmark-text",
    }
}

/// Whether an item type is anything other than a plain journal article. The
/// badge tints "special" types so the exceptions stand out in the library list
/// while the common article case stays neutral.
pub fn item_type_is_special(item_type: &str) -> bool {
    item_type != "journalArticle"
}

/// The label for a paper's venue (`publication.journal`) field, chosen by item
/// type so a book, conference paper, or report doesn't display its venue under
/// "Journal". Only the journal family is labeled "Journal"; anything unmapped
/// falls back to the neutral "Publication".
pub fn venue_label(item_type: &str) -> &'static str {
    match item_type {
        "journalArticle" => "Journal",
        "conferencePaper" => "Conference",
        "book" | "bookSection" => "Book Title",
        "thesis" => "University",
        "report" => "Institution",
        "webpage" | "blogPost" => "Website",
        "magazineArticle" => "Magazine",
        "newspaperArticle" => "Newspaper",
        _ => "Publication",
    }
}
