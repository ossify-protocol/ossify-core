# Adopter Guide

This guide is written for Real-World Asset token issuers (e.g., platforms like Invoice Liquidity Network or Kora Protocol) wanting to make their tokens Ossify-compliant. By adopting the `RwaCollateral` standard, your tokens become readable by any consumer protocol that already integrates the registry, without a bilateral integration per token.

Ossify v1 targets 1:1 asset-to-token models, meaning one contract per claim. If your token is a fractionalized or vault-style asset, read "Known Limitations" in [SPEC.md](../SPEC.md) before adopting, because `collateral_value` reports the total pool value rather than a holder's share.

## Step 1: Add the Dependencies

First, add the `ossify-rwa-trait` dependency to your token contract's `Cargo.toml`. Neither crate is published on crates.io, so depend on the repository and pin it to a revision:

```toml
[dependencies]
ossify-rwa-trait = { git = "https://github.com/Aycode01/ossify-core", rev = "e344ccb2a5004a3a7adabd353ddad147be9eafff", package = "ossify-rwa-trait" }
ossify-registry = { git = "https://github.com/Aycode01/ossify-core", rev = "e344ccb2a5004a3a7adabd353ddad147be9eafff", package = "ossify-registry" }
soroban-sdk = "28.0.0"
```

`ossify-registry` is only needed if you use the `register_in_ossify` helper from Step 3. `rev` is pinned so your build does not change when `main` moves; the revision above is the one these instructions were validated against, so bump it when you want newer contract changes. The `package` keys are only needed if you rename a dependency locally.

## Step 2: Implement the `RwaCollateral` Trait

In your token contract, implement the trait for your contract's main struct. You must define three functions: `collateral_value`, `is_redeemable`, and `liquidate`.

```rust
use ossify_rwa_trait::RwaCollateral;
use soroban_sdk::{contractimpl, Address, Env, Error};

#[contractimpl]
impl RwaCollateral for MyInvoiceToken {
    fn collateral_value(env: Env, token: Address) -> i128 {
        // Return the face value or discounted value of your asset.
        // Example: read from your token's internal state.
        Self::get_face_value(&env)
    }

    fn is_redeemable(env: Env, token: Address) -> bool {
        // Return true if the asset is in good standing.
        // Example: check if the invoice is past due or defaulted.
        Self::get_status(&env) != Status::Defaulted
    }

    fn liquidate(env: Env, token: Address, liquidator: Address) -> Result<(), Error> {
        // Implement the logic to transfer ownership or claim rights to the liquidator.
        Self::transfer_claim(&env, liquidator);
        Ok(())
    }
}
```

### A worked example

`contracts/reference-rwa` is a complete 1:1 implementation to read alongside this
guide. It creates its asset record (face value, due date, issuer) in an `init` that
requires the issuer's authorization, and reports a `0` collateral value and `false`
redeemability once the claim has been redeemed, liquidated or passed its due date.
Two details there are worth copying:

- **Typed errors on a trait function.** `liquidate` belongs to the shared trait, whose
  signature returns `Result<(), soroban_sdk::Error>` and cannot be changed, so the
  reference token defines a `#[contracterror]` enum and converts it on the way out
  (`Err(ContractError::AlreadyRedeemed.into())`). Consumers convert the host error back
  with `ContractError::try_from(error)`. Functions that are not part of the trait, such
  as `init` and `redeem` there, can return `Result<_, YourError>` directly.
- **Liquidator authorization.** The reference `liquidate` calls
  `liquidator.require_auth()`, because that address is the one taking over the claim.

## Step 3: Enable Registration

Because the `RwaRegistry` ensures security by requiring the token contract to authorize its own registration (`token.require_auth()`), your contract must provide a way to invoke the registry.

You can add a helper function for your admin to trigger the registration:

```rust
use ossify_registry::RwaRegistryClient;

#[contractimpl]
impl MyInvoiceToken {
    pub fn register_in_ossify(env: Env, registry: Address) {
        // (Optional) Require your admin's auth here to restrict who can register the token.
        // admin.require_auth();
        
        let registry_client = RwaRegistryClient::new(&env, &registry);
        registry_client.register(&env.current_contract_address());
    }
}
```

## Step 4: Deploy and Register

Deploy your token to the Stellar network as usual. Once deployed, invoke your `register_in_ossify` function, passing the official Ossify `RwaRegistry` contract ID.

Your token is now listed in the Ossify `RwaRegistry` and can be read by any consumer protocol that integrates that registry. Note that listing only means your contract authorized its own registration: consumers still need to set their own risk policy, since the registry does not verify your `collateral_value` or `liquidate` logic.
