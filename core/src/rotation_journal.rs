#![allow(dead_code)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationJournal {
    pub vault_id: String,
    pub from_agent: String,
    pub to_agent: String,
    pub step: RotationStep,
    pub completed_steps: Vec<RotationStep>,
    pub status: RotationStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RotationStatus {
    Pending,
    InProgress,
    Completed,
    RolledBack,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RotationStep {
    PolicyRevoke,
    PolicyGrant,
    VaultReassign,
    StateSnapshot,
    Confirm,
}

const ALL_STEPS: &[RotationStep] = &[
    RotationStep::PolicyRevoke,
    RotationStep::PolicyGrant,
    RotationStep::VaultReassign,
    RotationStep::StateSnapshot,
    RotationStep::Confirm,
];

impl RotationJournal {
    pub fn new(vault_id: String, from: String, to: String) -> Self {
        let now = Utc::now();
        Self {
            vault_id,
            from_agent: from,
            to_agent: to,
            step: RotationStep::PolicyRevoke,
            completed_steps: Vec::new(),
            status: RotationStatus::Pending,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn can_resume(&self) -> bool {
        self.status == RotationStatus::InProgress || self.status == RotationStatus::Pending
    }

    pub fn next_step(&self) -> Option<RotationStep> {
        if self.is_complete() {
            return None;
        }
        Some(ALL_STEPS[self.completed_steps.len()].clone())
    }

    pub fn complete_step(&mut self, step: RotationStep) {
        if self.next_step() == Some(step.clone()) {
            self.completed_steps.push(step.clone());
            self.status = RotationStatus::InProgress;
            self.updated_at = Utc::now();
            if let Some(idx) = ALL_STEPS.iter().position(|s| s == &step) {
                let next_idx = idx + 1;
                if next_idx < ALL_STEPS.len() {
                    self.step = ALL_STEPS[next_idx].clone();
                }
            }
            if self.is_complete() {
                self.status = RotationStatus::Completed;
            }
        }
    }

    pub fn rollback(&mut self) {
        self.completed_steps.clear();
        self.step = RotationStep::PolicyRevoke;
        self.status = RotationStatus::RolledBack;
        self.updated_at = Utc::now();
    }

    pub fn is_complete(&self) -> bool {
        self.completed_steps.len() >= ALL_STEPS.len()
    }

    pub fn progress_pct(&self) -> f64 {
        (self.completed_steps.len() as f64 / ALL_STEPS.len() as f64) * 100.0
    }
}

pub struct RotationJournalStore {
    journals: Vec<RotationJournal>,
}

impl Default for RotationJournalStore {
    fn default() -> Self {
        Self::new()
    }
}

impl RotationJournalStore {
    pub fn new() -> Self {
        Self {
            journals: Vec::new(),
        }
    }

    pub fn save(&mut self, journal: RotationJournal) {
        if let Some(existing) = self
            .journals
            .iter_mut()
            .find(|j| j.vault_id == journal.vault_id)
        {
            *existing = journal;
        } else {
            self.journals.push(journal);
        }
    }

    pub fn get_incomplete(&self) -> Vec<&RotationJournal> {
        self.journals
            .iter()
            .filter(|j| j.can_resume())
            .collect()
    }

    pub fn resume(&mut self, vault_id: &str) -> Option<&mut RotationJournal> {
        self.journals
            .iter_mut()
            .find(|j| j.vault_id == vault_id && j.can_resume())
    }

    pub fn recover_orphaned(&mut self) -> Vec<String> {
        let mut recovered = Vec::new();
        for journal in &mut self.journals {
            if journal.status == RotationStatus::InProgress && !journal.is_complete() {
                journal.rollback();
                recovered.push(journal.vault_id.clone());
            }
        }
        recovered
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_journal_lifecycle() {
        let mut journal =
            RotationJournal::new("vault1".to_string(), "agent_a".to_string(), "agent_b".to_string());
        assert_eq!(journal.progress_pct(), 0.0);
        assert_eq!(journal.status, RotationStatus::Pending);

        let steps = vec![
            RotationStep::PolicyRevoke,
            RotationStep::PolicyGrant,
            RotationStep::VaultReassign,
            RotationStep::StateSnapshot,
            RotationStep::Confirm,
        ];

        for step in &steps {
            assert_eq!(journal.next_step(), Some(step.clone()));
            journal.complete_step(step.clone());
        }

        assert!(journal.is_complete());
        assert!(journal.next_step().is_none());
        assert_eq!(journal.progress_pct(), 100.0);
        assert_eq!(journal.status, RotationStatus::Completed);
    }

    #[test]
    fn test_store_resume() {
        let mut store = RotationJournalStore::new();
        let journal =
            RotationJournal::new("vault1".to_string(), "a".to_string(), "b".to_string());
        store.save(journal);
        assert_eq!(store.get_incomplete().len(), 1);
        assert!(store.resume("vault1").is_some());
    }

    #[test]
    fn test_rollback_resets_status() {
        let mut journal =
            RotationJournal::new("vault1".to_string(), "a".to_string(), "b".to_string());
        journal.complete_step(RotationStep::PolicyRevoke);
        journal.complete_step(RotationStep::PolicyGrant);
        assert_eq!(journal.status, RotationStatus::InProgress);
        journal.rollback();
        assert_eq!(journal.status, RotationStatus::RolledBack);
        assert!(journal.completed_steps.is_empty());
        assert_eq!(journal.progress_pct(), 0.0);
    }

    #[test]
    fn test_recover_orphaned() {
        let mut store = RotationJournalStore::new();
        let mut j1 =
            RotationJournal::new("v1".to_string(), "a".to_string(), "b".to_string());
        j1.complete_step(RotationStep::PolicyRevoke);
        j1.status = RotationStatus::InProgress;
        store.save(j1);

        let mut j2 =
            RotationJournal::new("v2".to_string(), "c".to_string(), "d".to_string());
        j2.complete_step(RotationStep::PolicyRevoke);
        j2.complete_step(RotationStep::PolicyGrant);
        j2.complete_step(RotationStep::VaultReassign);
        j2.complete_step(RotationStep::StateSnapshot);
        j2.complete_step(RotationStep::Confirm);
        store.save(j2);

        let recovered = store.recover_orphaned();
        assert_eq!(recovered, vec!["v1".to_string()]);
    }
}
