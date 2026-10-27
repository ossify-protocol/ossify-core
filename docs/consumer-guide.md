# Consumer Guide

This guide is for decentralized applications (DeFi), such as lending pools, AMMs, or insurance protocols, that want to integrate any Ossify-compliant Real-World Asset (RWA).

## Why Integrate Ossify?

Without Ossify, your protocol must implement a custom interface and state-checking logic for every tokenized RWA. By supporting Ossify, you write your integration logic once and can accept any registered token.

## Scope

Ossify v1 targets 1:1 asset-to-token models, where each registered contract represents one claim and `collateral_value` returns that claim's value. For fractionalized or vault-style tokens, `collateral_value` reports the total pool, so your protocol has to measure a holder's share itself. See "Known Limitations" in [SPEC.md](../SPEC.md) and the survey in [RESEARCH.md](../RESEARCH.md).

## Step 1: Add Dependencies

Add the registry and trait clients to your `Cargo.toml`. The crates are not published on crates.io, so depend on the repository and pin it to a revision:

```toml
[dependencies]
ossify-registry = { git = "https://github.com/Aycode01/ossify-core", rev = "e344ccb2a5004a3a7adabd353ddad147be9eafff", package = "ossify-registry" }
ossify-rwa-trait = { git = "https://github.com/Aycode01/ossify-core", rev = "e344ccb2a5004a3a7adabd353ddad147be9eafff", package = "ossify-rwa-trait" }
soroban-sdk = "28.0.0"
```

`rev` is pinned so your build does not change when `main` moves. The revision above is the last one validated against these guides; bump it when you want newer contract changes. The `package` keys are only needed if you rename the dependency locally.

## Step 2: Verify the Token

Before interacting with an RWA, verify it is registered in the official `RwaRegistry`. You can either store the registry address in your contract or pass it as an argument.

```rust
use ossify_registry::RwaRegistryClient;

// Ensure token is registered
let registry_client = RwaRegistryClient::new(&env, &registry_address);
let registered_tokens = registry_client.get_registered();
assert!(registered_tokens.contains(&token_address), "Token is not Ossify-compliant");
```

## Step 3: Check Token Health

Call `is_redeemable` to verify that the underlying asset is in good standing. This protects your protocol from accepting defaulted or locked collateral.

```rust
use ossify_rwa_trait::RwaCollateralClient;

let token_client = RwaCollateralClient::new(&env, &token_address);
assert!(token_client.is_redeemable(&token_address), "Token is defaulted or not redeemable");
```

## Step 4: Value the Collateral

Query the exact value of the asset. Because the value represents a real-world claim (e.g., $1,000 USD), your protocol can use it as the input to its own Loan-to-Value (LTV) calculation. The registry does not verify the reported value, so the risk policy, haircut, and LTV ceiling remain your protocol's decision.

```rust
let collateral_value = token_client.collateral_value(&token_address);

// Example: Lend out 50% of the collateral value
let borrow_amount = collateral_value / 2;
```

## Liquidations

If the position becomes undercollateralized or the borrower defaults, your protocol can call the standardized `liquidate` function to claim the underlying asset.

```rust
// Liquidate and transfer the claim to the liquidator address
token_client.liquidate(&token_address, &liquidator_address);
```

Two things to expect from a compliant token. First, it may require the
`liquidator_address` to authorize the call, since that address is the one taking
over the claim; the reference invoice token does. Pass an address that can sign,
not an arbitrary one. Second, liquidation is a state change on the token, so it can
fail with a typed error when the claim was already redeemed or already liquidated.
Use the `try_` form if you want to handle that failure instead of panicking:

```rust
match token_client.try_liquidate(&token_address, &liquidator_address) {
    Ok(_) => { /* the claim was moved to the liquidator */ }
    Err(Ok(_error)) => { /* the token returned a typed error; see its contracterror enum */ }
    Err(Err(_invoke_error)) => { /* the invocation itself aborted */ }
}
```

## A reference consumer

`contracts/toy-lending-pool` is a small consumer to read alongside this guide. It
covers the whole flow above in one place, and two of its choices are worth copying:

- **Reject with typed errors, not panics.** An unregistered token or a token that
  reports that it is not redeemable returns a `#[contracterror]` variant, so a
  caller can react to the reason instead of parsing a panic message.
- **Record the loan as a position.** It keeps one position per borrower and
  collateral token, keyed in persistent storage, which is what stops the same
  collateral from backing two loans. Repayment clears the position and emits an
  event, so an indexer can follow the flow without polling.
