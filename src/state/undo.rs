use dioxus::prelude::*;
use rotero_models::Annotation;

use crate::state::app_state::PdfTabManager;
use rotero_db::Database;

#[derive(Debug, Clone)]
pub enum UndoAction {
    Create(Annotation),
    Delete(Annotation),
    UpdateContent {
        id: String,
        old: Option<String>,
        new: Option<String>,
    },
    UpdateColor {
        id: String,
        old: String,
        new: String,
    },
}

#[derive(Debug, Clone, Default)]
pub struct UndoStack {
    undo: Vec<UndoAction>,
    redo: Vec<UndoAction>,
}

impl UndoStack {
    pub fn push(&mut self, action: UndoAction) {
        self.undo.push(action);
        self.redo.clear();
    }

    pub fn pop_undo(&mut self) -> Option<UndoAction> {
        let action = self.undo.pop()?;
        self.redo.push(action.clone());
        Some(action)
    }

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

    /// Re-insert gives a new DB id; patch it so future undo/redo uses the correct id.
    fn patch_last_ann_id(stack: &mut [UndoAction], new_id: String) {
        if let Some(UndoAction::Create(ann) | UndoAction::Delete(ann)) = stack.last_mut() {
            ann.id = Some(new_id);
        }
    }

    pub fn patch_last_redo_id(&mut self, new_id: String) {
        Self::patch_last_ann_id(&mut self.redo, new_id);
    }

    pub fn patch_last_undo_id(&mut self, new_id: String) {
        Self::patch_last_ann_id(&mut self.undo, new_id);
    }
}

pub async fn reverse_action(
    db: Database,
    tabs: &mut Signal<PdfTabManager>,
    undo_stack: &mut Signal<UndoStack>,
    action: UndoAction,
) {
    match action {
        UndoAction::Create(ref ann) => {
            let ann_id = ann.id.clone().unwrap_or_default();
            if let Ok(()) = rotero_db::annotations::delete_annotation(db.conn(), &ann_id).await {
                tabs.with_mut(|m| {
                    if let Some(t) = m.active_tab_mut() {
                        t.annotations
                            .retain(|a| a.id.as_deref() != Some(ann_id.as_str()));
                    }
                });
            }
        }
        UndoAction::Delete(ref ann) => {
            if let Ok(id) = rotero_db::annotations::insert_annotation(db.conn(), ann).await {
                let mut ann = ann.clone();
                ann.id = Some(id.clone());
                undo_stack.with_mut(|s| s.patch_last_redo_id(id));
                tabs.with_mut(|m| {
                    if let Some(t) = m.active_tab_mut() {
                        t.annotations.push(ann);
                    }
                });
            }
        }
        UndoAction::UpdateContent {
            ref id, ref old, ..
        } => {
            let opt = old.as_deref();
            if let Ok(()) =
                rotero_db::annotations::update_annotation_content(db.conn(), id, opt).await
            {
                let id = id.clone();
                tabs.with_mut(|m| {
                    if let Some(t) = m.active_tab_mut()
                        && let Some(a) = t
                            .annotations
                            .iter_mut()
                            .find(|a| a.id.as_deref() == Some(id.as_str()))
                    {
                        a.content = old.clone();
                    }
                });
            }
        }
        UndoAction::UpdateColor {
            ref id, ref old, ..
        } => {
            if let Ok(()) =
                rotero_db::annotations::update_annotation_color(db.conn(), id, old).await
            {
                let id = id.clone();
                tabs.with_mut(|m| {
                    if let Some(t) = m.active_tab_mut()
                        && let Some(a) = t
                            .annotations
                            .iter_mut()
                            .find(|a| a.id.as_deref() == Some(id.as_str()))
                    {
                        a.color = old.clone();
                    }
                });
            }
        }
    }
}

pub async fn forward_action(
    db: Database,
    tabs: &mut Signal<PdfTabManager>,
    undo_stack: &mut Signal<UndoStack>,
    action: UndoAction,
) {
    match action {
        UndoAction::Create(ref ann) => {
            if let Ok(id) = rotero_db::annotations::insert_annotation(db.conn(), ann).await {
                let mut ann = ann.clone();
                ann.id = Some(id.clone());
                undo_stack.with_mut(|s| s.patch_last_undo_id(id));
                tabs.with_mut(|m| {
                    if let Some(t) = m.active_tab_mut() {
                        t.annotations.push(ann);
                    }
                });
            }
        }
        UndoAction::Delete(ref ann) => {
            let ann_id = ann.id.clone().unwrap_or_default();
            if let Ok(()) = rotero_db::annotations::delete_annotation(db.conn(), &ann_id).await {
                tabs.with_mut(|m| {
                    if let Some(t) = m.active_tab_mut() {
                        t.annotations
                            .retain(|a| a.id.as_deref() != Some(ann_id.as_str()));
                    }
                });
            }
        }
        UndoAction::UpdateContent {
            ref id, ref new, ..
        } => {
            let opt = new.as_deref();
            if let Ok(()) =
                rotero_db::annotations::update_annotation_content(db.conn(), id, opt).await
            {
                let id = id.clone();
                tabs.with_mut(|m| {
                    if let Some(t) = m.active_tab_mut()
                        && let Some(a) = t
                            .annotations
                            .iter_mut()
                            .find(|a| a.id.as_deref() == Some(id.as_str()))
                    {
                        a.content = new.clone();
                    }
                });
            }
        }
        UndoAction::UpdateColor {
            ref id, ref new, ..
        } => {
            if let Ok(()) =
                rotero_db::annotations::update_annotation_color(db.conn(), id, new).await
            {
                let id = id.clone();
                tabs.with_mut(|m| {
                    if let Some(t) = m.active_tab_mut()
                        && let Some(a) = t
                            .annotations
                            .iter_mut()
                            .find(|a| a.id.as_deref() == Some(id.as_str()))
                    {
                        a.color = new.clone();
                    }
                });
            }
        }
    }
}
