use crate::models::{ItemStatus, QueueItem};
use std::collections::VecDeque;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueError {
    ItemNotFound(usize),
    InvalidStateTransition {
        id: usize,
        from: ItemStatus,
        to: ItemStatus,
    },
}

impl std::fmt::Display for QueueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueueError::ItemNotFound(id) => write!(f, "Queue item #{} not found", id),
            QueueError::InvalidStateTransition { id, from, to } => {
                write!(
                    f,
                    "Invalid status change for #{}: {:?} -> {:?}",
                    id, from, to
                )
            }
        }
    }
}

impl std::error::Error for QueueError {}

#[derive(Debug, Default)]
pub struct QueueEngine {
    items: VecDeque<QueueItem>,
    next_id: usize,
}

impl QueueEngine {
    pub fn new() -> Self {
        Self {
            items: VecDeque::new(),
            next_id: 1,
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn enqueue(&mut self, prompt: String) -> usize {
        self.enqueue_with_attachments(prompt, None)
    }

    pub fn enqueue_with_attachments(
        &mut self,
        prompt: String,
        attachments_summary: Option<String>,
    ) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.items
            .push_back(QueueItem::with_attachments(id, prompt, attachments_summary));
        id
    }

    pub fn get(&self, id: usize) -> Option<&QueueItem> {
        self.items.iter().find(|i| i.id == id)
    }

    pub fn get_mut(&mut self, id: usize) -> Option<&mut QueueItem> {
        self.items.iter_mut().find(|i| i.id == id)
    }

    pub fn peek_next_pending(&self) -> Option<&QueueItem> {
        self.items.iter().find(|i| i.status == ItemStatus::Queued)
    }

    pub fn mark_submitting(&mut self, id: usize, timestamp: u64) -> Result<(), QueueError> {
        let item = self.get_mut(id).ok_or(QueueError::ItemNotFound(id))?;
        if item.status != ItemStatus::Queued {
            return Err(QueueError::InvalidStateTransition {
                id,
                from: item.status,
                to: ItemStatus::Submitting,
            });
        }
        item.mark_submitting(timestamp);
        Ok(())
    }

    pub fn mark_acknowledged(&mut self, id: usize) -> Result<(), QueueError> {
        let item = self.get_mut(id).ok_or(QueueError::ItemNotFound(id))?;
        if item.status != ItemStatus::Submitting && item.status != ItemStatus::Uncertain {
            return Err(QueueError::InvalidStateTransition {
                id,
                from: item.status,
                to: ItemStatus::Acknowledged,
            });
        }
        item.mark_acknowledged();
        Ok(())
    }

    pub fn mark_uncertain(&mut self, id: usize) -> Result<(), QueueError> {
        let item = self.get_mut(id).ok_or(QueueError::ItemNotFound(id))?;
        if item.status != ItemStatus::Submitting {
            return Err(QueueError::InvalidStateTransition {
                id,
                from: item.status,
                to: ItemStatus::Uncertain,
            });
        }
        item.mark_uncertain();
        Ok(())
    }

    pub fn update_prompt(
        &mut self,
        id: usize,
        new_prompt: String,
        new_attachments: Option<String>,
    ) -> Result<(), QueueError> {
        let item = self.get_mut(id).ok_or(QueueError::ItemNotFound(id))?;
        if item.status != ItemStatus::Queued {
            return Err(QueueError::InvalidStateTransition {
                id,
                from: item.status,
                to: ItemStatus::Queued,
            });
        }
        item.prompt = new_prompt;
        item.attachments_summary = new_attachments;
        Ok(())
    }

    pub fn last_queued_id(&self) -> Option<usize> {
        self.items
            .iter()
            .rev()
            .find(|i| i.status == ItemStatus::Queued)
            .map(|i| i.id)
    }

    pub fn queued_ids(&self) -> Vec<usize> {
        self.items
            .iter()
            .filter(|i| i.status == ItemStatus::Queued)
            .map(|i| i.id)
            .collect()
    }

    pub fn all_items(&self) -> &VecDeque<QueueItem> {
        &self.items
    }
}
