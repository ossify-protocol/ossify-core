#![cfg(test)]

use super::*;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events},
    vec, Address, ConversionError, Env, IntoVal, InvokeError, Symbol, Vec,
};

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
