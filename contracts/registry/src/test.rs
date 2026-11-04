#![cfg(test)]

use super::*;
use ossify_rwa_trait::RwaCollateral;
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short,
    testutils::{Address as _, Events},
    vec, Address, ConversionError, Env, Error, IntoVal, InvokeError, Symbol, Vec,
};

/// A minimal RWA token implementing the shared trait, so the registry tests can
/// call `get_collateral_info` and `refresh_collateral_info` without depending on
/// the reference token crate (which would make the dev-dependencies cyclic).
#[contract]
pub struct MockRwaToken;

#[contracttype]
pub enum MockKey {
    Value,
    Redeemable,
}

#[contractimpl]
impl MockRwaToken {
    pub fn set(env: Env, collateral_value: i128, is_redeemable: bool) {
        env.storage()
            .instance()
            .set(&MockKey::Value, &collateral_value);
        env.storage()
            .instance()
            .set(&MockKey::Redeemable, &is_redeemable);
    }

    pub fn register_self(env: Env, registry: Address) {
        RwaRegistryClient::new(&env, &registry).register(&env.current_contract_address());
    }

    pub fn publish(env: Env, registry: Address) -> CollateralInfo {
        let collateral_value: i128 = env.storage().instance().get(&MockKey::Value).unwrap_or(0);
        let is_redeemable: bool = env
            .storage()
            .instance()
            .get(&MockKey::Redeemable)
            .unwrap_or(false);

        RwaRegistryClient::new(&env, &registry).refresh_collateral_info(
            &env.current_contract_address(),
            &collateral_value,
            &is_redeemable,
        )
    }
}

#[contractimpl]
impl RwaCollateral for MockRwaToken {
    fn collateral_value(env: Env, _token: Address) -> i128 {
        env.storage().instance().get(&MockKey::Value).unwrap_or(0)
    }

    fn is_redeemable(env: Env, _token: Address) -> bool {
        env.storage()
            .instance()
            .get(&MockKey::Redeemable)
            .unwrap_or(false)
    }

    fn liquidate(_env: Env, _token: Address, _liquidator: Address) -> Result<(), Error> {
        Ok(())
    }
}

/// Registers `count` fresh addresses and returns them in registration order.
fn register_tokens(env: &Env, client: &RwaRegistryClient, count: u32) -> Vec<Address> {
    let mut tokens = Vec::new(env);
    for _ in 0..count {
        let token = Address::generate(env);
        client.register(&token);
        tokens.push_back(token);
    }
    tokens
}

/// Unwraps the typed error of a `try_` client call.
fn typed_error<T>(
    result: Result<Result<T, ConversionError>, Result<RegistryError, InvokeError>>,
) -> RegistryError {
    let Err(Ok(error)) = result else {
        panic!("expected the call to fail with a contract error");
    };
    error
}

/// Builds the expected page from the full list, so the assertions spell out the
/// registration order rather than trusting the implementation's indexing.
fn expected_page(env: &Env, tokens: &Vec<Address>, start: u32, end: u32) -> Vec<Address> {
    let mut page = Vec::new(env);
    for index in start..end {
        page.push_back(tokens.get(index).unwrap());
    }
    page
}

#[test]
fn test_registry_register_and_deregister() {
    let env = Env::default();
    env.mock_all_auths();

    let registry_id = env.register(RwaRegistry, ());
    let client = RwaRegistryClient::new(&env, &registry_id);

    let token1 = Address::generate(&env);
    let token2 = Address::generate(&env);

    // Initial state
    assert_eq!(client.get_registered().len(), 0);

    // Register a new token
    client.register(&token1);

    assert_eq!(
        env.events().all(),
        vec![
            &env,
            (
                registry_id.clone(),
                (symbol_short!("register"),).into_val(&env),
                token1.into_val(&env)
            )
        ]
    );

    let registered = client.get_registered();
    assert_eq!(registered.len(), 1);
    assert_eq!(registered.get(0).unwrap(), token1.clone());

    // Attempt to register a duplicate
    client.register(&token1);
    let registered = client.get_registered();
    assert_eq!(registered.len(), 1);

    // Register a second token
    client.register(&token2);
    let registered = client.get_registered();
    assert_eq!(registered.len(), 2);

    // Deregister an existing token
    client.deregister(&token1);

    assert_eq!(
        env.events().all(),
        vec![
            &env,
            (
                registry_id.clone(),
                (Symbol::new(&env, "deregister"),).into_val(&env),
                token1.into_val(&env)
            )
        ]
    );

    let registered = client.get_registered();
    assert_eq!(registered.len(), 1);
    assert_eq!(registered.get(0).unwrap(), token2.clone());

    // Deregister a token that was never registered (or already deregistered)
    client.deregister(&token1);
    let registered = client.get_registered();
    assert_eq!(registered.len(), 1);
    assert_eq!(registered.get(0).unwrap(), token2.clone());
}

