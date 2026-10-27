#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::{Address as _, Events}, Env, IntoVal, symbol_short, vec};

use ossify_registry::{RwaRegistry, RwaRegistryClient};
use ossify_reference_rwa::{MockInvoiceToken, MockInvoiceTokenClient};

/// A $1,000 invoice in 7-decimal units.
const FACE_VALUE: i128 = 1000_0000000;
/// 2026-11-01T00:00:00Z: after the default ledger timestamp of 0, so the
/// invoice is still before its due date for the whole test.
const DUE_DATE: u64 = 1_793_491_200;

#[test]
fn test_toy_lending_pool_borrow() {
    let env = Env::default();
    env.mock_all_auths();
    
    let registry_id = env.register(RwaRegistry, ());
    let registry_client = RwaRegistryClient::new(&env, &registry_id);
    
    let token_id = env.register(MockInvoiceToken, ());
    let token_client = MockInvoiceTokenClient::new(&env, &token_id);

    // The reference token stores real asset state, so it has to be initialized
    // before a consumer can read live collateral from it.
    let issuer = Address::generate(&env);
    token_client.init(&issuer, &FACE_VALUE, &DUE_DATE);
    
    registry_client.register(&token_id);
    
    let registered = registry_client.get_registered();
    assert_eq!(registered.len(), 1);
    assert_eq!(registered.get(0).unwrap(), token_id.clone());
    
    let pool_id = env.register(ToyLendingPool, ());
    let pool_client = ToyLendingPoolClient::new(&env, &pool_id);
    
    let borrow_amount = pool_client.borrow_against(&registry_id, &token_id);
    
    assert_eq!(
        env.events().all(),
        vec![
            &env,
            (
                pool_id.clone(),
                (symbol_short!("borrow"),).into_val(&env),
                (token_id.clone(), 500_0000000i128).into_val(&env)
            )
        ]
    );

    assert_eq!(borrow_amount, 500_0000000);
}

#[test]
#[should_panic(expected = "Token is not registered in the RwaRegistry")]
fn test_toy_lending_pool_borrow_unregistered() {
    let env = Env::default();
    
    let registry_id = env.register(RwaRegistry, ());
    let token_id = env.register(MockInvoiceToken, ());
    let pool_id = env.register(ToyLendingPool, ());
    
    let pool_client = ToyLendingPoolClient::new(&env, &pool_id);
    
    pool_client.borrow_against(&registry_id, &token_id);
}
