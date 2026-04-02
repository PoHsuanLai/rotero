use rotero_models::Annotation;

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
}
