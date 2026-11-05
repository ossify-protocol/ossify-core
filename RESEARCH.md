# Phase 0 Research: RWA Token Interfaces on Soroban

Before designing the `RwaCollateral` standard, we conducted a targeted survey of several active RWA projects operating on Soroban. The primary goal was to understand their token models and identify areas where a unified collateral interface would both succeed and face friction. 

## Surveyed Projects

### 1. Invoice Liquidity Network (ILN)
ILN tokenizes factored invoices utilizing an "Invoice NFT" model. In this setup, each distinct invoice has its own corresponding tokenized state managed on-chain.
- **Fit with `RwaCollateral`**: Excellent. Because each invoice can logically act as a unique token contract instance, passing `token: Address` cleanly maps to a specific invoice, and methods like `collateral_value` (representing face value) and `is_redeemable` (based on invoice maturity or default state) directly map to the token's logic.

### 2. Kora Protocol
Kora Protocol functions similarly to ILN by tokenizing invoice financing to provide working capital.
- **Fit with `RwaCollateral`**: Excellent. The underlying asset maps cleanly to a singular real-world claim, making integration trivial. 

### 3. LiquiFact
LiquiFact provides invoice liquidity infrastructure using a typed DTO (Data Transfer Object) state machine approach. Instead of a standard token interface handling everything natively, state requests (e.g., transition to "settled") drive the asset's lifecycle.
- **Fit with `RwaCollateral`**: Good, but requires an adapter. A token contract adopting our trait would need to internally translate `is_redeemable` calls into checks against LiquiFact's internal DTO state trackers. It is feasible, but the implementation is not native out of the box.

### 4. StellarSettle & TradeFlow-Core
These models rely more heavily on permissioned token standards (e.g., ERC-3643 or ERC-7943 uRWA concepts translated to Soroban) and fractionalized fungible vaults (similar to ERC-4626). Here, a single token contract holds pooled value belonging to many different fractional owners.
- **Fit with `RwaCollateral`**: Poor without an intermediate layer. The current signature of `collateral_value(env: Env, token: Address)` queries the *total* value of the contract. It lacks a `user` parameter to identify a specific fractional holder's balance. While the standard could be used by a lending pool calling `token_client.balance(user) * (token_client.collateral_value(token) / token_client.total_supply())`, this shifts complex logic onto the lending pool rather than keeping the trait strictly standard. We honestly flag this as a divergence that the current v1 standard does not serve well. The spike below measures the rounding, overflow, staleness and liquidation failure modes of that workaround.

## Spike: Measuring the Fractional-Share Workaround (issue #8)

To decide whether the interface should change, we built a throwaway vault token (a pooled, ERC-4626-style model) and a consumer that applies the workaround above, then ran them against a real Soroban test environment with `soroban-sdk` 28.0.0. The prototype was a scratch crate outside the workspace and is not committed; this section reports what it measured. The vault stores one pooled `collateral_value` and exposes `balance(user)` and `total_supply()`. The consumer reads all three and computes `balance(user) * collateral_value(token) / total_supply()`.

The workaround costs three cross-contract calls per position (balance, total supply, collateral value) instead of the single `collateral_value` call a 1:1 token needs, and the division is integer division.

### Rounding results

All values below are in base units. The prototype asserted each reported value.

| Pool value | Total supply | Holder's shares | Exact share | Reported share | Error |
| --- | --- | --- | --- | --- | --- |
| 10,000,000 | 3 | 1 | 3,333,333.33 | 3,333,333 | 0.00001% under |
| 100 | 7 | 1 | 14.2857 | 14 | 2.0% under |
| 1 | 3 | 1 | 0.3333 | 0 | 100% under |

Three holders of the first pool are each credited 3,333,333, so their sum is 9,999,999 and one base unit belongs to nobody. At seven holders of a 100-unit pool the stranded amount is two units, a 2% under-report of the pool. A holder whose share rounds to zero is invisible as collateral: the consumer cannot distinguish "no position" from "a position too small to value". Rounding down is conservative for a lender, but it is not neutral, and any consumer that rounds up when paying out or checking a liquidation threshold reverses that safety.

### Failure modes

