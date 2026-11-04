#![no_std]
use ossify_rwa_trait::RwaCollateralClient;
use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, Address, Env, Vec,
};

/// The largest page [`RwaRegistry::get_registered_page`] will return.
///
/// The bound keeps a single invocation's read footprint predictable. An indexer
/// walking a large registry should page through it with this limit rather than
/// fetching every token at once.
pub const MAX_PAGE_SIZE: u32 = 50;

#[contracttype]
pub enum DataKey {
    Tokens,
}

/// A page of registered token addresses plus the cursor for the next page.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistrationPage {
    /// The token addresses in this page, in registration order.
    pub tokens: Vec<Address>,
    /// The cursor to pass to fetch the following page, or `None` when this is
    /// the last page.
    pub next_cursor: Option<u32>,
}

/// An aggregate view of one registered token's collateral state.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollateralInfo {
    /// The registered token this snapshot describes.
    pub token: Address,
    /// The value the token reports through `RwaCollateral::collateral_value`.
    pub collateral_value: i128,
    /// The flag the token reports through `RwaCollateral::is_redeemable`.
    pub is_redeemable: bool,
}

/// Typed failures returned by the registry.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum RegistryError {
    /// `limit` was zero.
    InvalidLimit = 1,
    /// `limit` was greater than [`MAX_PAGE_SIZE`].
    LimitTooLarge = 2,
    /// `refresh_collateral_info` was called for a token that is not listed.
    NotRegistered = 3,
}

/// Emitted when a token registers itself with the registry.
///
/// `data_format = "single-value"` keeps the published data identical to the
/// plain address that was passed to `Events::publish`.
#[contractevent(data_format = "single-value")]
pub struct Register {
    token: Address,
}

/// Emitted when a token is removed from the registry.
#[contractevent(data_format = "single-value")]
pub struct Deregister {
    token: Address,
}

/// Emitted when a registered token publishes its current collateral state.
///
/// Indexers can follow this event instead of querying every token, provided the
/// token calls [`RwaRegistry::refresh_collateral_info`] after it changes.
#[contractevent(data_format = "vec")]
pub struct CollateralInfoUpdated {
    token: Address,
    collateral_value: i128,
    is_redeemable: bool,
}

#[contract]
pub struct RwaRegistry;

#[contractimpl]
impl RwaRegistry {
    /// Registers a token address as implementing the RwaCollateral trait.
    pub fn register(env: Env, token: Address) {
        token.require_auth();
        let mut tokens = read_tokens(&env);

        if !tokens.contains(&token) {
            tokens.push_back(token.clone());
            env.storage().instance().set(&DataKey::Tokens, &tokens);
            Register { token }.publish(&env);
        }
    }

    /// Returns a list of all registered compliant RWA token addresses.
    ///
    /// This is retained for backward compatibility, but it is unbounded: the
    /// whole list is read and returned in one invocation, so it is not suitable
    /// for a large registry. New callers should use
    /// [`Self::get_registered_page`], and can size their response with
    /// [`Self::get_registered_count`] first.
    pub fn get_registered(env: Env) -> Vec<Address> {
        read_tokens(&env)
    }

    /// Returns the number of registered compliant RWA token addresses.
    pub fn get_registered_count(env: Env) -> u32 {
        read_tokens(&env).len()
    }

    /// Returns one page of registered token addresses, in registration order.
    ///
    /// Pass `cursor: None` for the first page, then the `next_cursor` returned
    /// by the previous page. `limit` must be between `1` and [`MAX_PAGE_SIZE`];
    /// a zero limit fails with [`RegistryError::InvalidLimit`] and a larger one
    /// with [`RegistryError::LimitTooLarge`]. A cursor at or past the end of the
    /// list returns an empty page with `next_cursor: None` rather than an error,
    /// so a caller can stop when it sees no tokens.
    pub fn get_registered_page(
        env: Env,
        cursor: Option<u32>,
        limit: u32,
    ) -> Result<RegistrationPage, RegistryError> {
        if limit == 0 {
            return Err(RegistryError::InvalidLimit);
        }
        if limit > MAX_PAGE_SIZE {
            return Err(RegistryError::LimitTooLarge);
        }

        let tokens = read_tokens(&env);
        let start = cursor.unwrap_or(0);
        let len = tokens.len();

        if start >= len {
            return Ok(RegistrationPage {
                tokens: Vec::new(&env),
                next_cursor: None,
            });
        }

        let end = if len - start > limit {
            start + limit
        } else {
            len
        };

        let mut page = Vec::new(&env);
        for index in start..end {
            page.push_back(tokens.get(index).unwrap());
        }

        Ok(RegistrationPage {
            tokens: page,
            next_cursor: if end < len { Some(end) } else { None },
        })
    }

    /// Returns one aggregate view of a registered token's collateral state.
    ///
    /// The values are read live through the token's `RwaCollateral`
    /// implementation, so this reflects the token as of the current ledger.
    /// Returns `None` when the token is not registered. A registered token that
    /// traps in either trait call makes this call trap as well; the registry
    /// does not try to price a broken token as zero.
    pub fn get_collateral_info(env: Env, token: Address) -> Option<CollateralInfo> {
        if !read_tokens(&env).contains(&token) {
            return None;
        }
        Some(read_collateral_info(&env, &token))
    }

    /// Publishes a registered token's current collateral state as an event.
    ///
    /// The token supplies the values and must authorize the call, so no third
    /// party can publish on its behalf. It has to supply them rather than have
    /// the registry read them back: Soroban rejects contract re-entry, and a
    /// token calling here is already on the call stack. The registry does not
    /// verify the values, just as it does not audit a token at registration, so
    /// [`Self::get_collateral_info`] remains the authoritative live read.
    ///
    /// A token that changes its value or redeemability should call this after
    /// the change so indexers can follow the event instead of polling. Fails
    /// with [`RegistryError::NotRegistered`] for a token that is not listed.
    pub fn refresh_collateral_info(
        env: Env,
        token: Address,
        collateral_value: i128,
        is_redeemable: bool,
    ) -> Result<CollateralInfo, RegistryError> {
        token.require_auth();

        if !read_tokens(&env).contains(&token) {
            return Err(RegistryError::NotRegistered);
        }

        let info = CollateralInfo {
            token: token.clone(),
            collateral_value,
            is_redeemable,
        };
        CollateralInfoUpdated {
            token,
            collateral_value,
            is_redeemable,
        }
        .publish(&env);

        Ok(info)
    }

    /// Deregisters a token address, removing it from the compliant RWA list.
    /// If the token is not currently registered, this function acts as a no-op.
    pub fn deregister(env: Env, token: Address) {
        token.require_auth();
        let mut tokens = read_tokens(&env);

        if let Some(index) = tokens.first_index_of(&token) {
            tokens.remove(index);
            env.storage().instance().set(&DataKey::Tokens, &tokens);
            Deregister { token }.publish(&env);
        }
    }
}

/// Reads a token's collateral state through the shared trait, the same way a
/// consumer protocol does.
fn read_collateral_info(env: &Env, token: &Address) -> CollateralInfo {
    let client = RwaCollateralClient::new(env, token);
    CollateralInfo {
        token: token.clone(),
        collateral_value: client.collateral_value(token),
        is_redeemable: client.is_redeemable(token),
    }
}

/// Reads the registry list, treating an unset entry as empty.
fn read_tokens(env: &Env) -> Vec<Address> {
    env.storage()
        .instance()
        .get(&DataKey::Tokens)
        .unwrap_or(Vec::new(env))
}

mod test;
