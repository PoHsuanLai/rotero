use dioxus::desktop::use_muda_event_handler;
use dioxus::prelude::*;

use crate::app::{DevicePixelRatio, RenderChannel, ShowSettings};
use crate::state::app_state::{
    AnnotationMode, LibraryState, LibraryView, PdfTab, PdfTabManager, ViewerToolState,
};
use crate::state::undo::{UndoStack, forward_action, reverse_action};
use crate::sync::engine::SyncConfig;
use crate::updates::{UpdateState, UpdateStatus};
use rotero_db::Database;

// ── Keybinding architecture ────────────────────────────────────────────────
//
// One source of truth for every shortcut. A `Command` is a semantic action; the
// `BINDINGS` table maps keys + scope → command and (optionally) a menu id. Both
// the DOM keydown handler and the native-menu handler resolve to a `Command`
// and then call `dispatch`, so a shortcut is described in exactly one place and
// the menu bar in `init/window.rs` is generated from the same table.
//
// Precedence between "typing in a field" and "pressing a shortcut" is handled
// separately: local input handlers call `stop_propagation()` (see
// `text_input_onkeydown`), and `assets/keybindings.js` is the capture-phase
// backstop for bare keys. This module never sees a keystroke that a focused
// input claimed.

/// A semantic action, decoupled from the key that triggers it. This is the unit
/// a rebinding UI lets users remap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    OpenSettings,
    OpenPdf,
    ImportBibtex,
    ExportBibtex,
    Find,
    FocusLibrarySearch,
    CloseTab,
    NewCollection,
    ShowLibrary,
    PrevTab,
    NextTab,
    Undo,
    Redo,
    SelectAll,
    ToggleFavorite,
    ToggleRead,
    CheckUpdates,
    SelectNext,
    SelectPrev,
    OpenSelected,
    DeleteSelected,
    Escape,
}

impl Command {
    /// Stable string id used as the config key for a user override. Changing one
    /// of these orphans existing overrides, so treat them as a wire format.
    pub fn id(self) -> &'static str {
        match self {
            Self::OpenSettings => "open_settings",
            Self::OpenPdf => "open_pdf",
            Self::ImportBibtex => "import_bibtex",
            Self::ExportBibtex => "export_bibtex",
            Self::Find => "find",
            Self::FocusLibrarySearch => "focus_library_search",
            Self::CloseTab => "close_tab",
            Self::NewCollection => "new_collection",
            Self::ShowLibrary => "show_library",
            Self::PrevTab => "prev_tab",
            Self::NextTab => "next_tab",
            Self::Undo => "undo",
            Self::Redo => "redo",
            Self::SelectAll => "select_all",
            Self::ToggleFavorite => "toggle_favorite",
            Self::ToggleRead => "toggle_read",
            Self::CheckUpdates => "check_updates",
            Self::SelectNext => "select_next",
            Self::SelectPrev => "select_prev",
            Self::OpenSelected => "open_selected",
            Self::DeleteSelected => "delete_selected",
            Self::Escape => "escape",
        }
    }

    /// Human-readable name shown in the keybindings settings page.
    pub fn label(self) -> &'static str {
        match self {
            Self::OpenSettings => "Open settings",
            Self::OpenPdf => "Open PDF\u{2026}",
            Self::ImportBibtex => "Import BibTeX\u{2026}",
            Self::ExportBibtex => "Export BibTeX\u{2026}",
            Self::Find => "Find in page / library",
            Self::FocusLibrarySearch => "Focus library search",
            Self::CloseTab => "Close tab",
            Self::NewCollection => "New collection",
            Self::ShowLibrary => "Show library",
            Self::PrevTab => "Previous tab",
            Self::NextTab => "Next tab",
            Self::Undo => "Undo",
            Self::Redo => "Redo",
            Self::SelectAll => "Select all papers",
            Self::ToggleFavorite => "Toggle favorite",
            Self::ToggleRead => "Toggle read",
            Self::CheckUpdates => "Check for updates",
            Self::SelectNext => "Select next paper",
            Self::SelectPrev => "Select previous paper",
            Self::OpenSelected => "Open selected paper",
            Self::DeleteSelected => "Delete selected papers",
            Self::Escape => "Cancel / dismiss",
        }
    }

    /// Whether the settings page lets the user rebind this command.
    ///
    /// `Escape` is a hard-wired cancel key and `CheckUpdates` has no keyboard
    /// shortcut (menu-only), so neither is user-rebindable.
    pub fn is_rebindable(self) -> bool {
        !matches!(self, Self::Escape | Self::CheckUpdates)
    }
}

/// Where a binding is active. `Global` fires regardless of view; the others only
/// fire when the current view matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Global,
    Library,
    Viewer,
}

