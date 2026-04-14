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
