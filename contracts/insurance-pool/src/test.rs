#![cfg(test)]

use super::*;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger as _},
    vec, Env, InvokeError, IntoVal,
};

use ossify_reference_rwa::{MockInvoiceToken, MockInvoiceTokenClient};
use ossify_registry::{RwaRegistry, RwaRegistryClient};

/// A $1,000 invoice in 7-decimal units.
const FACE_VALUE: i128 = 1000_0000000;
/// 2026-11-01T00:00:00Z, the due date used across the tests.
const DUE_DATE: u64 = 1_793_491_200;
/// One day before `DUE_DATE`.
const BEFORE_DUE: u64 = DUE_DATE - 86_400;
/// One day after `DUE_DATE`.
const AFTER_DUE: u64 = DUE_DATE + 86_400;
/// Half of `FACE_VALUE`: what the pool lends against it.
const AMOUNT_LENT: i128 = FACE_VALUE / 2;

/// Deploys a registry, an invoice token registered in it, and the insurance pool,
/// and returns their ids plus a fresh borrower.
fn setup(timestamp: u64) -> (Env, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(timestamp);

    let registry_id = env.register(RwaRegistry, ());
    let registry_client = RwaRegistryClient::new(&env, &registry_id);

    let token_id = env.register(MockInvoiceToken, ());
    let token_client = MockInvoiceTokenClient::new(&env, &token_id);
    let issuer = Address::generate(&env);
    token_client.init(&issuer, &FACE_VALUE, &DUE_DATE);
    registry_client.register(&token_id);

    let pool_id = env.register(InsurancePool, ());
    let borrower = Address::generate(&env);

    (env, registry_id, token_id, pool_id, borrower)
}

/// Unwraps the typed error of a `try_` client call, so the tests assert on the
/// `contracterror` variant rather than on a panic message.
fn typed_error<T, E>(
    result: Result<Result<T, E>, Result<ContractError, InvokeError>>,
) -> ContractError {
    let Err(Ok(error)) = result else {
        panic!("expected the call to fail with a contract error");
    };
    error
}

// ============================================================================
// Borrow tests
// ============================================================================

#[test]
fn test_borrow_against_registered_token() {
    let (env, registry_id, token_id, pool_id, borrower) = setup(BEFORE_DUE);
    let pool_client = InsurancePoolClient::new(&env, &pool_id);

    let amount_lent = pool_client.borrow_against(&borrower, &registry_id, &token_id);

    assert_eq!(amount_lent, AMOUNT_LENT);

    // Asserted before reading the position: `Events::all` covers the last invocation only.
    assert_eq!(
        env.events().all(),
        vec![
            &env,
            (
                pool_id.clone(),
                (symbol_short!("borrow"),).into_val(&env),
                (borrower.clone(), token_id.clone(), amount_lent).into_val(&env)
            )
        ]
    );

    let position = pool_client.get_position(&borrower, &token_id).unwrap();
    assert_eq!(position.collateral_value, FACE_VALUE);
    assert_eq!(position.amount_owed, AMOUNT_LENT);
}

#[test]
fn test_repay_clears_the_position() {
    let (env, registry_id, token_id, pool_id, borrower) = setup(BEFORE_DUE);
    let pool_client = InsurancePoolClient::new(&env, &pool_id);

    let amount_lent = pool_client.borrow_against(&borrower, &registry_id, &token_id);
    let repaid = pool_client.repay(&borrower, &token_id);

    assert_eq!(repaid, amount_lent);
    assert_eq!(
        env.events().all(),
        vec![
            &env,
            (
                pool_id.clone(),
                (symbol_short!("repay"),).into_val(&env),
                (borrower.clone(), token_id.clone(), repaid).into_val(&env)
            )
        ]
    );

    assert_eq!(pool_client.get_position(&borrower, &token_id), None);

    // The open position was the only thing blocking a second borrow.
    assert_eq!(
        pool_client.borrow_against(&borrower, &registry_id, &token_id),
        AMOUNT_LENT
    );
}

// ============================================================================
// Double borrow rejected
// ============================================================================

#[test]
fn test_second_borrow_against_the_same_collateral_is_rejected() {
    let (env, registry_id, token_id, pool_id, borrower) = setup(BEFORE_DUE);
    let pool_client = InsurancePoolClient::new(&env, &pool_id);

    pool_client.borrow_against(&borrower, &registry_id, &token_id);

    assert_eq!(
        typed_error(pool_client.try_borrow_against(&borrower, &registry_id, &token_id)),
        ContractError::PositionAlreadyOpen
    );

    // A different borrower is not affected: positions are per borrower.
    let other_borrower = Address::generate(&env);
    assert_eq!(
        pool_client.borrow_against(&other_borrower, &registry_id, &token_id),
        AMOUNT_LENT
    );

    // The rejected call left the first position untouched.
    assert_eq!(
        pool_client
            .get_position(&borrower, &token_id)
            .unwrap()
            .amount_owed,
        AMOUNT_LENT
    );
}

// ============================================================================
// Unregistered token rejected
// ============================================================================

#[test]
fn test_unregistered_token_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let registry_id = env.register(RwaRegistry, ());
    let token_id = env.register(MockInvoiceToken, ());
    let pool_id = env.register(InsurancePool, ());
    let borrower = Address::generate(&env);

    let pool_client = InsurancePoolClient::new(&env, &pool_id);

    assert_eq!(
        typed_error(pool_client.try_borrow_against(&borrower, &registry_id, &token_id)),
        ContractError::NotRegistered
    );
    assert_eq!(pool_client.get_position(&borrower, &token_id), None);
}

// ============================================================================
// Non-redeemable token rejected
// ============================================================================

#[test]
fn test_non_redeemable_token_is_rejected() {
    let (env, registry_id, token_id, pool_id, borrower) = setup(BEFORE_DUE);
    let pool_client = InsurancePoolClient::new(&env, &pool_id);

    // At its due date the claim is no longer redeemable, while staying
    // registered, so this isolates the redeemability check.
    env.ledger().set_timestamp(DUE_DATE);

    assert_eq!(
        typed_error(pool_client.try_borrow_against(&borrower, &registry_id, &token_id)),
        ContractError::NotRedeemable
    );
    assert_eq!(pool_client.get_position(&borrower, &token_id), None);
}

// ============================================================================
// Borrower authorization
// ============================================================================

#[test]
#[should_panic(expected = "HostError: Error(Auth, InvalidAction)")]
fn test_borrow_requires_borrower_authorization() {
    let env = Env::default();

    let registry_id = env.register(RwaRegistry, ());
    let token_id = env.register(MockInvoiceToken, ());
    let pool_id = env.register(InsurancePool, ());
    let borrower = Address::generate(&env);

    // No auth is mocked, so another address cannot open a position in the
    // borrower's name.
    InsurancePoolClient::new(&env, &pool_id).borrow_against(&borrower, &registry_id, &token_id);
}