1. **Truncation and granularity.** The error is unbounded relative to the position: it is up to one base unit per holder, which is negligible for a 7-decimal stablecoin pool but severe for low-decimal or low-value assets. It also grows with the number of holders because each share is floored separately.
2. **Multiplication overflow.** `balance(user) * collateral_value` is computed in `i128`. The prototype's overflow test (a holder with 2 shares against a pool value of `i128::MAX`) trapped in the multiplication, not in a recoverable error path. A production consumer needs 256-bit `mul_div` or an equivalent checked implementation; the trait itself gives no hint that this is required.
3. **Stale or self-reported price.** `collateral_value` is only as fresh as the vault's last report. In the prototype, a pool that actually held 4,000,000 but still reported 10,000,000 was credited at 10,000,000, a 150% overstatement. The trait exposes no timestamp, ledger, or oracle reference, so a consumer cannot tell a fresh value from a stale one, and a stale value directly inflates borrowing power.
4. **Non-atomic reads.** The three values come from three separate calls. A deposit, withdrawal, or value update that lands between them produces a mix of states that never existed together (for example, a supply read before a mint and a balance read after it). The trait offers no snapshot view, so the consumer cannot make the read atomic.
5. **Share-price manipulation.** Because the consumer trusts the vault's self-reported pool value, a donation or first-depositor maneuver that moves the share price moves the collateral credit with it. Registration in the registry does not audit this; it only records that the vault authorized its own listing.
6. **Whole-pool liquidation.** `liquidate(token, liquidator)` has no `user` parameter and therefore can only hand over or impair the entire pool. The prototype vault refuses the call rather than seizing every holder's value, and the holder's balance stayed intact. A lending pool that liquidated one defaulting borrower through this trait would wipe out all other holders, so the workaround cannot support the liquidation path at all.
7. **Per-vault redeemability.** `is_redeemable` answers for the vault, not the holder. It cannot express a lockup that applies to one depositor, a KYC or jurisdiction restriction, or the case where the pool is healthy but a particular holder's shares are encumbered. The prototype could only confirm that the vault as a whole reported itself redeemable.

### Options

**Option A: add `user` to the existing trait.** Changing the signature to `collateral_value(env, token, user)` is a breaking change for every implementation, the registry's `get_collateral_info`, every client, and the published documentation. It also only fixes the read: `liquidate` still cannot target a holder, and `is_redeemable` still answers per vault, so the lifecycle mismatch remains. The 1:1 reference tokens would have to accept and ignore a parameter that has no meaning for them.

**Option B: a separate, additive trait.** Keep `RwaCollateral` unchanged and add a holder-scoped interface, for example:

```rust
pub trait FractionalCollateral {
    fn user_collateral_value(env: Env, token: Address, user: Address) -> i128;
    fn is_redeemable_for(env: Env, token: Address, user: Address) -> bool;
    fn liquidate_shares(
        env: Env,
        token: Address,
        liquidator: Address,
        user: Address,
        shares: i128,
    ) -> Result<(), Error>;
}
```

A vault implements both traits: `RwaCollateral` for pool-level discovery and `FractionalCollateral` for position-level credit and liquidation. The registry would need to record which capability a listed token exposes so consumers can branch without a probe call. This is additive, leaves the 1:1 path and every existing adopter untouched, and gives liquidation and redeemability a place to be holder-scoped.

**Option C: keep the status quo and document the adapter.** Consumers keep doing the three-call math with 256-bit arithmetic and their own freshness checks. This is viable for a single, trusted vault integration, but it leaves overflow and staleness as per-consumer responsibilities, cannot fix whole-pool liquidation, and gives the registry no way to describe what a listed token supports.

### Recommendation

Adopt **Option B** for a v2 extension and keep `RwaCollateral` as it is. Fractional support is not just a missing `user` argument: the measured failures include per-holder liquidation and per-holder redeemability, which a `user` parameter on the existing read does not address. A separate trait keeps v1 stable for the 1:1 adopters the standard targets today, while giving vault-style tokens a correct path for the operations that currently cannot be expressed.

Before implementing it, the following need answers:

- How a vault proves value freshness (ledger sequence or timestamp alongside `user_collateral_value`), and whether the registry should require it.
- How share decimals are normalized against the value's base units.
- Whether `liquidate_shares` transfers shares to the liquidator or burns them against the pool, and who may call it.
- How the registry records a token's capability without breaking its current listing schema.

This spike did not change `SPEC.md`; the limitation it documents is the one already stated there.

## Conclusion
The `RwaCollateral` standard fits very cleanly into the 1:1 "Asset = NFT" tokenization models utilized by factoring platforms like ILN and Kora Protocol. However, it struggles to natively capture the state of multi-user vault tokens without forcing lending pools to compute fractional ownership manually. For Phase 1, the standard deliberately targets the NFT-style model as its primary adopter base.