/// A key combination, normalized so the DOM handler, the menu accelerator, and
/// the config file all agree on the same representation. Serialized as a compact
/// string like `"cmd+shift+z"` or `"arrowdown"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeySpec {
    pub cmd: bool,
    pub shift: bool,
    pub trigger: Trigger,
}

/// The non-modifier part of a key combination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// A printable character, matched case-insensitively.
    Char(char),
    ArrowUp,
    ArrowDown,
    Enter,
    Backspace,
    Escape,
}

impl Trigger {
    /// Lowercase token used in the serialized form.
    fn token(self) -> String {
        match self {
            Trigger::Char(c) => c.to_ascii_lowercase().to_string(),
            Trigger::ArrowUp => "arrowup".to_string(),
            Trigger::ArrowDown => "arrowdown".to_string(),
            Trigger::Enter => "enter".to_string(),
            Trigger::Backspace => "backspace".to_string(),
            Trigger::Escape => "escape".to_string(),
        }
    }

    fn from_token(tok: &str) -> Option<Self> {
        Some(match tok {
            "arrowup" => Trigger::ArrowUp,
            "arrowdown" => Trigger::ArrowDown,
            "enter" => Trigger::Enter,
            "backspace" => Trigger::Backspace,
            "escape" => Trigger::Escape,
            _ => {
                let mut chars = tok.chars();
                let c = chars.next()?;
                if chars.next().is_some() {
                    return None; // multi-char token that isn't a known name
                }
                Trigger::Char(c)
            }
        })
    }

    /// Symbol shown in the UI (e.g. the up-arrow glyph).
    fn glyph(self) -> String {
        match self {
            Trigger::Char(c) => c.to_ascii_uppercase().to_string(),
            Trigger::ArrowUp => "\u{2191}".to_string(),
            Trigger::ArrowDown => "\u{2193}".to_string(),
            Trigger::Enter => "\u{21a9}".to_string(),
            Trigger::Backspace => "\u{232b}".to_string(),
            Trigger::Escape => "esc".to_string(),
        }
    }
}

impl KeySpec {
    const fn cmd(c: char) -> Self {
        Self { cmd: true, shift: false, trigger: Trigger::Char(c) }
    }
    const fn cmd_shift(c: char) -> Self {
        Self { cmd: true, shift: true, trigger: Trigger::Char(c) }
    }
    const fn bare(trigger: Trigger) -> Self {
        Self { cmd: false, shift: false, trigger }
    }

    /// Does this spec match a live keyboard event's modifiers + key?
    fn matches(&self, cmd: bool, shift: bool, key: &Key) -> bool {
        if self.cmd != cmd || self.shift != shift {
            return false;
        }
        match self.trigger {
            Trigger::Char(want) => matches!(
                key,
                Key::Character(c) if c.chars().next().is_some_and(|k| k.eq_ignore_ascii_case(&want))
            ),
            Trigger::ArrowUp => *key == Key::ArrowUp,
            Trigger::ArrowDown => *key == Key::ArrowDown,
            Trigger::Enter => *key == Key::Enter,
            // Backspace and Delete both mean "delete selected" in the library.
            Trigger::Backspace => *key == Key::Backspace || *key == Key::Delete,
            Trigger::Escape => *key == Key::Escape,
        }
    }

    /// Compact serialized form, e.g. `"cmd+shift+z"`, `"arrowdown"`.
    fn to_token_string(self) -> String {
        let mut parts = Vec::new();
        if self.cmd {
            parts.push("cmd".to_string());
        }
        if self.shift {
            parts.push("shift".to_string());
        }
        parts.push(self.trigger.token());
        parts.join("+")
    }

    fn from_token_string(s: &str) -> Option<Self> {
        let mut cmd = false;
        let mut shift = false;
        let mut trigger = None;
        for part in s.split('+') {
            match part {
                "cmd" => cmd = true,
                "shift" => shift = true,
                other => trigger = Some(Trigger::from_token(other)?),
            }
        }
        Some(Self { cmd, shift, trigger: trigger? })
    }

    /// Human-facing label for the settings chip, e.g. `⌘⇧Z` or `↓`.
    pub fn display(self) -> String {
        let mut s = String::new();
        if self.cmd {
            s.push('\u{2318}'); // ⌘
        }
        if self.shift {
            s.push('\u{21e7}'); // ⇧
        }
        s.push_str(&self.trigger.glyph());
        s
    }

