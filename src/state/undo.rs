use dioxus::prelude::*;
use rotero_models::Annotation;

use crate::db::Database;
use crate::state::app_state::PdfTabManager;

/// A forward annotation action (what was done).
#[derive(Debug, Clone)]
pub enum UndoAction {
    Create(Annotation),
    Delete(Annotation),
    UpdateContent { id: i64, old: Option<String>, new: Option<String> },
    UpdateColor { id: i64, old: String, new: String },
}

#[derive(Debug, Clone, Default)]
pub struct UndoStack {
    undo: Vec<UndoAction>,
    redo: Vec<UndoAction>,
}

impl UndoStack {
    /// Record a new action. Clears the redo stack.
    pub fn push(&mut self, action: UndoAction) {
        self.undo.push(action);
        self.redo.clear();
    }

    /// Pop the last action to undo. Caller must reverse it, then we move it to redo.
    pub fn pop_undo(&mut self) -> Option<UndoAction> {
        let action = self.undo.pop()?;
        self.redo.push(action.clone());
        Some(action)
    }

    /// Pop the last undone action to redo. Caller must re-apply it, then we move it to undo.
    pub fn pop_redo(&mut self) -> Option<UndoAction> {
        let action = self.redo.pop()?;
        self.undo.push(action.clone());
        Some(action)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// After a re-insert gives us a new DB id, patch the annotation id
    /// in the last entry of the given stack so future undo/redo uses the correct id.
    fn patch_last_ann_id(stack: &mut Vec<UndoAction>, new_id: i64) {
        if let Some(action) = stack.last_mut() {
            match action {
                UndoAction::Create(ann) | UndoAction::Delete(ann) => {
                    ann.id = Some(new_id);
                }
                _ => {}
            }
        }
    }

    /// After undo re-inserts (reverse of Delete), patch the redo stack entry.
    pub fn patch_last_redo_id(&mut self, new_id: i64) {
        Self::patch_last_ann_id(&mut self.redo, new_id);
    }

    /// After redo re-inserts (forward of Create), patch the undo stack entry.
    pub fn patch_last_undo_id(&mut self, new_id: i64) {
        Self::patch_last_ann_id(&mut self.undo, new_id);
    }
}

/// Reverse an action (for undo).
pub async fn reverse_action(
    db: Database,
    tabs: &mut Signal<PdfTabManager>,
    undo_stack: &mut Signal<UndoStack>,
    action: UndoAction,
) {
    match action {
        UndoAction::Create(ref ann) => {
            let ann_id = ann.id.unwrap_or(0);
            if let Ok(()) = crate::db::annotations::delete_annotation(db.conn(), ann_id).await {
                tabs.with_mut(|m| {
                    if let Some(t) = m.active_tab_mut() {
                        t.annotations.retain(|a| a.id != Some(ann_id));
                    }
                });
            }
        }
        UndoAction::Delete(ref ann) => {
            if let Ok(id) = crate::db::annotations::insert_annotation(db.conn(), ann).await {
                let mut ann = ann.clone();
                ann.id = Some(id);
                // Patch the redo stack so future redo uses the new id
                undo_stack.with_mut(|s| s.patch_last_redo_id(id));
                tabs.with_mut(|m| {
                    if let Some(t) = m.active_tab_mut() {
                        t.annotations.push(ann);
                    }
                });
            }
        }
        UndoAction::UpdateContent { id, ref old, .. } => {
            let opt = old.as_deref();
            if let Ok(()) = crate::db::annotations::update_annotation_content(db.conn(), id, opt).await {
                tabs.with_mut(|m| {
                    if let Some(t) = m.active_tab_mut() {
                        if let Some(a) = t.annotations.iter_mut().find(|a| a.id == Some(id)) {
                            a.content = old.clone();
                        }
                    }
                });
            }
        }
        UndoAction::UpdateColor { id, ref old, .. } => {
            if let Ok(()) = crate::db::annotations::update_annotation_color(db.conn(), id, old).await {
                tabs.with_mut(|m| {
                    if let Some(t) = m.active_tab_mut() {
                        if let Some(a) = t.annotations.iter_mut().find(|a| a.id == Some(id)) {
                            a.color = old.clone();
                        }
                    }
                });
            }
        }
    }
}

/// Re-apply an action (for redo).
pub async fn forward_action(
    db: Database,
    tabs: &mut Signal<PdfTabManager>,
    undo_stack: &mut Signal<UndoStack>,
    action: UndoAction,
) {
    match action {
        UndoAction::Create(ref ann) => {
            if let Ok(id) = crate::db::annotations::insert_annotation(db.conn(), ann).await {
                let mut ann = ann.clone();
                ann.id = Some(id);
                // Patch the undo stack so future undo uses the new id
                undo_stack.with_mut(|s| s.patch_last_undo_id(id));
                tabs.with_mut(|m| {
                    if let Some(t) = m.active_tab_mut() {
                        t.annotations.push(ann);
                    }
                });
            }
        }
        UndoAction::Delete(ref ann) => {
            let ann_id = ann.id.unwrap_or(0);
            if let Ok(()) = crate::db::annotations::delete_annotation(db.conn(), ann_id).await {
                tabs.with_mut(|m| {
                    if let Some(t) = m.active_tab_mut() {
                        t.annotations.retain(|a| a.id != Some(ann_id));
                    }
                });
            }
        }
        UndoAction::UpdateContent { id, ref new, .. } => {
            let opt = new.as_deref();
            if let Ok(()) = crate::db::annotations::update_annotation_content(db.conn(), id, opt).await {
                tabs.with_mut(|m| {
                    if let Some(t) = m.active_tab_mut() {
                        if let Some(a) = t.annotations.iter_mut().find(|a| a.id == Some(id)) {
                            a.content = new.clone();
                        }
                    }
                });
            }
        }
        UndoAction::UpdateColor { id, ref new, .. } => {
            if let Ok(()) = crate::db::annotations::update_annotation_color(db.conn(), id, new).await {
                tabs.with_mut(|m| {
                    if let Some(t) = m.active_tab_mut() {
                        if let Some(a) = t.annotations.iter_mut().find(|a| a.id == Some(id)) {
                            a.color = new.clone();
                        }
                    }
                });
            }
        }
    }
}
