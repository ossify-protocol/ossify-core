#![no_std]
use ossify_registry::RwaRegistryClient;
use ossify_rwa_trait::RwaCollateralClient;
use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, Address, Env,
};

/// Storage keys for the insurance pool.
#[contracttype]
pub enum DataKey {
    /// An open position, keyed by borrower and collateral token.
    Position(Address, Address),
}

/// A position the insurance pool has against one collateral token.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Position {
    /// The collateral value the token reported when the position was opened.
    pub collateral_value: i128,
    /// The amount owed, and therefore what the borrower owes until repayment.
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

/// Emitted when the insurance pool lends out against a registered RWA token.
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

#[contract]
pub struct InsurancePool;

#[contractimpl]
impl InsurancePool {
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