    /// Build a spec from a captured keyboard event, or `None` if the key can't be
    /// bound (e.g. a lone modifier press, or a non-printable key we don't model).
    pub fn from_event(cmd: bool, shift: bool, key: &Key) -> Option<Self> {
        let trigger = match key {
            Key::Character(c) => {
                let ch = c.chars().next()?;
                // Reject whitespace/control chars as bindings.
                if ch.is_control() {
                    return None;
                }
                Trigger::Char(ch)
            }
            Key::ArrowUp => Trigger::ArrowUp,
            Key::ArrowDown => Trigger::ArrowDown,
            Key::Enter => Trigger::Enter,
            Key::Backspace | Key::Delete => Trigger::Backspace,
            // Escape is reserved as cancel; don't let it be captured as a binding.
            _ => return None,
        };
        Some(Self { cmd, shift, trigger })
    }
}

impl serde::Serialize for KeySpec {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_token_string())
    }
}

impl<'de> serde::Deserialize<'de> for KeySpec {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::from_token_string(&s)
            .ok_or_else(|| serde::de::Error::custom(format!("invalid keyspec: {s:?}")))
    }
}

/// One row of the shortcut table: key + scope → command, plus an optional native
/// menu id so the menu bar can be generated from this same list.
pub struct Binding {
    pub key: KeySpec,
    pub command: Command,
    pub scope: Scope,
    pub menu_id: Option<&'static str>,
}

/// The single source of truth for every keyboard shortcut and menu accelerator.
///
/// Order matters only for resolution when two rows could match: the first match
/// wins, so put more specific scopes before `Global` if they ever share a key.
pub const BINDINGS: &[Binding] = &[
    // Global — fire in any view.
    Binding { key: KeySpec::cmd(','), command: Command::OpenSettings, scope: Scope::Global, menu_id: None },
    Binding { key: KeySpec::cmd('o'), command: Command::OpenPdf, scope: Scope::Global, menu_id: Some("open-pdf") },
    Binding { key: KeySpec::cmd('i'), command: Command::ImportBibtex, scope: Scope::Global, menu_id: Some("import-bibtex") },
    Binding { key: KeySpec::cmd('e'), command: Command::ExportBibtex, scope: Scope::Global, menu_id: Some("export-bibtex") },
    Binding { key: KeySpec::cmd('f'), command: Command::Find, scope: Scope::Global, menu_id: Some("find") },
    Binding { key: KeySpec::cmd('l'), command: Command::FocusLibrarySearch, scope: Scope::Global, menu_id: None },
    Binding { key: KeySpec::cmd('w'), command: Command::CloseTab, scope: Scope::Global, menu_id: Some("close-tab") },
    Binding { key: KeySpec::cmd('n'), command: Command::NewCollection, scope: Scope::Global, menu_id: Some("new-collection") },
    Binding { key: KeySpec::cmd('1'), command: Command::ShowLibrary, scope: Scope::Global, menu_id: Some("show-library") },
    Binding { key: KeySpec::cmd('['), command: Command::PrevTab, scope: Scope::Global, menu_id: None },
    Binding { key: KeySpec::cmd(']'), command: Command::NextTab, scope: Scope::Global, menu_id: None },
    Binding { key: KeySpec::cmd('z'), command: Command::Undo, scope: Scope::Global, menu_id: None },
    Binding { key: KeySpec::cmd_shift('z'), command: Command::Redo, scope: Scope::Global, menu_id: None },
    Binding { key: KeySpec::bare(Trigger::Escape), command: Command::Escape, scope: Scope::Global, menu_id: None },
    // Library — only when a paper list is showing.
    Binding { key: KeySpec::cmd('a'), command: Command::SelectAll, scope: Scope::Library, menu_id: None },
    Binding { key: KeySpec::cmd_shift('f'), command: Command::ToggleFavorite, scope: Scope::Library, menu_id: None },
    Binding { key: KeySpec::cmd_shift('u'), command: Command::ToggleRead, scope: Scope::Library, menu_id: None },
    Binding { key: KeySpec::bare(Trigger::ArrowDown), command: Command::SelectNext, scope: Scope::Library, menu_id: None },
    Binding { key: KeySpec::bare(Trigger::ArrowUp), command: Command::SelectPrev, scope: Scope::Library, menu_id: None },
    Binding { key: KeySpec::bare(Trigger::Enter), command: Command::OpenSelected, scope: Scope::Library, menu_id: None },
    Binding { key: KeySpec::bare(Trigger::Backspace), command: Command::DeleteSelected, scope: Scope::Library, menu_id: None },
];

/// The default (built-in) key for a command, if it has one.
pub fn default_key(command: Command) -> Option<KeySpec> {
    BINDINGS
        .iter()
        .find(|b| b.command == command)
        .map(|b| b.key)
}

/// The scope a command belongs to (from its `BINDINGS` row).
pub fn command_scope(command: Command) -> Scope {
    BINDINGS
        .iter()
        .find(|b| b.command == command)
        .map(|b| b.scope)
        .unwrap_or(Scope::Global)
}

