#![cfg(test)]

use super::*;
use ossify_registry::{RwaRegistry, RwaRegistryClient};
use ossify_rwa_trait::RwaCollateralClient;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, ConversionError, Env, Error, InvokeError,
};

/// A $1,000 invoice in 7-decimal units.
const FACE_VALUE: i128 = 1000_0000000;
/// 2026-11-01T00:00:00Z, the due date used across the tests.
const DUE_DATE: u64 = 1_793_491_200;
/// One day before `DUE_DATE`.
const BEFORE_DUE: u64 = DUE_DATE - 86_400;
/// One day after `DUE_DATE`.
const AFTER_DUE: u64 = DUE_DATE + 86_400;

/// Registers a registry and the reference token, moves the ledger to
/// `timestamp`, and initializes the invoice as a fresh issuer.
/// `mock_all_auths` lets the issuer authorize `init` and the token authorize its
/// own registration.
fn setup(timestamp: u64) -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(timestamp);

    let registry_id = env.register(RwaRegistry, ());
    let token_id = env.register(MockInvoiceToken, ());
    let issuer = Address::generate(&env);

    MockInvoiceTokenClient::new(&env, &token_id).init(&issuer, &FACE_VALUE, &DUE_DATE);

    (env, registry_id, token_id)
}

/// Unwraps the typed error of a `try_` client call to a function that returns
/// `Result<_, ContractError>`.
fn typed_error<T>(
    result: Result<Result<T, ConversionError>, Result<ContractError, InvokeError>>,
) -> ContractError {
    let Err(Ok(error)) = result else {
        panic!("expected the call to fail with a contract error");
    };
    error
}

/// `liquidate` belongs to the shared trait, whose signature returns
/// `soroban_sdk::Error` rather than a contract-specific error type, so the host
/// error has to be converted back into [`ContractError`] to assert on it.
fn liquidate_error(
    result: Result<Result<(), ConversionError>, Result<Error, InvokeError>>,
) -> ContractError {
    let Err(Ok(error)) = result else {
        panic!("expected liquidate to fail with a contract error");
    };
    ContractError::try_from(error).expect("liquidate returned a non-contract error")
}

#[test]
fn test_happy_path_stores_the_asset_and_reports_it() {
    let (env, _registry_id, token_id) = setup(BEFORE_DUE);
    let token_client = MockInvoiceTokenClient::new(&env, &token_id);

    assert_eq!(token_client.collateral_value(&token_id), FACE_VALUE);
    assert!(token_client.is_redeemable(&token_id));
}

#[test]
fn test_liquidate_takes_over_the_claim() {
    let (env, _registry_id, token_id) = setup(BEFORE_DUE);
    let token_client = MockInvoiceTokenClient::new(&env, &token_id);
    let liquidator = Address::generate(&env);

    token_client.liquidate(&token_id, &liquidator);

    assert_eq!(token_client.collateral_value(&token_id), 0);
    assert!(!token_client.is_redeemable(&token_id));
}

#[test]
fn test_liquidate_fails_when_already_liquidated() {
    let (env, _registry_id, token_id) = setup(BEFORE_DUE);
    let token_client = MockInvoiceTokenClient::new(&env, &token_id);
    let liquidator = Address::generate(&env);

    token_client.liquidate(&token_id, &liquidator);

    assert_eq!(
        liquidate_error(token_client.try_liquidate(&token_id, &liquidator)),
        ContractError::AlreadyLiquidated
    );
}

#[test]
fn test_liquidate_fails_when_already_redeemed() {
    let (env, _registry_id, token_id) = setup(BEFORE_DUE);
    let token_client = MockInvoiceTokenClient::new(&env, &token_id);
    let liquidator = Address::generate(&env);

    token_client.redeem();

    assert_eq!(token_client.collateral_value(&token_id), 0);
    assert!(!token_client.is_redeemable(&token_id));

    assert_eq!(
        liquidate_error(token_client.try_liquidate(&token_id, &liquidator)),
        ContractError::AlreadyRedeemed
    );
}

#[test]
fn test_past_due_asset_is_worthless_and_not_redeemable() {
    let (env, _registry_id, token_id) = setup(AFTER_DUE);
    let token_client = MockInvoiceTokenClient::new(&env, &token_id);

    assert_eq!(token_client.collateral_value(&token_id), 0);
    assert!(!token_client.is_redeemable(&token_id));

    assert_eq!(
        typed_error(token_client.try_redeem()),
        ContractError::PastDue
    );
}

#[test]
#[should_panic(expected = "HostError: Error(Auth, InvalidAction)")]
fn test_init_requires_issuer_authorization() {
    let env = Env::default();
    env.ledger().set_timestamp(BEFORE_DUE);

    let token_id = env.register(MockInvoiceToken, ());
    let issuer = Address::generate(&env);
    let token_client = MockInvoiceTokenClient::new(&env, &token_id);

    // No auth is mocked, so `issuer.require_auth()` inside `init` fails.
    token_client.init(&issuer, &FACE_VALUE, &DUE_DATE);
}

#[test]
fn test_register_self_registers_token_in_registry() {
    let (env, registry_id, token_id) = setup(BEFORE_DUE);
    let registry_client = RwaRegistryClient::new(&env, &registry_id);
    let token_client = MockInvoiceTokenClient::new(&env, &token_id);

    assert!(registry_client.get_registered().is_empty());

    token_client.register_self(&registry_id);

    let registered = registry_client.get_registered();
    assert_eq!(registered.len(), 1);
    assert!(registered.contains(&token_id));
    assert_eq!(registered.get(0).unwrap(), token_id);
}

/// Exercises the `RwaCollateralClient` path a consumer protocol uses, against
/// the deployed token, exactly as the consumer guide describes.
#[test]
fn test_consumer_can_call_token_through_trait_client() {
    let (env, registry_id, token_id) = setup(BEFORE_DUE);
    let token_client = MockInvoiceTokenClient::new(&env, &token_id);
    token_client.register_self(&registry_id);

    let registry_client = RwaRegistryClient::new(&env, &registry_id);
    assert!(registry_client.get_registered().contains(&token_id));

    let collateral_client = RwaCollateralClient::new(&env, &token_id);
    assert!(collateral_client.is_redeemable(&token_id));

    let value = collateral_client.collateral_value(&token_id);
    assert_eq!(value, FACE_VALUE);

    let liquidator = Address::generate(&env);
    collateral_client.liquidate(&token_id, &liquidator);

    assert_eq!(collateral_client.collateral_value(&token_id), 0);
    assert!(!collateral_client.is_redeemable(&token_id));
}
