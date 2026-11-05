//! Property-based tests for the registry's listing invariants (issue #9).
//!
//! The example-based tests in `src/test.rs` cover one hand-written sequence
//! each. These drive randomized sequences of register and deregister calls and
//! check the invariants after *every* step, including the steps that should do
//! nothing.
//!
//! The runner is seeded explicitly, and proptest documents that the same
//! algorithm and seed produce the same sequence on every system, so a CI
//! failure gives a repeatable case rather than a fresh random one.

use std::cell::Cell;

use ossify_registry::{RwaRegistry, RwaRegistryClient};
use proptest::collection::vec as prop_vec;
use proptest::prelude::*;
use proptest::test_runner::{Config as ProptestConfig, RngAlgorithm, TestRng, TestRunner};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, EnvTestConfig, Events},
    Address, Env, IntoVal, Symbol, Val, Vec,
};

/// How many distinct token addresses the generated operations can target.
///
/// A small pool means the sequences collide often, which is the interesting
/// part: duplicate registrations and deregistrations of missing tokens.
const TOKEN_POOL_SIZE: u32 = 6;

/// The longest operation sequence to generate.
const MAX_OPERATIONS: usize = 24;

/// How many independent sequences to run.
const CASES: u32 = 64;

/// Fixed seed for the runner. Any change here changes every case, so treat it
/// as part of the test, not as a knob to turn when a case fails.
const SEED: [u8; 32] = [
    0x0a, 0x1f, 0x2e, 0x3d, 0x4c, 0x5b, 0x6a, 0x79, 0x88, 0x97, 0xa6, 0xb5, 0xc4, 0xd3, 0xe2, 0xf1,
    0x10, 0x2f, 0x3e, 0x4d, 0x5c, 0x6b, 0x7a, 0x89, 0x98, 0xa7, 0xb6, 0xc5, 0xd4, 0xe3, 0xf2, 0x01,
];

/// Fails if `list` contains the same address at two different positions.
fn assert_no_duplicates(list: &Vec<Address>) -> Result<(), TestCaseError> {
    for left in 0..list.len() {
        for right in (left + 1)..list.len() {
            prop_assert_ne!(
                list.get(left).unwrap(),
                list.get(right).unwrap(),
                "token duplicated at positions {} and {}",
                left,
                right
            );
        }
    }
    Ok(())
}

/// Builds the one event the last invocation should have published, so a call
/// can be compared against it exactly.
fn expected_event(
    env: &Env,
    registry: &Address,
    token: &Address,
    topic: Symbol,
) -> Vec<(Address, Vec<Val>, Val)> {
    let mut expected = Vec::new(env);
    expected.push_back((
        registry.clone(),
        (topic,).into_val(env),
        token.clone().into_val(env),
    ));
    expected
}

#[test]
fn test_random_register_and_deregister_sequences_keep_the_listing_invariants() {
    // Each `u32` is decoded into one operation: the low bit picks register or
    // deregister, the remaining bits pick a token from the pool.
    let operations = prop_vec(any::<u32>(), 0..=MAX_OPERATIONS);

    let mut runner = TestRunner::new_with_rng(
        ProptestConfig {
            cases: CASES,
            ..ProptestConfig::default()
        },
        TestRng::from_seed(RngAlgorithm::ChaCha, &SEED),
    );

    // The no-op paths are the easy ones to stop generating without noticing, so
    // track that the fixed seed still reaches all four branches.
    let saw_new_registration = Cell::new(false);
    let saw_duplicate_registration = Cell::new(false);
    let saw_listed_deregistration = Cell::new(false);
    let saw_missing_deregistration = Cell::new(false);

    runner
        .run(&operations, |operations| {
            let mut env = Env::default();
            // Every case builds its own ledger, so without this the SDK would
            // write one test snapshot file per generated case.
            env.set_config(EnvTestConfig {
                capture_snapshot_at_drop: false,
            });
            env.mock_all_auths();

            let registry_id = env.register(RwaRegistry, ());
            let client = RwaRegistryClient::new(&env, &registry_id);

            let mut pool = Vec::new(&env);
            for _ in 0..TOKEN_POOL_SIZE {
                pool.push_back(Address::generate(&env));
            }

            // The list the registry is expected to hold, in order. Kept as a
            // mirror so the assertions describe membership and ordering
            // independently of how the contract stores the list.
            let mut expected: Vec<Address> = Vec::new(&env);

            for operation in operations {
                let token = pool.get((operation / 2) % TOKEN_POOL_SIZE).unwrap();
                let is_deregister = operation % 2 == 1;

                let before = client.get_registered();
                let was_listed = before.contains(&token);

                prop_assert_eq!(before.len(), expected.len());

                if is_deregister {
                    client.deregister(&token);

                    if was_listed {
                        saw_listed_deregistration.set(true);
                        // Deregistering a listed token removes exactly that one
                        // entry and publishes exactly one event for it.
                        let index = expected.first_index_of(&token).unwrap();
                        expected.remove(index);
                        prop_assert_eq!(before.len(), expected.len() + 1);
                        prop_assert_eq!(
                            env.events().all(),
                            expected_event(
                                &env,
                                &registry_id,
                                &token,
                                Symbol::new(&env, "deregister")
                            )
                        );
                    } else {
                        saw_missing_deregistration.set(true);
                        // A token that is not listed is a no-op: no entry
                        // changes and no event is published.
                        prop_assert_eq!(before.len(), expected.len());
                        prop_assert!(env.events().all().events().is_empty());
                    }
                } else {
                    client.register(&token);

                    if was_listed {
                        saw_duplicate_registration.set(true);
                        // Re-registering a listed token is a no-op, including
                        // its event, so it can never produce a duplicate entry.
                        prop_assert!(env.events().all().events().is_empty());
                    } else {
                        saw_new_registration.set(true);
                        // A new token is appended and publishes one event.
                        expected.push_back(token.clone());
                        prop_assert_eq!(
                            env.events().all(),
                            expected_event(&env, &registry_id, &token, symbol_short!("register"))
                        );
                    }
                }

                let after = client.get_registered();
                prop_assert_eq!(after.len(), client.get_registered_count());
                prop_assert_eq!(&after, &expected);
                assert_no_duplicates(&after)?;
            }

            Ok(())
        })
        .unwrap();

    assert!(
        saw_new_registration.get()
            && saw_duplicate_registration.get()
            && saw_listed_deregistration.get()
            && saw_missing_deregistration.get(),
        "the seeded sequences must exercise every register and deregister branch"
    );
}

#[test]
fn test_deregistering_a_never_registered_token_is_a_silent_noop() {
    let env = Env::default();
    env.mock_all_auths();

    let registry_id = env.register(RwaRegistry, ());
    let client = RwaRegistryClient::new(&env, &registry_id);
    let token = Address::generate(&env);

    client.deregister(&token);

    assert_eq!(client.get_registered_count(), 0);
    assert!(client.get_registered().is_empty());
    // The acceptance criteria call this out specifically: a no-op deregister
    // must not publish a `deregister` event an indexer could act on.
    assert!(env.events().all().events().is_empty());
}