/// User overrides: command id → binding.
///
/// - key **absent**  → use the built-in default
/// - `Some(key)`     → rebound to `key`
/// - `None`          → explicitly unbound (default suppressed, no shortcut)
pub type Overrides = std::collections::HashMap<String, Option<KeySpec>>;

/// The effective key for a command, honoring a user override if present. Returns
/// `None` when the command has no default and no override, or when the user has
/// explicitly unbound it.
pub fn effective_key(command: Command, overrides: &Overrides) -> Option<KeySpec> {
    match overrides.get(command.id()) {
        // An entry exists (rebound or explicitly unbound) — it wins over the default.
        Some(binding) => *binding,
        // No entry — fall back to the built-in default.
        None => default_key(command),
    }
}

impl Scope {
    pub fn label(self) -> &'static str {
        match self {
            Scope::Global => "Global",
            Scope::Library => "Library",
            Scope::Viewer => "PDF Viewer",
        }
    }
}

/// Every user-rebindable command, in table order. Used by the settings page.
pub fn rebindable_commands() -> impl Iterator<Item = Command> {
    BINDINGS
        .iter()
        .filter(|b| b.command.is_rebindable())
        .map(|b| b.command)
}

/// Would assigning `key` to `command` collide with another command that could
/// fire in the same scope? Returns the conflicting command if so.
///
/// Two commands conflict when their scopes overlap (either is `Global`, or they
/// share a scope) and their effective keys match the same event.
pub fn conflict_for(
    command: Command,
    key: KeySpec,
    overrides: &Overrides,
) -> Option<Command> {
    let target_scope = command_scope(command);
    for b in BINDINGS {
        if b.command == command {
            continue;
        }
        let other_scope = command_scope(b.command);
        let scopes_overlap = target_scope == Scope::Global
            || other_scope == Scope::Global
            || target_scope == other_scope;
        if !scopes_overlap {
            continue;
        }
        if effective_key(b.command, overrides) == Some(key) {
            return Some(b.command);
        }
    }
    None
}

/// Resolve a live key event within a scope to a command, honoring user overrides.
///
/// A command matches if its *effective* key (override or default) matches the
/// event and its scope is active. The default table order is preserved so that,
/// absent overrides, resolution is identical to the built-in behavior.
fn resolve(cmd: bool, shift: bool, key: &Key, scope: Scope, overrides: &Overrides) -> Option<Command> {
    BINDINGS
        .iter()
        .filter(|b| b.scope == scope || b.scope == Scope::Global)
        .find(|b| {
            effective_key(b.command, overrides)
                .is_some_and(|k| k.matches(cmd, shift, key))
        })
        .map(|b| b.command)
}

/// Look up the command a native menu item triggers.
fn command_for_menu(menu_id: &str) -> Option<Command> {
    BINDINGS
        .iter()
        .find(|b| b.menu_id == Some(menu_id))
        .map(|b| b.command)
}

/// The native-menu accelerator for a menu item, derived from its `BINDINGS` row
/// so the menu bar and the DOM handler can never disagree about a shortcut.
/// Returns `None` for menu items that have no keyboard binding.
#[cfg(feature = "desktop")]
pub fn menu_accelerator(menu_id: &str) -> Option<dioxus::desktop::muda::accelerator::Accelerator> {
    use dioxus::desktop::muda::accelerator::{Accelerator, Code, Modifiers};

    let binding = BINDINGS.iter().find(|b| b.menu_id == Some(menu_id))?;
    let key = binding.key;

    // Every menu-bound shortcut today is a Cmd+<char>. Map the character to a
    // muda Code; extend this match if a menu item ever needs a non-char key.
    let code = match key.trigger {
        Trigger::Char('o') => Code::KeyO,
        Trigger::Char('i') => Code::KeyI,
        Trigger::Char('e') => Code::KeyE,
        Trigger::Char('f') => Code::KeyF,
        Trigger::Char('w') => Code::KeyW,
        Trigger::Char('n') => Code::KeyN,
        Trigger::Char('1') => Code::Digit1,
        _ => return None,
    };

    let mut mods = Modifiers::empty();
    if key.cmd {
        mods |= Modifiers::SUPER;
    }
    if key.shift {
        mods |= Modifiers::SHIFT;
    }
    Some(Accelerator::new(Some(mods), code))
}

fn action_open_settings(mut show_settings: Signal<ShowSettings>) {
    show_settings.set(ShowSettings(true));
}

