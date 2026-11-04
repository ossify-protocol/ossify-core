# Ossify: Shared Collateral Standard for Tokenized RWAs on Soroban

## Purpose
The `RwaCollateral` standard defines a minimal, shared interface for Real-World Asset (RWA) tokens on the Soroban network. Its goal is to provide lending protocols, insurance pools, and other consumers a unified way to assess the collateral value and liquidation state of any compliant RWA, without needing custom integrations for each individual asset project.

## The `RwaCollateral` Trait
Any RWA contract seeking to be utilized as collateral must implement the following trait. 

```rust
pub trait RwaCollateral {
    fn collateral_value(env: Env, token: Address) -> i128;
    fn is_redeemable(env: Env, token: Address) -> bool;
    fn liquidate(env: Env, token: Address, liquidator: Address) -> Result<(), Error>;
}
```

### 1. `collateral_value(env: Env, token: Address) -> i128`
Returns the current valuation of the RWA token.
- **Parameters**: 
  - `token`: The `Address` of the specific RWA contract instance or asset. For singular NFT-style assets (e.g., an individual factored invoice), this is the contract address of the asset itself.
- **Returns**: An `i128` representing the token's value in a standardized base unit (e.g., using 7 decimal places for a stablecoin peg).
- **Semantics**: The RWA contract is responsible for accurately reporting its internal value or fetching it via an oracle. Lending protocols will use this value to calculate borrowing power and collateralization ratios.

### 2. `is_redeemable(env: Env, token: Address) -> bool`
Checks if the asset is in good standing and available for redemption.
- **Parameters**:
  - `token`: The `Address` of the RWA token being queried.
- **Returns**: A boolean indicating whether the asset can currently be redeemed or if it has expired, defaulted, or been locked.
- **Semantics**: A value of `false` implies the RWA is impaired or illiquid. Lending pools should generally reject collateral if `is_redeemable` returns `false`, and may choose to initiate liquidation on existing loans.

### 3. `liquidate(env: Env, token: Address, liquidator: Address) -> Result<(), Error>`
Executes a liquidation, transferring the underlying claim or ownership of the RWA to a designated liquidator.
- **Parameters**:
  - `token`: The `Address` of the RWA token.
  - `liquidator`: The `Address` that should receive ownership or the underlying real-world claim to the asset.
- **Returns**: `Ok(())` on a successful liquidation, or a Soroban `Error` if the process fails (e.g., if the token is not currently eligible for liquidation, or the caller lacks authorization).
- **Semantics**: The caller of this function is typically a lending pool executing a default. Implementers must ensure that this function adheres strictly to their platform's legal and on-chain liquidation policies.

## The RWA Registry
Consumers can query the `RwaRegistry` contract to verify if an RWA contract is listed as compliant with the `RwaCollateral` standard. The registry requires token self-authorization for both registration and deregistration, meaning only the token contract itself (or its explicitly authorized proxy) can manage its listing. This prevents spoofing and impersonation by third parties. However, note that registration simply confirms the token contract authorized being listed; it does not mean its `collateral_value`, `is_redeemable`, or `liquidate` logic has been independently audited or verified as correct.

The registry's primary listing view is paginated: `get_registered_page(cursor, limit)` returns a page of token addresses plus the cursor for the next page, with a maximum page size of 50. `get_registered_count` reports the total, and the original unbounded `get_registered` is retained for backward compatibility but is not intended for large registries.

## Adoption and Compatibility
The `token: Address` parameter is designed to treat the RWA contract itself as the distinct asset, matching the architectural patterns of leading invoice factoring projects where `1 Contract = 1 Invoice`. For platforms using fractionalized fungible vaults, the `token` parameter can be used to pass a specific vault's address, though such platforms should ensure they report the total available pool value, requiring consumers to measure their fractional share independently.

## Known Limitations

The `RwaCollateral` standard targets 1:1 asset-to-token models (such as single invoices) as its primary use case for v1. It does not natively handle fractionalized or vault-style RWA tokens, as `collateral_value` returns the total value of the pool, which pushes the math for fractional ownership onto the consuming lending pool. For a detailed analysis of how this fits with existing ecosystem protocols, please refer to [RESEARCH.md](RESEARCH.md).
