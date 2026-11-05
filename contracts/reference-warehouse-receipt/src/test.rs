#![cfg(test)]

use super::*;
use ossify_reference_rwa::{MockInvoiceToken, MockInvoiceTokenClient};
use ossify_registry::{RwaRegistry, RwaRegistryClient};
use ossify_rwa_trait::RwaCollateralClient;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger as _},
    vec, Address, ConversionError, Env, Error, IntoVal, InvokeError,
};

/// 500 units of goods in storage.
const QUANTITY: u32 = 500;
/// Each unit is worth $2.00 in 7-decimal units.
const UNIT_PRICE: i128 = 2_0000000;
/// The resulting $1,000 receipt value in 7-decimal units.
const FACE_VALUE: i128 = 1000_0000000;
/// 2026-11-01T00:00:00Z, the storage expiry used across the tests.
const STORAGE_EXPIRY: u64 = 1_793_491_200;
/// One day before `STORAGE_EXPIRY`.
const BEFORE_EXPIRY: u64 = STORAGE_EXPIRY - 86_400;
/// One day after `STORAGE_EXPIRY`.
const AFTER_EXPIRY: u64 = STORAGE_EXPIRY + 86_400;

/// Registers a registry and the receipt token, moves the ledger to `timestamp`,
/// and initializes the receipt as a fresh depositor. `mock_all_auths` lets the
/// warehouse authorize `init` and the token authorize its own registration.
fn setup(timestamp: u64) -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(timestamp);

    let registry_id = env.register(RwaRegistry, ());
    let token_id = env.register(WarehouseReceiptToken, ());
    let warehouse = Address::generate(&env);
    let depositor = Address::generate(&env);

    WarehouseReceiptTokenClient::new(&env, &token_id).init(
        &warehouse,
        &depositor,
        &QUANTITY,
        &UNIT_PRICE,
        &STORAGE_EXPIRY,
    );

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
fn test_happy_path_reports_the_stored_valuation() {
    let (env, _registry_id, token_id) = setup(BEFORE_EXPIRY);
    let token_client = WarehouseReceiptTokenClient::new(&env, &token_id);

    // The value is quantity times the unit price read from storage, not a
    // constant returned by the contract.
    assert_eq!(token_client.collateral_value(&token_id), FACE_VALUE);
    assert!(token_client.is_redeemable(&token_id));
}

#[test]
fn test_redeem_by_the_depositor_retires_the_receipt() {
    let (env, _registry_id, token_id) = setup(BEFORE_EXPIRY);
    let token_client = WarehouseReceiptTokenClient::new(&env, &token_id);

    token_client.redeem();

    assert_eq!(token_client.collateral_value(&token_id), 0);
    assert!(!token_client.is_redeemable(&token_id));

    assert_eq!(
        typed_error(token_client.try_redeem()),
        ContractError::AlreadyRedeemed
    );
}

#[test]
fn test_liquidate_marks_the_receipt_defaulted() {
    let (env, _registry_id, token_id) = setup(BEFORE_EXPIRY);
    let token_client = WarehouseReceiptTokenClient::new(&env, &token_id);
    let liquidator = Address::generate(&env);

    token_client.liquidate(&token_id, &liquidator);

    assert_eq!(token_client.collateral_value(&token_id), 0);
    assert!(!token_client.is_redeemable(&token_id));

    // A defaulted receipt is final: neither another liquidation nor a
    // redemption can revive it.
    assert_eq!(
        liquidate_error(token_client.try_liquidate(&token_id, &liquidator)),
        ContractError::AlreadyDefaulted
    );
    assert_eq!(
        typed_error(token_client.try_redeem()),
        ContractError::AlreadyDefaulted
    );
}

#[test]
fn test_liquidate_fails_when_already_redeemed() {
    let (env, _registry_id, token_id) = setup(BEFORE_EXPIRY);
    let token_client = WarehouseReceiptTokenClient::new(&env, &token_id);
    let liquidator = Address::generate(&env);

    token_client.redeem();

    assert_eq!(
        liquidate_error(token_client.try_liquidate(&token_id, &liquidator)),
        ContractError::AlreadyRedeemed
    );
}

#[test]
fn test_past_storage_expiry_is_worthless_and_not_redeemable() {
    let (env, _registry_id, token_id) = setup(AFTER_EXPIRY);
    let token_client = WarehouseReceiptTokenClient::new(&env, &token_id);
    let liquidator = Address::generate(&env);

    assert_eq!(token_client.collateral_value(&token_id), 0);
    assert!(!token_client.is_redeemable(&token_id));

    assert_eq!(
        typed_error(token_client.try_redeem()),
        ContractError::PastMaturity
    );

    // Expiry is not redemption: the goods are still there, so a liquidator can
    // still take over the claim.
    token_client.liquidate(&token_id, &liquidator);
    assert_eq!(
        liquidate_error(token_client.try_liquidate(&token_id, &liquidator)),
        ContractError::AlreadyDefaulted
    );
}

#[test]
fn test_liquidate_fails_on_a_token_that_was_never_initialized() {
    let env = Env::default();
    env.mock_all_auths();

    let token_id = env.register(WarehouseReceiptToken, ());
    let token_client = WarehouseReceiptTokenClient::new(&env, &token_id);
    let liquidator = Address::generate(&env);

    assert_eq!(token_client.collateral_value(&token_id), 0);
    assert!(!token_client.is_redeemable(&token_id));

    assert_eq!(
        liquidate_error(token_client.try_liquidate(&token_id, &liquidator)),
        ContractError::NotInitialized
    );
}

