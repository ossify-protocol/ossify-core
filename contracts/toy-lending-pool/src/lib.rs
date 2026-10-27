#![no_std]
use ossify_registry::RwaRegistryClient;
use ossify_rwa_trait::RwaCollateralClient;
use soroban_sdk::{contract, contracterror, contractevent, contractimpl, contracttype, Address, Env};

/// Storage keys for the pool.
#[contracttype]
pub enum DataKey {
    /// An open loan, keyed by borrower and collateral token.
    Position(Address, Address),
}

/// A loan the pool has issued against one collateral token.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Position {
    /// The collateral value the token reported when the loan was issued.
    pub collateral_value: i128,
    /// The amount lent, and therefore what the borrower owes until repayment.
    pub amount_owed: i128,
}

/// Typed failures returned by the pool.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ContractError {
    /// The token is not listed in the registry the pool was pointed at.
    NotRegistered = 1,
    /// The token reports that it is not currently redeemable.
    NotRedeemable = 2,
    /// The borrower already has an open position against this collateral.
    PositionAlreadyOpen = 3,
    /// `repay` was called with no open position for this borrower and token.
    NoOpenPosition = 4,
}

/// Emitted when the pool lends out against a registered RWA token.
///
/// `data_format = "vec"` keeps the published data a list, matching what
/// `Events::publish` emitted before the pool stored positions. The `borrower` is
/// part of it because a position now belongs to one.
#[contractevent(data_format = "vec")]
pub struct Borrow {
    borrower: Address,
    token: Address,
    amount_lent: i128,
}

/// Emitted when a borrower repays an open position.
#[contractevent(data_format = "vec")]
pub struct Repay {
    borrower: Address,
    token: Address,
    amount_repaid: i128,
}

/// Positions are per user, so they live in persistent storage and are bumped
/// when their remaining TTL drops below `POSITION_TTL_THRESHOLD` ledgers
/// (~14 days at 5-second ledgers), extending them to `POSITION_TTL_EXTEND_TO`
/// (~30 days). Without the bump a position would eventually be archived while
/// still outstanding, which would silently let the borrower borrow again.
const POSITION_TTL_THRESHOLD: u32 = 241_920;
const POSITION_TTL_EXTEND_TO: u32 = 518_400;

#[contract]
pub struct ToyLendingPool;

#[contractimpl]
impl ToyLendingPool {
    /// Lends out against a registered RWA collateral token.
    ///
    /// The borrower must authorize the call, the token must be listed in the
    /// registry, must report itself redeemable, and must not already back an
    /// open position for that borrower. The pool lends half of the collateral
    /// value the token reports.
    pub fn borrow_against(
        env: Env,
        borrower: Address,
        registry: Address,
        token: Address,
    ) -> Result<i128, ContractError> {
        borrower.require_auth();

        let registry_client = RwaRegistryClient::new(&env, &registry);
        if !registry_client.get_registered().contains(&token) {
            return Err(ContractError::NotRegistered);
        }

        // Read the collateral through the trait client, the way any consumer does.
        let token_client = RwaCollateralClient::new(&env, &token);
        if !token_client.is_redeemable(&token) {
            return Err(ContractError::NotRedeemable);
        }
        let collateral_value = token_client.collateral_value(&token);

        let key = DataKey::Position(borrower.clone(), token.clone());
        if env.storage().persistent().has(&key) {
            return Err(ContractError::PositionAlreadyOpen);
        }

        let amount_lent = collateral_value / 2;
        env.storage().persistent().set(
            &key,
            &Position {
                collateral_value,
                amount_owed: amount_lent,
            },
        );
        env.storage()
            .persistent()
            .extend_ttl(&key, POSITION_TTL_THRESHOLD, POSITION_TTL_EXTEND_TO);

        Borrow {
            borrower,
            token,
            amount_lent,
        }
        .publish(&env);

        Ok(amount_lent)
    }

    /// Repays an open position in full and clears it, returning what was owed.
    ///
    /// The borrower must authorize the call. This mock moves no settlement
    /// asset: repayment is the position being cleared, which is also what makes
    /// the collateral available to borrow against again.
    pub fn repay(env: Env, borrower: Address, token: Address) -> Result<i128, ContractError> {
        borrower.require_auth();

        let key = DataKey::Position(borrower.clone(), token.clone());
        let position: Position = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(ContractError::NoOpenPosition)?;

        env.storage().persistent().remove(&key);

        Repay {
            borrower,
            token,
            amount_repaid: position.amount_owed,
        }
        .publish(&env);

        Ok(position.amount_owed)
    }

    /// Returns the borrower's open position against `token`, if there is one.
    pub fn get_position(env: Env, borrower: Address, token: Address) -> Option<Position> {
        env.storage()
            .persistent()
            .get(&DataKey::Position(borrower, token))
    }
}
mod test;
