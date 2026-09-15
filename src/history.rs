//! Undoable editor drafts, separate from device and lighting runtime state.
use crate::config::Config;
use std::collections::VecDeque;

const MAX_SNAPSHOTS: usize = 65;

#[derive(Clone, Debug, PartialEq)]
pub struct EditorSnapshot {
    pub config: Config,
    pub address: String,
}

pub struct EditorHistory {
    snapshots: VecDeque<EditorSnapshot>,
    cursor: usize,
    active_group: Option<u64>,
}

impl EditorHistory {
    pub fn new(config: Config, address: String) -> Self {
        let mut snapshots = VecDeque::with_capacity(MAX_SNAPSHOTS);
        snapshots.push_back(EditorSnapshot { config, address });
        Self {
            snapshots,
            cursor: 0,
            active_group: None,
        }
    }

    /// Records a draft without validating it. Equal drafts leave redo intact.
    /// Consecutive changes in the same group replace the gesture's latest draft;
    /// `None`, a different group, or `finish_group` separates undo steps.
    pub fn record(&mut self, config: &Config, address: &str, group: Option<u64>) -> bool {
        if self.active_group != group {
            self.finish_group();
        }
        let current = &self.snapshots[self.cursor];
        if current.config == *config && current.address == address {
            return false;
        }

        self.snapshots.truncate(self.cursor + 1);
        if group.is_some() && self.active_group == group {
            let original = &self.snapshots[self.cursor - 1];
            if original.config == *config && original.address == address {
                // A gesture that returns to its start leaves no undo step.
                self.snapshots.pop_back();
                self.cursor -= 1;
                self.finish_group();
                return true;
            }
            self.snapshots[self.cursor] = EditorSnapshot {
                config: config.clone(),
                address: address.to_owned(),
            };
        } else {
            // Evict before pushing so a full timeline never grows its allocation.
            if self.snapshots.len() == MAX_SNAPSHOTS {
                self.snapshots.pop_front();
                self.cursor -= 1;
            }
            self.snapshots.push_back(EditorSnapshot {
                config: config.clone(),
                address: address.to_owned(),
            });
            self.cursor += 1;
        }
        self.active_group = group;
        true
    }

    pub fn finish_group(&mut self) {
        self.active_group = None;
    }

    pub fn undo(&mut self) -> Option<EditorSnapshot> {
        self.finish_group();
        if !self.can_undo() {
            return None;
        }
        self.cursor -= 1;
        Some(self.snapshots[self.cursor].clone())
    }

    pub fn redo(&mut self) -> Option<EditorSnapshot> {
        self.finish_group();
        if !self.can_redo() {
            return None;
        }
        self.cursor += 1;
        Some(self.snapshots[self.cursor].clone())
    }

    pub fn can_undo(&self) -> bool {
        self.cursor > 0
    }

    pub fn can_redo(&self) -> bool {
        self.cursor + 1 < self.snapshots.len()
    }
}