#[test]
fn test_init_rejects_invalid_amounts_without_writing_a_record() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(BEFORE_EXPIRY);

    let token_id = env.register(WarehouseReceiptToken, ());
    let token_client = WarehouseReceiptTokenClient::new(&env, &token_id);
    let warehouse = Address::generate(&env);
    let depositor = Address::generate(&env);

    for (quantity, unit_price) in [(0, UNIT_PRICE), (QUANTITY, 0), (QUANTITY, -1)] {
        assert_eq!(
            typed_error(token_client.try_init(
                &warehouse,
                &depositor,
                &quantity,
                &unit_price,
                &STORAGE_EXPIRY
            )),
            ContractError::InvalidAmount
        );
    }

    // quantity * unit_price must fit in an i128 for the stored valuation.
    assert_eq!(
        typed_error(token_client.try_init(
            &warehouse,
            &depositor,
            &u32::MAX,
            &i128::MAX,
            &STORAGE_EXPIRY
        )),
        ContractError::InvalidAmount
    );

    // None of the rejected calls left a record behind.
    assert_eq!(token_client.collateral_value(&token_id), 0);
    assert!(!token_client.is_redeemable(&token_id));

    token_client.init(
        &warehouse,
        &depositor,
        &QUANTITY,
        &UNIT_PRICE,
        &STORAGE_EXPIRY,
    );
    assert_eq!(token_client.collateral_value(&token_id), FACE_VALUE);
}

#[test]
fn test_init_can_only_run_once() {
    let (env, _registry_id, token_id) = setup(BEFORE_EXPIRY);
    let token_client = WarehouseReceiptTokenClient::new(&env, &token_id);
    let other = Address::generate(&env);

    assert_eq!(
        typed_error(token_client.try_init(&other, &other, &QUANTITY, &UNIT_PRICE, &STORAGE_EXPIRY)),
        ContractError::AlreadyInitialized
    );

    // The original record is untouched.
    assert_eq!(token_client.collateral_value(&token_id), FACE_VALUE);
}

#[test]
#[should_panic(expected = "HostError: Error(Auth, InvalidAction)")]
fn test_init_requires_warehouse_authorization() {
    let env = Env::default();
    env.ledger().set_timestamp(BEFORE_EXPIRY);

    let token_id = env.register(WarehouseReceiptToken, ());
    let warehouse = Address::generate(&env);
    let depositor = Address::generate(&env);
    let token_client = WarehouseReceiptTokenClient::new(&env, &token_id);

    // No auth is mocked, so `warehouse.require_auth()` inside `init` fails.
    token_client.init(
        &warehouse,
        &depositor,
        &QUANTITY,
        &UNIT_PRICE,
        &STORAGE_EXPIRY,
    );
}

#[test]
fn test_register_self_registers_token_in_registry() {
    let (env, registry_id, token_id) = setup(BEFORE_EXPIRY);
    let registry_client = RwaRegistryClient::new(&env, &registry_id);
    let token_client = WarehouseReceiptTokenClient::new(&env, &token_id);

    assert!(registry_client.get_registered().is_empty());

    token_client.register_self(&registry_id);

    // The registry, not the token, emits the event; the token only calls it.
    // Assert before another top-level call, since `events().all()` reports the
    // events of the most recent invocation.
    assert_eq!(
        env.events().all(),
        vec![
            &env,
            (
                registry_id.clone(),
                (symbol_short!("register"),).into_val(&env),
                token_id.into_val(&env)
            )
        ]
    );

    let registered = registry_client.get_registered();
    assert_eq!(registered.len(), 1);
    assert!(registered.contains(&token_id));
    assert_eq!(registered.get(0).unwrap(), token_id);
}

/// The point of a second reference token: one registry lists both asset types,
/// and a consumer reads each through the same trait.
#[test]
fn test_registry_lists_both_reference_tokens() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(BEFORE_EXPIRY);

    let registry_id = env.register(RwaRegistry, ());
    let registry_client = RwaRegistryClient::new(&env, &registry_id);

    let invoice_id = env.register(MockInvoiceToken, ());
    let invoice_client = MockInvoiceTokenClient::new(&env, &invoice_id);
    let issuer = Address::generate(&env);
    invoice_client.init(&issuer, &FACE_VALUE, &STORAGE_EXPIRY);

    let receipt_id = env.register(WarehouseReceiptToken, ());
    let receipt_client = WarehouseReceiptTokenClient::new(&env, &receipt_id);
    let warehouse = Address::generate(&env);
    let depositor = Address::generate(&env);
    receipt_client.init(
        &warehouse,
        &depositor,
        &QUANTITY,
        &UNIT_PRICE,
        &STORAGE_EXPIRY,
    );

    invoice_client.register_self(&registry_id);
    receipt_client.register_self(&registry_id);

    assert_eq!(registry_client.get_registered_count(), 2);
    let registered = registry_client.get_registered();
    assert_eq!(registered.get(0).unwrap(), invoice_id);
    assert_eq!(registered.get(1).unwrap(), receipt_id);

    let invoice_info = registry_client.get_collateral_info(&invoice_id).unwrap();
    assert_eq!(invoice_info.collateral_value, FACE_VALUE);
    assert!(invoice_info.is_redeemable);

    let receipt_info = registry_client.get_collateral_info(&receipt_id).unwrap();
    assert_eq!(receipt_info.collateral_value, FACE_VALUE);
    assert!(receipt_info.is_redeemable);
}

/// Exercises the `RwaCollateralClient` path a consumer protocol uses, against
/// the deployed receipt, exactly as the consumer guide describes.
#[test]
fn test_consumer_can_call_receipt_through_trait_client() {
    let (env, registry_id, token_id) = setup(BEFORE_EXPIRY);
    let token_client = WarehouseReceiptTokenClient::new(&env, &token_id);
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