fn action_check_updates(mut update_state: Signal<UpdateState>) {
    update_state.with_mut(|s| {
        s.status = UpdateStatus::Checking;
        s.show_dialog = true;
        s.error = None;
    });
    spawn(async move {
        match crate::updates::check_for_update().await {
            Ok(Some(info)) => {
                update_state.with_mut(|s| {
                    s.status = UpdateStatus::Available;
                    s.info = Some(info);
                });
            }
            Ok(None) => {
                update_state.with_mut(|s| {
                    s.status = UpdateStatus::UpToDate;
                });
            }
            Err(e) => {
                update_state.with_mut(|s| {
                    s.status = UpdateStatus::Error;
                    s.error = Some(e);
                });
            }
        }
    });
}

fn action_open_pdf(
    mut tabs: Signal<PdfTabManager>,
    mut lib_state: Signal<LibraryState>,
    config: Signal<SyncConfig>,
    dpr_sig: Signal<DevicePixelRatio>,
) {
    let file = crate::ui::pick_file(&["pdf"], "Open PDF");
    if let Some(path) = file {
        let path_str = path.to_string_lossy().to_string();
        tabs.with_mut(|m| {
            if let Some(idx) = m.find_by_path(&path_str) {
                let tid = m.tabs[idx].id;
                m.switch_to(tid);
            } else {
                let cfg = config.read();
                let id = m.next_id();
                let title = std::path::Path::new(&path_str)
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Untitled".to_string());
                let tab = PdfTab::new(
                    id,
                    path_str.clone(),
                    title,
                    cfg.pdf.default_zoom,
                    cfg.pdf.page_batch_size,
                    dpr_sig.read().0,
                );
                m.open_tab(tab);
            }
        });
        lib_state.with_mut(|s| s.view = LibraryView::PdfViewer);
    }
}

fn action_import_bibtex(db: Database, mut lib_state: Signal<LibraryState>) {
    let file = crate::ui::pick_file(&["bib", "bibtex"], "Import BibTeX");
    if let Some(path) = file
        && let Ok(content) = std::fs::read_to_string(&path)
        && let Ok(entries) = rotero_bib::import_bibtex(&content)
    {
        let bib_dir = path.parent().map(|p| p.to_path_buf());
        spawn(async move {
            for entry in entries {
                let rotero_bib::ImportedPaper { paper, source_pdf } = entry;
                if let Ok(id) = rotero_db::papers::insert_paper(db.conn(), &paper).await {
                    let mut paper = paper;
                    paper.id = Some(id.clone());

                    if let (Some(bib_dir), Some(rel_pdf)) = (&bib_dir, &source_pdf) {
                        let pdf_abs = bib_dir.join(rel_pdf);
                        if pdf_abs.exists()
                            && let Ok(rel_path) = db.import_pdf(
                                pdf_abs.to_str().unwrap_or_default(),
                                Some(paper.title.as_str()),
                                paper.authors.first().map(|a| a.as_str()),
                                paper.year,
                            )
                        {
                            let _ =
                                rotero_db::papers::update_pdf_path(db.conn(), &id, &rel_path).await;
                            paper.links.pdf_path = Some(rel_path);
                        }
                    }

                    lib_state.with_mut(|s| s.papers.insert(0, paper));
                }
            }
        });
    }
}

fn action_export_bibtex(lib_state: Signal<LibraryState>) {
    let papers = lib_state.read().papers.clone();
    if !papers.is_empty() {
        let file = crate::ui::save_file(&["bib"], "Export BibTeX", "rotero-export.bib");
        if let Some(path) = file {
            let bibtex = rotero_bib::export_bibtex(&papers);
            let _ = std::fs::write(&path, bibtex);
        }
    }
}

fn action_find(lib_state: Signal<LibraryState>, mut tabs: Signal<PdfTabManager>) {
    let view = lib_state.read().view.clone();
    if view == LibraryView::PdfViewer {
        tabs.with_mut(|m| {
            if let Some(t) = m.active_tab_mut() {
                t.search.visible = !t.search.visible;
                if !t.search.visible {
                    t.search.query.clear();
                    t.search.matches.clear();
                    t.search.current_index = 0;
                }
            }
        });
    } else {
        spawn(async move {
            let _ = document::eval("document.getElementById('library-search-input')?.focus()");
        });
    }
}

fn action_focus_library_search(mut lib_state: Signal<LibraryState>) {
    let view = lib_state.read().view.clone();
    if view == LibraryView::PdfViewer {
        lib_state.with_mut(|s| s.view = LibraryView::AllPapers);
    }
    spawn(async move {
        let _ = document::eval(
            "setTimeout(() => { document.getElementById('library-search-input')?.focus(); }, 50)",
        );
    });
}

