use dioxus::prelude::*;

use crate::sync::engine::SyncConfig;
use crate::ui::helpers::save_config;
use crate::ui::keybindings::{
    Command, KeySpec, Scope, command_scope, conflict_for, default_key, effective_key,
    rebindable_commands,
};

/// A rebind that collided with an existing binding, awaiting user confirmation
/// to swap. `key` would move from `existing_owner` to `command`.
#[derive(Clone, Copy, PartialEq)]
struct PendingSwap {
    command: Command,
    key: KeySpec,
    existing_owner: Command,
}

/// The keybindings settings page: one row per rebindable command, each with a
/// clickable chip that captures a new combo, plus per-row and global reset.
#[component]
pub fn KeybindingsSection() -> Element {
    let mut config = use_context::<Signal<SyncConfig>>();

    // Which command (if any) is currently capturing a keypress. `None` = idle.
    let mut capturing = use_signal(|| None::<Command>);
    // Transient message shown after a capture (e.g. "that key can't be bound").
    let mut notice = use_signal(String::new);
    // A conflicting rebind awaiting the user's confirm-to-swap decision.
    let mut pending_swap = use_signal(|| None::<PendingSwap>);

    let overrides = config.read().keybindings.clone();
    let any_override = !overrides.is_empty();

    // Group commands by their scope for a tidy layout.
    let scopes = [Scope::Global, Scope::Library, Scope::Viewer];

    rsx! {
        div { class: "settings-section",
            div { class: "settings-section-header",
                h4 { class: "settings-section-title", "Keybindings" }
                if any_override {
                    button {
                        class: "btn btn--sm",
                        onclick: move |_| {
                            capturing.set(None);
                            notice.set(String::new());
                            save_config(&mut config, |c| c.keybindings.clear());
                        },
                        "Reset all"
                    }
                }
            }

            p { class: "settings-description",
                if capturing.read().is_some() {
                    "Press a key combination\u{2026} (Esc to cancel)"
                } else {
                    "Click a shortcut to change it."
                }
            }

            if !notice.read().is_empty() {
                p { class: "settings-keybind-notice", "{notice}" }
            }

            for scope in scopes {
                {
                    let overrides = overrides.clone();
                    let rows: Vec<Command> = rebindable_commands()
                        .filter(|c| command_scope(*c) == scope)
                        .collect();
                    if rows.is_empty() {
                        rsx! {}
                    } else {
                        rsx! {
                            div { class: "settings-keybind-group",
                                span { class: "settings-keybind-scope", "{scope.label()}" }
                                for command in rows {
                                    KeybindRow {
                                        command,
                                        overrides: overrides.clone(),
                                        capturing,
                                        notice,
                                        pending_swap,
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if let Some(swap) = *pending_swap.read() {
                SwapConfirm {
                    swap,
                    on_cancel: move |_| pending_swap.set(None),
                    on_confirm: move |_| {
                        // Assign the key to the new command and unbind the old
                        // owner. `None` marks it explicitly unbound so its default
                        // no longer resolves (see keybindings::effective_key).
                        let new_is_default = default_key(swap.command) == Some(swap.key);
                        save_config(&mut config, |c| {
                            if new_is_default {
                                c.keybindings.remove(swap.command.id());
                            } else {
                                c.keybindings.insert(swap.command.id().to_string(), Some(swap.key));
                            }
                            c.keybindings.insert(swap.existing_owner.id().to_string(), None);
                        });
                        notice.set(String::new());
                        pending_swap.set(None);
                    },
                }
            }
        }
    }
}

/// Modal warning shown when a rebind would take a key from another command.
#[component]
fn SwapConfirm(
    swap: PendingSwap,
    on_cancel: EventHandler<()>,
    on_confirm: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "settings-overlay settings-overlay--nested",
            onclick: move |_| on_cancel.call(()),
            div { class: "settings-confirm",
                onclick: move |evt| evt.stop_propagation(),
                h4 { class: "settings-confirm-title", "Reassign shortcut?" }
                p { class: "settings-confirm-body",
                    span { class: "settings-keybind-chip", "{swap.key.display()}" }
                    " is already used by \u{201c}{swap.existing_owner.label()}\u{201d}."
                }
                p { class: "settings-confirm-body",
                    "Reassign it to \u{201c}{swap.command.label()}\u{201d}? "
                    "\u{201c}{swap.existing_owner.label()}\u{201d} will have no shortcut."
                }
                div { class: "settings-confirm-actions",
                    button {
                        class: "btn btn--sm",
                        onclick: move |_| on_cancel.call(()),
                        "Cancel"
                    }
                    button {
                        class: "btn btn--sm btn--primary",
                        onclick: move |_| on_confirm.call(()),
                        "Reassign"
                    }
                }
            }
        }
    }
}

#[component]
fn KeybindRow(
    command: Command,
    overrides: crate::ui::keybindings::Overrides,
    capturing: Signal<Option<Command>>,
    notice: Signal<String>,
    pending_swap: Signal<Option<PendingSwap>>,
) -> Element {
    let mut config = use_context::<Signal<SyncConfig>>();
    let mut capturing = capturing;
    let mut notice = notice;
    let mut pending_swap = pending_swap;

    let is_capturing = *capturing.read() == Some(command);
    let current = effective_key(command, &overrides);
    let is_overridden = overrides.contains_key(command.id());

    let chip_label = if is_capturing {
        "Press keys\u{2026}".to_string()
    } else {
        current
            .map(KeySpec::display)
            .unwrap_or_else(|| "\u{2014}".to_string())
    };

    let chip_class = if is_capturing {
        "settings-keybind-chip settings-keybind-chip--capturing"
    } else {
        "settings-keybind-chip"
    };

    rsx! {
        div { class: "settings-keybind-row",
            span { class: "settings-keybind-label", "{command.label()}" }
            div { class: "settings-keybind-controls",
                // A focusable div (not a <button>) so it receives raw keydowns
                // for every key — a native button would hijack Enter/Space as an
                // activation and never deliver them as a keydown to capture.
                div {
                    id: "keybind-chip-{command.id()}",
                    class: "{chip_class}",
                    tabindex: "0",
                    role: "button",
                    // Capturing rows listen for the next keydown here. The div is
                    // focused (see onclick) so the event lands on it and is
                    // stopped from bubbling to the global keydown handler.
                    onkeydown: move |evt| {
                        if *capturing.read() != Some(command) {
                            return;
                        }
                        evt.stop_propagation();
                        evt.prevent_default();
                        let m = evt.modifiers();
                        let cmd = m.meta() || m.ctrl();
                        let shift = m.shift();
                        let key = evt.key();

                        // Escape cancels capture without binding.
                        if key == Key::Escape {
                            capturing.set(None);
                            return;
                        }
                        // Ignore lone modifier presses; wait for a real key.
                        if matches!(key, Key::Control | Key::Meta | Key::Shift | Key::Alt) {
                            return;
                        }
                        match KeySpec::from_event(cmd, shift, &key) {
                            Some(spec) => {
                                let overrides = config.read().keybindings.clone();
                                if let Some(other) = conflict_for(command, spec, &overrides) {
                                    // Don't overwrite silently — ask the user to
                                    // confirm swapping the key away from its owner.
                                    notice.set(String::new());
                                    pending_swap.set(Some(PendingSwap {
                                        command,
                                        key: spec,
                                        existing_owner: other,
                                    }));
                                } else {
                                    notice.set(String::new());
                                    // Store as override unless it equals the default,
                                    // in which case drop the override to keep config tidy.
                                    let is_default = default_key(command) == Some(spec);
                                    save_config(&mut config, |c| {
                                        if is_default {
                                            c.keybindings.remove(command.id());
                                        } else {
                                            c.keybindings.insert(command.id().to_string(), Some(spec));
                                        }
                                    });
                                }
                                capturing.set(None);
                            }
                            None => {
                                notice.set("That key can\u{2019}t be bound.".to_string());
                            }
                        }
                    },
                    onclick: move |_| {
                        notice.set(String::new());
                        if *capturing.read() == Some(command) {
                            capturing.set(None);
                        } else {
                            capturing.set(Some(command));
                            // Ensure the chip holds focus so the next keydown is
                            // captured here rather than by the global handler.
                            let id = command.id();
                            spawn(async move {
                                let _ = document::eval(&format!(
                                    "document.getElementById('keybind-chip-{id}')?.focus();"
                                ));
                            });
                        }
                    },
                    "{chip_label}"
                }
                if is_overridden {
                    button {
                        class: "btn--icon settings-keybind-reset",
                        title: "Reset to default",
                        onclick: move |_| {
                            notice.set(String::new());
                            capturing.set(None);
                            save_config(&mut config, |c| {
                                c.keybindings.remove(command.id());
                            });
                        },
                        "\u{27f2}"
                    }
                }
            }
        }
    }
}
