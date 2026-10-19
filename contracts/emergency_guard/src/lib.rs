#![no_std]
//! # EmergencyGuard — standardized emergency controls for Soroban contracts
//!
//! Standalone circuit-breaker contract ([CONTRACT-5](#499)).
//!
//! All of the reusable logic — granular pause bitmask, multi-signature admin
//! committee, threshold validation, typed events — now lives in the shared
//! [`Perigee-guards`] library crate as [`Guard`], which is re-exported here as
//! [`DefaultEmergencyGuard`]. This crate only:
//!
//! 1. exposes the `EmergencyGuard` contract entry points (a thin delegation
//!    layer over the core), and
//! 2. re-exports the guard core, so host contracts that already embed guard
//!    storage via `emergency_guard` keep compiling and behaving identically.
//!
//! Storage layout, event topics, error codes, and public entry points are
//! unchanged; no on-chain state migration is required.

#[cfg(feature = "contract")]
use soroban_sdk::{contract, contractimpl};
use soroban_sdk::{Address, Env, Vec};

/// Re-export the reusable guard core and all of its public types, events and
/// emitters so existing imports (`emergency_guard::{DefaultEmergencyGuard,
/// PauseType, GuardError, …}`) keep resolving unchanged.
pub use Perigee_guards::*;
/// Backward-compatible name for the canonical [`Guard`] core.
pub use Perigee_guards::Guard as DefaultEmergencyGuard;
/// Granular pause types using bitmask for efficient storage
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PauseType(u32);

impl PauseType {
    pub const SWAP: u32 = 1 << 0;
    pub const DEPOSIT: u32 = 1 << 1;
    pub const WITHDRAW: u32 = 1 << 2;
    pub const TRANSFER: u32 = 1 << 3;
    pub const MINT: u32 = 1 << 4;
    pub const BURN: u32 = 1 << 5;
    pub const CREATE_PAIR: u32 = 1 << 6;
    /// Pause staking operations
    pub const STAKE: u32 = 1 << 7;
    /// Pause reward claims on the staking rewards contract.
    pub const CLAIM_REWARDS: u32 = 1 << 8;
    /// Pause metadata URI writes and cache invalidation.
    pub const METADATA: u32 = 1 << 9;

    pub fn new(value: u32) -> Self {
        PauseType(value)
    }

    /// Returns true if `operation` bit is set in the pause bitmask.
    /// `#[inline(always)]` ensures this reduces to a single AND + comparison
    /// instruction at the call site, minimising gas on every guard check.
    #[inline(always)]
    pub fn is_paused(&self, operation: u32) -> bool {
        (self.0 & operation) != 0
    }

    #[inline(always)]
    pub fn set_paused(&mut self, operation: u32, paused: bool) {
        if paused {
            self.0 |= operation;
        } else {
            self.0 &= !operation;
        }
    }

    pub fn pause_all(&mut self) {
        self.0 = u32::MAX;
    }

    pub fn unpause_all(&mut self) {
        self.0 = 0;
    }

    pub fn as_u32(self) -> u32 {
        self.0
    }
}

/// Data keys for emergency guard storage
#[contracttype]
pub enum GuardDataKey {
    PauseState,
    Admins,
    SignatureThreshold,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
#[repr(u32)]
pub enum GuardError {
    NotInitialized = 0,
    Unauthorized = 1,
    Paused = 2,
    InsufficientSignatures = 3,
    InvalidThreshold = 4,
    AdminNotFound = 5,
    AlreadyInitialized = 6,
}

/// Standardized event actions emitted by every successful guard action.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum EmergencyGuardAction {
    Initialized,
    PauseSet,
    EmergencyPause,
    Resume,
    AdminAdded,
    AdminRemoved,
    AdminRotated,
}

/// Standardized event payload for EmergencyGuard administrative actions.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmergencyGuardEvent {
    pub action: EmergencyGuardAction,
    pub admin: Option<Address>,
    pub operation: u32,
    pub paused: bool,
    pub threshold: u32,
    pub admin_count: u32,
    pub approver_count: u32,
}

fn action_topic(env: &Env, action: EmergencyGuardAction) -> String {
    match action {
        EmergencyGuardAction::Initialized => String::from_str(env, "initialized"),
        EmergencyGuardAction::PauseSet => String::from_str(env, "pause_set"),
        EmergencyGuardAction::EmergencyPause => String::from_str(env, "emergency_pause"),
        EmergencyGuardAction::Resume => String::from_str(env, "resume"),
        EmergencyGuardAction::AdminAdded => String::from_str(env, "admin_added"),
        EmergencyGuardAction::AdminRemoved => String::from_str(env, "admin_removed"),
        EmergencyGuardAction::AdminRotated => String::from_str(env, "admin_rotated"),
    }
}