fn action_close_tab(
    mut tabs: Signal<PdfTabManager>,
    mut lib_state: Signal<LibraryState>,
    render_ch: RenderChannel,
    config: Signal<SyncConfig>,
    dpr_sig: Signal<DevicePixelRatio>,
) {
    let Some(tab_id) = tabs.read().active_tab_id else {
        return;
    };
    tabs.with_mut(|m| m.close_tab(tab_id));
    if tabs.read().tabs.is_empty() {
        lib_state.with_mut(|s| s.view = LibraryView::AllPapers);
        // No PDFs open — free the engine's cached file bytes.
        let _ = render_ch
            .sender()
            .send(crate::state::commands::RenderRequest::ClearCache);
    } else {
        let needs = tabs
            .read()
            .active_tab()
            .map(|t| t.needs_render())
            .unwrap_or(false);
        let new_id = tabs.read().active_tab_id;
        if needs && let Some(new_id) = new_id {
            let render_tx = render_ch.sender();
            let cfg_dir = config.read().effective_library_path();
            tabs.with_mut(|m| m.tab_mut().is_loading = true);
            spawn(async move {
                let _ = crate::state::commands::open_pdf(
                    &render_tx,
                    &mut tabs,
                    new_id,
                    &cfg_dir,
                    dpr_sig.read().0,
                )
                .await;
            });
        }
    }
}

fn action_new_collection(
    lib_state: Signal<LibraryState>,
    mut new_coll_editing: Signal<Option<Option<String>>>,
) {
    let parent = match &lib_state.read().view {
        LibraryView::Collection(id) => Some(id.clone()),
        _ => None,
    };
    new_coll_editing.set(Some(parent));
}

fn action_show_library(mut lib_state: Signal<LibraryState>) {
    lib_state.with_mut(|s| s.view = LibraryView::AllPapers);
}

fn action_prev_tab(mut tabs: Signal<PdfTabManager>, mut lib_state: Signal<LibraryState>) {
    tabs.with_mut(|m| {
        if let Some(active_id) = m.active_tab_id
            && let Some(idx) = m.tabs.iter().position(|t| t.id == active_id)
            && idx > 0
        {
            let prev_id = m.tabs[idx - 1].id;
            m.switch_to(prev_id);
        }
    });
    if lib_state.read().view != LibraryView::PdfViewer {
        lib_state.with_mut(|s| s.view = LibraryView::PdfViewer);
    }
}

fn action_next_tab(mut tabs: Signal<PdfTabManager>, mut lib_state: Signal<LibraryState>) {
    tabs.with_mut(|m| {
        if let Some(active_id) = m.active_tab_id
            && let Some(idx) = m.tabs.iter().position(|t| t.id == active_id)
            && idx + 1 < m.tabs.len()
        {
            let next_id = m.tabs[idx + 1].id;
            m.switch_to(next_id);
        }
    });
    if lib_state.read().view != LibraryView::PdfViewer {
        lib_state.with_mut(|s| s.view = LibraryView::PdfViewer);
    }
}

fn action_undo(db: Database, mut tabs: Signal<PdfTabManager>, mut undo_stack: Signal<UndoStack>) {
    let action = undo_stack.with_mut(|s| s.pop_undo());
    if let Some(action) = action {
        spawn(async move {
            reverse_action(db, &mut tabs, &mut undo_stack, action).await;
        });
    }
}

fn action_redo(db: Database, mut tabs: Signal<PdfTabManager>, mut undo_stack: Signal<UndoStack>) {
    let action = undo_stack.with_mut(|s| s.pop_redo());
    if let Some(action) = action {
        spawn(async move {
            forward_action(db, &mut tabs, &mut undo_stack, action).await;
        });
    }
}

fn action_escape(
    mut show_settings: Signal<ShowSettings>,
    mut tabs: Signal<PdfTabManager>,
    mut tools: Signal<ViewerToolState>,
    mut lib_state: Signal<LibraryState>,
) {
    let mode = tools.read().annotation_mode;
    if mode != AnnotationMode::None {
        tools.with_mut(|t| t.annotation_mode = AnnotationMode::None);
    } else if show_settings.read().0 {
        show_settings.set(ShowSettings(false));
    } else {
        let mut handled = false;
        tabs.with_mut(|m| {
            if let Some(t) = m.active_tab_mut()
                && t.search.visible
            {
                t.search.visible = false;
                t.search.query.clear();
                t.search.matches.clear();
                t.search.current_index = 0;
                handled = true;
            }
        });
        if !handled && !lib_state.read().selected_paper_ids.is_empty() {
            lib_state.with_mut(|s| s.clear_selection());
        }
    }
}

fn action_select_next(mut lib_state: Signal<LibraryState>) {
    let ids = lib_state.read().filtered_paper_ids();
    if ids.is_empty() {
        return;
    }
    let current = lib_state.read().single_selected_id().cloned();
    let next = match current {
        Some(ref cid) => {
            let pos = ids.iter().position(|id| id == cid).unwrap_or(0);
            if pos + 1 < ids.len() { pos + 1 } else { pos }
        }
        None => 0,
    };
    lib_state.with_mut(|s| s.select_one(ids[next].clone()));
}