#[test]
fn test_get_registered_page_walks_first_middle_and_last() {
    let env = Env::default();
    env.mock_all_auths();

    let registry_id = env.register(RwaRegistry, ());
    let client = RwaRegistryClient::new(&env, &registry_id);
    let tokens = register_tokens(&env, &client, 5);

    assert_eq!(client.get_registered_count(), 5);

    // First page: two tokens, and a cursor pointing at the third.
    let first = client.get_registered_page(&None, &2);
    assert_eq!(first.tokens, expected_page(&env, &tokens, 0, 2));
    assert_eq!(first.next_cursor, Some(2));

    // Middle page: the next two tokens.
    let middle = client.get_registered_page(&first.next_cursor, &2);
    assert_eq!(middle.tokens, expected_page(&env, &tokens, 2, 4));
    assert_eq!(middle.next_cursor, Some(4));

    // Last page: the final token, with no further cursor.
    let last = client.get_registered_page(&middle.next_cursor, &2);
    assert_eq!(last.tokens, expected_page(&env, &tokens, 4, 5));
    assert_eq!(last.next_cursor, None);
}

#[test]
fn test_get_registered_page_at_an_exact_page_boundary_ends_the_walk() {
    let env = Env::default();
    env.mock_all_auths();

    let registry_id = env.register(RwaRegistry, ());
    let client = RwaRegistryClient::new(&env, &registry_id);
    let tokens = register_tokens(&env, &client, 4);

    let first = client.get_registered_page(&None, &2);
    assert_eq!(first.tokens, expected_page(&env, &tokens, 0, 2));
    assert_eq!(first.next_cursor, Some(2));

    // The last page is exactly full, so it must not hand out another cursor.
    let second = client.get_registered_page(&first.next_cursor, &2);
    assert_eq!(second.tokens, expected_page(&env, &tokens, 2, 4));
    assert_eq!(second.next_cursor, None);
}

#[test]
fn test_get_registered_page_on_an_empty_registry_is_empty() {
    let env = Env::default();

    let registry_id = env.register(RwaRegistry, ());
    let client = RwaRegistryClient::new(&env, &registry_id);

    assert_eq!(client.get_registered_count(), 0);

    let page = client.get_registered_page(&None, &10);
    assert!(page.tokens.is_empty());
    assert_eq!(page.next_cursor, None);
}

#[test]
fn test_get_registered_page_with_an_out_of_range_cursor_is_empty() {
    let env = Env::default();
    env.mock_all_auths();

    let registry_id = env.register(RwaRegistry, ());
    let client = RwaRegistryClient::new(&env, &registry_id);
    register_tokens(&env, &client, 3);

    let page = client.get_registered_page(&Some(99), &10);
    assert!(page.tokens.is_empty());
    assert_eq!(page.next_cursor, None);
}

#[test]
fn test_get_registered_page_rejects_invalid_limits() {
    let env = Env::default();

    let registry_id = env.register(RwaRegistry, ());
    let client = RwaRegistryClient::new(&env, &registry_id);

    assert_eq!(
        typed_error(client.try_get_registered_page(&None, &0)),
        RegistryError::InvalidLimit
    );
    assert_eq!(
        typed_error(client.try_get_registered_page(&None, &(MAX_PAGE_SIZE + 1))),
        RegistryError::LimitTooLarge
    );

    // The maximum itself is accepted, even when there is nothing to return.
    let page = client.get_registered_page(&None, &MAX_PAGE_SIZE);
    assert!(page.tokens.is_empty());
    assert_eq!(page.next_cursor, None);
}

