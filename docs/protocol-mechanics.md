# Protocol Mechanics

This guide explains the full lifecycle of a Real-World Asset (RWA) token as it interacts with the Ossify standard, from initial registration to its use as collateral in a decentralized application.

## 1. Registration

Before an RWA can be accepted by an Ossify-compliant consumer (like a lending pool), it must be registered in the **RwaRegistry**.

1. The token contract itself must authorize its registration.
2. The `register` function is called on the `RwaRegistry` contract, passing the token's address. 
3. The registry checks `token.require_auth()` to ensure the token genuinely intends to be registered.
4. If successful, the registry adds the token to its internal list of compliant RWAs and emits a `register` event.

## 2. Using as Collateral

Once registered, a user can provide their token to a consumer protocol, such as a lending pool.

1. The lending pool receives a request to borrow against the token.
2. The pool queries the `RwaRegistry` via `get_registered()` to verify the token is on the approved list.
3. The pool uses the `RwaCollateral` client to call `is_redeemable(token)` directly on the token contract to ensure the asset is in good standing (e.g., the underlying invoice hasn't defaulted).
4. The pool calls `collateral_value(token)` on the token contract to determine the exact fiat or underlying value of the asset.
5. Using this data, the pool safely calculates the Loan-to-Value (LTV) ratio and extends the loan to the user, emitting a `borrow` event.
6. The consumer records the resulting loan as a position, keyed by borrower and collateral token, so the same collateral cannot be borrowed against twice. A rejection at any step is reported to the caller as a typed error rather than a panic; the reference consumer (`contracts/toy-lending-pool`) does this with a `#[contracterror]` enum and lends out 50% of the reported value.
7. Repayment clears the position, which is what makes the collateral available again. The reference consumer's `repay` returns what was owed and emits a `repay` event.

## 3. Liquidation

If the borrower defaults on their loan or the consumer protocol requires the asset to be liquidated to cover a position:

1. The lending pool calls the `liquidate(token, liquidator)` function defined by the `RwaCollateral` trait.
2. The token contract handles the underlying logic to transfer ownership, update state, or otherwise settle the liquidation in favor of the `liquidator` address.

In the 1:1 reference model (`contracts/reference-rwa`) the liquidation is a flag on
the asset record: the claim then reports a `0` collateral value and `false` for
`is_redeemable`. It requires the `liquidator` to authorize the call, it stays
available after the due date, and it is rejected with a typed error if the claim
was already redeemed or already liquidated. Redemption retires the claim the same
way: once the issuer has settled it, `collateral_value` reports `0`,
`is_redeemable` reports `false`, and a later liquidation fails as
`AlreadyRedeemed`.

## 4. Deregistration

If a token issuer decides to deprecate their asset or withdraw from the Ossify standard, they can remove themselves from the registry.

1. The token contract authorizes its own deregistration.
2. The `deregister` function is called on the `RwaRegistry`, passing the token's address.
3. The registry checks `token.require_auth()`.
4. The token is removed from the registry's active list, and a `deregister` event is emitted. Consumer protocols querying the registry will no longer see the token.