fn action_select_prev(mut lib_state: Signal<LibraryState>) {
    let ids = lib_state.read().filtered_paper_ids();
    if ids.is_empty() {
        return;
    }
    let current = lib_state.read().single_selected_id().cloned();
    let prev = match current {
        Some(ref cid) => {
            let pos = ids.iter().position(|id| id == cid).unwrap_or(0);
            if pos > 0 { pos - 1 } else { 0 }
        }
        None => 0,
    };
    lib_state.with_mut(|s| s.select_one(ids[prev].clone()));
}

fn action_select_all(mut lib_state: Signal<LibraryState>) {
    let ids = lib_state.read().filtered_paper_ids();
    lib_state.with_mut(|s| s.select_all(ids.into_iter()));
}

fn action_open_selected_pdf(
    mut lib_state: Signal<LibraryState>,
    mut tabs: Signal<PdfTabManager>,
    db: &Database,
    config: &Signal<SyncConfig>,
    dpr_sig: &Signal<DevicePixelRatio>,
) {
    let state = lib_state.read();
    if state.selection_count() != 1 {
        return;
    }
    let paper = state.selected_paper().cloned();
    drop(state);
    if let Some(paper) = paper
        && let Some(ref rel_path) = paper.links.pdf_path
    {
        let pid = paper.id.clone().unwrap_or_default();
        crate::state::commands::open_paper_pdf(
            db,
            &mut tabs,
            &mut lib_state,
            config,
            dpr_sig,
            &pid,
            rel_path,
            &paper.title,
        );
    }
}

fn action_delete_selected(mut lib_state: Signal<LibraryState>) {
    let ids: Vec<String> = lib_state
        .read()
        .selected_paper_ids
        .iter()
        .cloned()
        .collect();
    if !ids.is_empty() {
        lib_state.with_mut(|s| s.confirm_delete = Some(ids));
    }
}

fn action_toggle_favorite_selected(mut lib_state: Signal<LibraryState>, db: Database) {
    let ids: Vec<String> = lib_state
        .read()
        .selected_paper_ids
        .iter()
        .cloned()
        .collect();
    if ids.is_empty() {
        return;
    }
    // For single: toggle. For multi: set all to favorite.
    let new_val = if ids.len() == 1 {
        !lib_state
            .read()
            .papers
            .iter()
            .find(|p| p.id.as_deref() == Some(ids[0].as_str()))
            .map(|p| p.status.is_favorite)
            .unwrap_or(false)
    } else {
        true
    };
    spawn(async move {
        for pid in &ids {
            let _ = rotero_db::papers::set_favorite(db.conn(), pid, new_val).await;
        }
        lib_state.with_mut(|s| {
            for pid in &ids {
                if let Some(p) = s
                    .papers
                    .iter_mut()
                    .find(|p| p.id.as_deref() == Some(pid.as_str()))
                {
                    p.status.is_favorite = new_val;
                }
            }
        });
    });
}

fn action_toggle_read_selected(mut lib_state: Signal<LibraryState>, db: Database) {
    let ids: Vec<String> = lib_state
        .read()
        .selected_paper_ids
        .iter()
        .cloned()
        .collect();
    if ids.is_empty() {
        return;
    }
    let new_val = if ids.len() == 1 {
        !lib_state
            .read()
            .papers
            .iter()
            .find(|p| p.id.as_deref() == Some(ids[0].as_str()))
            .map(|p| p.status.is_read)
            .unwrap_or(false)
    } else {
        true
    };
    spawn(async move {
        for pid in &ids {
            let _ = rotero_db::papers::set_read(db.conn(), pid, new_val).await;
        }
        lib_state.with_mut(|s| {
            for pid in &ids {
                if let Some(p) = s
                    .papers
                    .iter_mut()
                    .find(|p| p.id.as_deref() == Some(pid.as_str()))
                {
                    p.status.is_read = new_val;
                }
            }
        });
    });
}

/// All the state a command might need to run. Bundling it lets `dispatch` have a
/// single signature and lets both entry points (DOM + menu) share one code path.
#[derive(Clone, Copy)]
pub struct KeyCtx {
    pub show_settings: Signal<ShowSettings>,
    pub lib_state: Signal<LibraryState>,
    pub tabs: Signal<PdfTabManager>,
    pub render_ch: RenderChannel,
    pub config: Signal<SyncConfig>,
    pub new_coll_editing: Signal<Option<Option<String>>>,
    pub undo_stack: Signal<UndoStack>,
    pub tools: Signal<ViewerToolState>,
    pub dpr_sig: Signal<DevicePixelRatio>,
    pub update_state: Signal<UpdateState>,
}