#[test]
fn test_get_collateral_info_for_a_registered_token() {
    let env = Env::default();
    env.mock_all_auths();

    let registry_id = env.register(RwaRegistry, ());
    let registry_client = RwaRegistryClient::new(&env, &registry_id);
    let token_id = env.register(MockRwaToken, ());
    let token_client = MockRwaTokenClient::new(&env, &token_id);

    token_client.set(&1_000_0000000, &true);
    token_client.register_self(&registry_id);

    assert_eq!(
        registry_client.get_collateral_info(&token_id),
        Some(CollateralInfo {
            token: token_id.clone(),
            collateral_value: 1_000_0000000,
            is_redeemable: true,
        })
    );
}

#[test]
fn test_get_collateral_info_for_an_unregistered_token_is_none() {
    let env = Env::default();

    let registry_id = env.register(RwaRegistry, ());
    let registry_client = RwaRegistryClient::new(&env, &registry_id);
    let unknown_token = Address::generate(&env);

    assert_eq!(registry_client.get_collateral_info(&unknown_token), None);
}

#[test]
fn test_get_collateral_info_reflects_a_value_changed_since_registration() {
    let env = Env::default();
    env.mock_all_auths();

    let registry_id = env.register(RwaRegistry, ());
    let registry_client = RwaRegistryClient::new(&env, &registry_id);
    let token_id = env.register(MockRwaToken, ());
    let token_client = MockRwaTokenClient::new(&env, &token_id);

    token_client.set(&1_000_0000000, &true);
    token_client.register_self(&registry_id);

    let before = registry_client.get_collateral_info(&token_id).unwrap();
    assert_eq!(before.collateral_value, 1_000_0000000);
    assert!(before.is_redeemable);

    // The view reads the token live, so it must not serve the registration-time
    // values after the token changes them.
    token_client.set(&750_0000000, &false);

    let after = registry_client.get_collateral_info(&token_id).unwrap();
    assert_eq!(after.collateral_value, 750_0000000);
    assert!(!after.is_redeemable);
}

#[test]
fn test_refresh_collateral_info_publishes_the_current_state() {
    let env = Env::default();
    env.mock_all_auths();

    let registry_id = env.register(RwaRegistry, ());
    let token_id = env.register(MockRwaToken, ());
    let token_client = MockRwaTokenClient::new(&env, &token_id);

    token_client.set(&4_200_0000000, &true);
    token_client.register_self(&registry_id);

    // Calls the registry from the token contract itself, so the registry's
    // `token.require_auth()` is satisfied the way a real integration would.
    let info = token_client.publish(&registry_id);

    assert_eq!(
        info,
        CollateralInfo {
            token: token_id.clone(),
            collateral_value: 4_200_0000000,
            is_redeemable: true,
        }
    );

    assert_eq!(
        env.events().all(),
        vec![
            &env,
            (
                registry_id.clone(),
                (Symbol::new(&env, "collateral_info_updated"),).into_val(&env),
                (token_id.clone(), 4_200_0000000i128, true).into_val(&env)
            )
        ]
    );
}

#[test]
fn test_refresh_collateral_info_rejects_an_unregistered_token() {
    let env = Env::default();
    env.mock_all_auths();

    let registry_id = env.register(RwaRegistry, ());
    let registry_client = RwaRegistryClient::new(&env, &registry_id);
    let unknown_token = Address::generate(&env);

    assert_eq!(
        typed_error(registry_client.try_refresh_collateral_info(&unknown_token, &1, &true)),
        RegistryError::NotRegistered
    );
}

#[test]
#[should_panic(expected = "HostError: Error(Auth, InvalidAction)")]
fn test_refresh_collateral_info_requires_token_authorization() {
    let env = Env::default();
    // No auth is mocked, so `token.require_auth()` inside the registry fails.

    let registry_id = env.register(RwaRegistry, ());
    let registry_client = RwaRegistryClient::new(&env, &registry_id);
    let token = Address::generate(&env);

    registry_client.refresh_collateral_info(&token, &1, &true);
}

#[test]
#[should_panic(expected = "HostError: Error(Auth, InvalidAction)")]
fn test_registry_register_unauthorized() {
    let env = Env::default();
    // Do NOT mock auth here to verify it panics

    let registry_id = env.register(RwaRegistry, ());
    let client = RwaRegistryClient::new(&env, &registry_id);

    let token = Address::generate(&env);

    // Should panic due to missing auth
    client.register(&token);
}
