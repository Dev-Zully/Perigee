use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, BytesN, Env, IntoVal, Symbol,
    Val, Vec,
};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ProxyError {
    NotInitialized = 0,
    UnverifiedImplementation = 1,
    SelfUpgrade = 2,
    AlreadyVerified = 3,
    NotVerified = 4,
}

#[contracttype]
pub enum DataKey {
    Admin,
    Implementation,
    Counter,
    Storage(BytesN<32>),
    VerifiedImplementations,
}

#[contract]
pub struct Proxy;

#[contractimpl]
impl Proxy {
    pub fn initialize(env: Env, admin: Address, implementation: Address) {
        if env.storage().persistent().has(&DataKey::Admin) {
            panic!("Proxy already initialized");
        }

        env.storage().persistent().set(&DataKey::Admin, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::Implementation, &implementation);
        env.storage().persistent().set(&DataKey::Counter, &0i32);
        // The initial implementation is trusted by construction.
        let initial: Vec<Address> = Vec::from_array(&env, [implementation.clone()]);
        env.storage()
            .persistent()
            .set(&DataKey::VerifiedImplementations, &initial);
    }

    pub fn get_admin(env: Env) -> Address {
        env.storage().persistent().get(&DataKey::Admin).unwrap()
    }

    pub fn get_implementation(env: Env) -> Address {
        env.storage()
            .persistent()
            .get(&DataKey::Implementation)
            .unwrap()
    }

    fn is_verified(env: &Env, implementation: &Address) -> bool {
        let verified: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::VerifiedImplementations)
            .unwrap_or(Vec::new(env));
        verified.contains(implementation)
    }

    /// Adds an implementation to the allowlist of verified implementations.
    /// Only the admin may call this. Upgrades are restricted to registered
    /// implementations so a compromised admin key cannot point the proxy at an
    /// arbitrary contract.
    ///
    /// # Returns
    /// - `Err(ProxyError::AlreadyVerified)` if the implementation is registered.
    pub fn register_implementation(
        env: Env,
        implementation: Address,
    ) -> Result<(), ProxyError> {
        Self::guard_admin(env.clone())?;
        if Self::is_verified(&env, &implementation) {
            return Err(ProxyError::AlreadyVerified);
        }
        let mut verified: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::VerifiedImplementations)
            .unwrap_or(Vec::new(&env));
        verified.push_back(implementation);
        env.storage()
            .persistent()
            .set(&DataKey::VerifiedImplementations, &verified);
        Ok(())
    }

    /// Removes an implementation from the allowlist. Only the admin may call
    /// this. Removing a currently active implementation does not downgrade the
    /// proxy; it only prevents future upgrades to it.
    ///
    /// # Returns
    /// - `Err(ProxyError::NotVerified)` if the implementation is not registered.
    pub fn unregister_implementation(
        env: Env,
        implementation: Address,
    ) -> Result<(), ProxyError> {
        Self::guard_admin(env.clone())?;
        if !Self::is_verified(&env, &implementation) {
            return Err(ProxyError::NotVerified);
        }
        let verified: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::VerifiedImplementations)
            .unwrap_or(Vec::new(&env));
        let mut updated: Vec<Address> = Vec::new(&env);
        for a in verified.iter() {
            if a != implementation {
                updated.push_back(a);
            }
        }
        env.storage()
            .persistent()
            .set(&DataKey::VerifiedImplementations, &updated);
        Ok(())
    }

    /// Returns the allowlist of verified implementations.
    pub fn get_verified_implementations(env: Env) -> Vec<Address> {
        env.storage()
            .persistent()
            .get(&DataKey::VerifiedImplementations)
            .unwrap_or(Vec::new(&env))
    }

    fn guard_admin(env: Env) -> Result<(), ProxyError> {
        if !env.storage().persistent().has(&DataKey::Admin) {
            return Err(ProxyError::NotInitialized);
        }
        let admin = Self::get_admin(env.clone());
        admin.require_auth();
        Ok(())
    }

    /// Safely upgrades the proxy to a verified implementation.
    ///
    /// # Errors
    /// - `ProxyError::NotInitialized` if the proxy was never initialized.
    /// - `ProxyError::UnverifiedImplementation` if the implementation is not
    ///   registered via [`Self::register_implementation`].
    /// - `ProxyError::SelfUpgrade` if the implementation is already active.
    pub fn upgrade_to(env: Env, implementation: Address) -> Result<(), ProxyError> {
        Self::guard_admin(env.clone())?;
        if implementation == Self::get_implementation(env.clone()) {
            return Err(ProxyError::SelfUpgrade);
        }
        if !Self::is_verified(&env, &implementation) {
            return Err(ProxyError::UnverifiedImplementation);
        }
        env.storage()
            .persistent()
            .set(&DataKey::Implementation, &implementation);
        Ok(())
    }

    /// Safely upgrades the proxy to a verified implementation and immediately
    /// delegates a call to it.
    ///
    /// # Errors
    /// Same as [`Self::upgrade_to`].
    pub fn upgrade_to_and_call(
        env: Env,
        implementation: Address,
        method: Symbol,
        args: Vec<Val>,
    ) -> Result<Val, ProxyError> {
        Self::upgrade_to(env.clone(), implementation)?;
        Ok(Self::delegate_call(env, method, args))
    }

    pub fn delegate_call(env: Env, method: Symbol, args: Vec<Val>) -> Val {
        let implementation = Self::get_implementation(env.clone());
        env.invoke_contract(&implementation, &method, args)
    }

    pub fn increment(env: Env, amount: i32) -> i32 {
        let current = Self::get_value(env.clone());
        let method = Symbol::new(&env, "calculate");
        let args: Vec<Val> = Vec::from_array(&env, [current.into_val(&env), amount.into_val(&env)]);
        let next: i32 = env.invoke_contract(&Self::get_implementation(env.clone()), &method, args);
        Self::set_value(env, next);
        next
    }

    pub fn get_value(env: Env) -> i32 {
        env.storage()
            .persistent()
            .get(&DataKey::Counter)
            .unwrap_or(0)
    }

    pub fn set_value(env: Env, value: i32) {
        env.storage().persistent().set(&DataKey::Counter, &value);
    }

    pub fn set_storage(env: Env, key: BytesN<32>, value: Val) {
        let admin = Self::get_admin(env.clone());
        admin.require_auth();
        env.storage()
            .persistent()
            .set(&DataKey::Storage(key), &value);
    }

    pub fn get_storage(env: Env, key: BytesN<32>) -> Option<Val> {
        env.storage().persistent().get(&DataKey::Storage(key))
    }
}