impl KeyCtx {
    /// Gather every signal from context. Must be called inside a component (it
    /// uses `use_context`). Returns the `Database` alongside because it isn't
    /// `Copy` and so can't live in the `Copy` `KeyCtx`.
    pub fn from_context() -> (Self, Database) {
        (
            Self {
                show_settings: use_context::<Signal<ShowSettings>>(),
                lib_state: use_context::<Signal<LibraryState>>(),
                tabs: use_context::<Signal<PdfTabManager>>(),
                render_ch: use_context::<RenderChannel>(),
                config: use_context::<Signal<SyncConfig>>(),
                new_coll_editing: use_context::<Signal<Option<Option<String>>>>(),
                undo_stack: use_context::<Signal<UndoStack>>(),
                tools: use_context::<Signal<ViewerToolState>>(),
                dpr_sig: use_context::<Signal<DevicePixelRatio>>(),
                update_state: use_context::<Signal<UpdateState>>(),
            },
            use_context::<Database>(),
        )
    }

    fn scope(&self) -> Scope {
        match self.lib_state.read().view {
            LibraryView::PdfViewer => Scope::Viewer,
            LibraryView::Graph => Scope::Global,
            _ => Scope::Library,
        }
    }
}

/// Run a resolved command. This is the one place a `Command` maps to behavior;
/// the existing `action_*` functions do the work.
fn dispatch(cmd: Command, ctx: &KeyCtx, db: &Database) {
    let KeyCtx {
        show_settings,
        lib_state,
        tabs,
        render_ch,
        config,
        new_coll_editing,
        undo_stack,
        tools,
        dpr_sig,
        update_state,
    } = *ctx;
    match cmd {
        Command::OpenSettings => action_open_settings(show_settings),
        Command::OpenPdf => action_open_pdf(tabs, lib_state, config, dpr_sig),
        Command::ImportBibtex => action_import_bibtex(db.clone(), lib_state),
        Command::ExportBibtex => action_export_bibtex(lib_state),
        Command::Find => action_find(lib_state, tabs),
        Command::FocusLibrarySearch => action_focus_library_search(lib_state),
        Command::CloseTab => action_close_tab(tabs, lib_state, render_ch, config, dpr_sig),
        Command::NewCollection => action_new_collection(lib_state, new_coll_editing),
        Command::ShowLibrary => action_show_library(lib_state),
        Command::PrevTab => action_prev_tab(tabs, lib_state),
        Command::NextTab => action_next_tab(tabs, lib_state),
        Command::Undo => action_undo(db.clone(), tabs, undo_stack),
        Command::Redo => action_redo(db.clone(), tabs, undo_stack),
        Command::SelectAll => action_select_all(lib_state),
        Command::ToggleFavorite => action_toggle_favorite_selected(lib_state, db.clone()),
        Command::ToggleRead => action_toggle_read_selected(lib_state, db.clone()),
        Command::CheckUpdates => action_check_updates(update_state),
        Command::SelectNext => action_select_next(lib_state),
        Command::SelectPrev => action_select_prev(lib_state),
        Command::OpenSelected => {
            action_open_selected_pdf(lib_state, tabs, db, &config, &dpr_sig)
        }
        Command::DeleteSelected => action_delete_selected(lib_state),
        Command::Escape => action_escape(show_settings, tabs, tools, lib_state),
    }
}

/// Handles keyboard shortcuts (window-scoped via onkeydown) and native menu
/// events. Both resolve to a `Command` and go through `dispatch`.
#[component]
pub fn GlobalKeyHandler() -> Element {
    let (ctx, db) = KeyCtx::from_context();

    let _ = use_muda_event_handler(move |event| {
        if let Some(cmd) = command_for_menu(event.id().0.as_str()) {
            dispatch(cmd, &ctx, &db);
        } else if event.id().0 == "check-updates" {
            // Not a keyboard shortcut, so it has no BINDINGS row.
            dispatch(Command::CheckUpdates, &ctx, &db);
        }
    });

    rsx! {}
}

/// Keyboard event handler called from Layout's root div onkeydown. Resolves the
/// key within the current scope to a command, then dispatches.
pub fn handle_keydown(event: Event<KeyboardData>, ctx: KeyCtx, db: Database) {
    let key = event.key();
    let modifiers = event.modifiers();
    let cmd = modifiers.meta() || modifiers.ctrl();
    let shift = modifiers.shift();

    let overrides = ctx.config.read().keybindings.clone();
    if let Some(command) = resolve(cmd, shift, &key, ctx.scope(), &overrides) {
        // Escape is intentionally not prevent_default'd — matches prior behavior.
        if command != Command::Escape {
            event.prevent_default();
        }
        dispatch(command, &ctx, &db);
    }
}
