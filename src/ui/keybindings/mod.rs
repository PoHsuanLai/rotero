//! Keyboard-shortcut handling.
//!
//! The focus-tracking primitives below compile on every platform: they carry no
//! desktop dependency and are attached to text inputs throughout the UI. The
//! full shortcut engine (the `Command`/`BINDINGS` table, the DOM and native-menu
//! handlers, muda accelerators) lives in the desktop-only [`desktop`] submodule
//! and is re-exported here on desktop builds.

use dioxus::prelude::*;

#[cfg(feature = "desktop")]
mod desktop;
#[cfg(feature = "desktop")]
pub use desktop::*;

/// True while a text input/textarea holds focus. Provided via context at the app
/// root and read once per keystroke by the desktop `handle_keydown`. A newtype
/// (not a bare `Signal<bool>`) so it can't be confused with any other boolean
/// context.
#[derive(Clone, Copy)]
pub struct EditableFocused(pub Signal<bool>);

/// `onfocusin` handler for an editable input: flags that a text field is focused
/// so the global shortcut handler yields the keys native text editing owns. Attach
/// alongside [`editable_focus_out`] to every `<input>`/`<textarea>`:
///
/// ```ignore
/// rsx! { input { onfocusin: editable_focus_in, onfocusout: editable_focus_out, /* … */ } }
/// ```
///
/// These are plain `fn`s (not hooks), so they can be attached anywhere — including
/// inside a `for`/`if` in `rsx!` or a `.map()` over a list of inputs.
pub fn editable_focus_in(_: FocusEvent) {
    set_editable_focused(true);
}

/// `onfocusout` counterpart to [`editable_focus_in`]. Clears the focus flag.
pub fn editable_focus_out(_: FocusEvent) {
    set_editable_focused(false);
}

/// Set the "an editable element is focused" flag. Reads `EditableFocused` from
/// context lazily (via `try_consume_context`), so it works at event time from
/// anywhere a Dioxus runtime is active.
fn set_editable_focused(focused: bool) {
    if let Some(EditableFocused(mut sig)) = try_consume_context::<EditableFocused>() {
        sig.set(focused);
    }
}