fn emit_guard_event(env: &Env, event: EmergencyGuardEvent) {
    env.events().publish(
        (
            String::from_str(env, "EmergencyGuard"),
            action_topic(env, event.action),
        ),
        event,
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuardInitializedEvent {
    pub admins: Vec<Address>,
    pub threshold: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PauseStateChangedEvent {
    pub admin: Address,
    pub operation: u32,
    pub paused: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmergencyPausedEvent {
    pub approvers: Vec<Address>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResumedEvent {
    pub approvers: Vec<Address>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminAddedEvent {
    pub approvers: Vec<Address>,
    pub new_admin: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminRemovedEvent {
    pub approvers: Vec<Address>,
    pub admin: Address,
}

const EVENT_INIT_GUARD: &str = "emergency_guard_initialized";
const EVENT_SET_PAUSE: &str = "emergency_guard_pause_state_changed";
const EVENT_EMERGENCY_PAUSE_ALL: &str = "emergency_guard_emergency_paused_all";
const EVENT_RESUME_ALL: &str = "emergency_guard_resumed_all";
const EVENT_ADD_ADMIN: &str = "emergency_guard_admin_added";
const EVENT_REMOVE_ADMIN: &str = "emergency_guard_admin_removed";

pub fn emit_guard_initialized(e: &Env, admins: &Vec<Address>, threshold: u32) {
    e.events().publish(
        (String::from_str(e, EVENT_INIT_GUARD),),
        GuardInitializedEvent {
            admins: admins.clone(),
            threshold,
        },
    );
}

pub fn emit_pause_state_changed(e: &Env, admin: &Address, operation: u32, paused: bool) {
    e.events().publish(
        (String::from_str(e, EVENT_SET_PAUSE), admin.clone()),
        PauseStateChangedEvent {
            admin: admin.clone(),
            operation,
            paused,
        },
    );
}

pub fn emit_emergency_paused_all(e: &Env, approvers: &Vec<Address>) {
    e.events().publish(
        (String::from_str(e, EVENT_EMERGENCY_PAUSE_ALL),),
        EmergencyPausedEvent {
            approvers: approvers.clone(),
        },
    );
}

pub fn emit_resumed_all(e: &Env, approvers: &Vec<Address>) {
    e.events().publish(
        (String::from_str(e, EVENT_RESUME_ALL),),
        ResumedEvent {
            approvers: approvers.clone(),
        },
    );
}

pub fn emit_admin_added(e: &Env, approvers: &Vec<Address>, new_admin: &Address) {
    e.events().publish(
        (String::from_str(e, EVENT_ADD_ADMIN), new_admin.clone()),
        AdminAddedEvent {
            approvers: approvers.clone(),
            new_admin: new_admin.clone(),
        },
    );
}

pub fn emit_admin_removed(e: &Env, approvers: &Vec<Address>, admin: &Address) {
    e.events().publish(
        (String::from_str(e, EVENT_REMOVE_ADMIN), admin.clone()),
        AdminRemovedEvent {
            approvers: approvers.clone(),
            admin: admin.clone(),
        },
    );
}

#[cfg_attr(feature = "contract", contract)]
pub struct EmergencyGuard;

#[cfg_attr(feature = "contract", contractimpl)]
impl EmergencyGuard {
    /// Initialize the emergency guard with a list of admins and required threshold.
    pub fn initialize(env: Env, admins: Vec<Address>, threshold: u32) -> Result<(), GuardError> {
        DefaultEmergencyGuard::init_guard(&env, admins, threshold)
    }

    /// Returns the raw pause-state bitmask.
    pub fn get_pause_state(env: Env) -> u32 {
        DefaultEmergencyGuard::get_pause_state(&env)
    }

    /// Check if an operation is paused.
    pub fn is_paused(env: Env, operation: u32) -> bool {
        DefaultEmergencyGuard::is_paused(&env, operation)
    }

    /// Gas-optimized pause probe: single storage read + inline bitwise AND.
    #[inline(always)]
    pub fn is_paused_ref(env: &Env, operation: u32) -> bool {
        DefaultEmergencyGuard::is_paused(env, operation)
    }

    /// Panics when the requested operation bit is set in the pause bitmask.
    #[inline(always)]
    pub fn ensure_not_paused(env: &Env, operation: u32) {
        DefaultEmergencyGuard::ensure_not_paused(env, operation);
    }

    /// Set pause state for a specific operation (any single admin can do this).
    pub fn set_pause(
        env: Env,
        admin: Address,
        operation: u32,
        paused: bool,
    ) -> Result<(), GuardError> {
        DefaultEmergencyGuard::set_pause_by_admin(&env, &admin, operation, paused)
    }

    /// Emergency pause all operations (requires multi-sig approval).
    pub fn emergency_pause(env: Env, approvers: Vec<Address>) -> Result<(), GuardError> {
        DefaultEmergencyGuard::emergency_pause_all(&env, approvers)
    }

    /// Resume all operations (requires multi-sig approval).
    pub fn resume(env: Env, approvers: Vec<Address>) -> Result<(), GuardError> {
        DefaultEmergencyGuard::resume_all(&env, approvers)
    }

    /// Check if an address is an admin.
    pub fn is_admin_public(env: Env, addr: Address) -> bool {
        DefaultEmergencyGuard::is_admin(&env, &addr)
    }

    /// Add new admin (multi-sig required).
    pub fn add_admin(
        env: Env,
        approvers: Vec<Address>,
        new_admin: Address,
    ) -> Result<(), GuardError> {
        DefaultEmergencyGuard::add_admin(&env, approvers, new_admin)
    }

    /// Remove admin (multi-sig required).
    pub fn remove_admin(
        env: Env,
        approvers: Vec<Address>,
        admin: Address,
    ) -> Result<(), GuardError> {
        DefaultEmergencyGuard::remove_admin(&env, approvers, admin)
    }

    /// Rotate admin (multi-sig required).
    pub fn rotate_admin(
        env: Env,
        approvers: Vec<Address>,
        old_admin: Address,
        new_admin: Address,
    ) -> Result<(), GuardError> {
        DefaultEmergencyGuard::rotate_admin(&env, approvers, old_admin, new_admin)
    }

    /// Get list of current admins.
    pub fn get_admins(env: Env) -> Vec<Address> {
        DefaultEmergencyGuard::get_admins(&env)
    }

    /// Get required signature threshold.
    pub fn get_threshold(env: Env) -> u32 {
        DefaultEmergencyGuard::get_threshold(&env)
    }

    /// Public wrapper to validate approvers against the stored threshold.
    pub fn validate_multi_sig(env: Env, approvers: Vec<Address>) -> Result<(), GuardError> {
        DefaultEmergencyGuard::validate_multi_sig(&env, &approvers)
    }
}

#[cfg(test)]
mod test;

#[cfg(test)]
mod spec;

impl DefaultEmergencyGuard {
    pub fn check_not_paused(env: &Env, operation: u32) -> Result<(), GuardError> {
        if EmergencyGuard::is_paused(env.clone(), operation) {
            Err(GuardError::Paused)
        } else {
            Ok(())
        }
    }
    pub fn get_pause_state(env: &Env) -> u32 {
        EmergencyGuard::get_pause_state(env.clone())
    }
    pub fn set_pause_state(env: &Env, operation: u32, paused: bool) -> Result<(), GuardError> {
        let admins = EmergencyGuard::get_admins(env.clone());
        if let Some(admin) = admins.get(0) {
            EmergencyGuard::set_pause(env.clone(), admin, operation, paused)
        } else {
            Err(GuardError::Unauthorized)
        }
    }
    pub fn unpause(env: &Env, operation: u32) -> Result<(), GuardError> {
        let admins = EmergencyGuard::get_admins(env.clone());
        if let Some(admin) = admins.get(0) {
            EmergencyGuard::set_pause(env.clone(), admin, operation, false)
        } else {
            Err(GuardError::Unauthorized)
        }
    }
    pub fn unpause_all(env: &Env) -> Result<(), GuardError> {
        let admins = EmergencyGuard::get_admins(env.clone());
        if let Some(admin) = admins.get(0) {
            EmergencyGuard::set_pause(env.clone(), admin, u32::MAX, false)
        } else {
            Err(GuardError::Unauthorized)
        }
    }
    pub fn emergency_pause_all(env: &Env, approvers: Vec<Address>) -> Result<(), GuardError> {
        EmergencyGuard::emergency_pause(env.clone(), approvers)
    }
    pub fn resume_all(env: &Env, approvers: Vec<Address>) -> Result<(), GuardError> {
        EmergencyGuard::resume(env.clone(), approvers)
    }
    pub fn init_guard(env: &Env, admins: Vec<Address>, threshold: u32) -> Result<(), GuardError> {
        EmergencyGuard::initialize(env.clone(), admins, threshold)
    }
    pub fn add_admin(
        env: &Env,
        approvers: Vec<Address>,
        new_admin: Address,
    ) -> Result<(), GuardError> {
        EmergencyGuard::add_admin(env.clone(), approvers, new_admin)
    }
    pub fn remove_admin(
        env: &Env,
        approvers: Vec<Address>,
        admin: Address,
    ) -> Result<(), GuardError> {
        EmergencyGuard::remove_admin(env.clone(), approvers, admin)
    }
    pub fn rotate_admin(
        env: &Env,
        approvers: Vec<Address>,
        old_admin: Address,
        new_admin: Address,
    ) -> Result<(), GuardError> {
        EmergencyGuard::rotate_admin(env.clone(), approvers, old_admin, new_admin)
    }
    pub fn get_admins(env: &Env) -> Vec<Address> {
        EmergencyGuard::get_admins(env.clone())
    }
    pub fn get_threshold(env: &Env) -> u32 {
        EmergencyGuard::get_threshold(env.clone())
    }
    pub fn is_admin(env: &Env, addr: Address) -> bool {
        EmergencyGuard::is_admin_public(env.clone(), addr)
    }
}
mod test;
