# Smart Contract Reference

## 1. `RwaCollateral` Trait (`contracts/trait`)

This standard interface must be implemented by any compliant RWA token.

### `collateral_value`
- **Signature**: `fn collateral_value(env: Env, token: Address) -> i128`
- **Description**: Returns the real-world value of the token. 
- **Who Can Call**: Any external protocol or user.
- **Triggers**: Called when a consumer protocol needs to determine the asset's worth.

### `is_redeemable`
- **Signature**: `fn is_redeemable(env: Env, token: Address) -> bool`
- **Description**: Returns whether the asset is in good standing (e.g., an invoice hasn't defaulted).
- **Who Can Call**: Any external protocol or user.
- **Triggers**: Called during underwriting or periodic health checks.

### `liquidate`
- **Signature**: `fn liquidate(env: Env, token: Address, liquidator: Address) -> Result<(), Error>`
- **Description**: Executes the liquidation process for the token, transferring the claim to the `liquidator`.
- **Who Can Call**: Typically the consumer protocol holding the collateral.
- **Triggers**: Called when a borrower defaults or a position is liquidated.

## 2. `RwaRegistry` (`contracts/registry`)

The central on-chain registry of compliant tokens.

### `register`
- **Signature**: `pub fn register(env: Env, token: Address)`
- **Description**: Adds a token to the registry. Emits a `register` event.
- **Who Can Call**: The `token` contract itself (enforced via `token.require_auth()`).

### `get_registered`
- **Signature**: `pub fn get_registered(env: Env) -> Vec<Address>`
- **Description**: Returns a list of all currently registered RWA token addresses.
- **Who Can Call**: Anyone.

### `deregister`
- **Signature**: `pub fn deregister(env: Env, token: Address)`
- **Description**: Removes a token from the registry. Emits a `deregister` event.
- **Who Can Call**: The `token` contract itself (enforced via `token.require_auth()`).

## 3. `MockInvoiceToken` (`contracts/reference-rwa`)

A reference implementation of a tokenized factoring invoice. It is a 1:1 model:
one contract instance holds one asset record, created once through `init`.

### `init`
- **Signature**: `pub fn init(env: Env, issuer: Address, face_value: i128, due_date: u64) -> Result<(), ContractError>`
- **Description**: Creates the asset record: `face_value` in 7-decimal units, `due_date` as a ledger timestamp in seconds, the `issuer`, and the `redeemed` and `liquidated` flags, both starting `false`. A second call fails with `AlreadyInitialized` instead of replacing the record.
- **Who Can Call**: The `issuer` address, which must authorize the call.

### `redeem`
- **Signature**: `pub fn redeem(env: Env) -> Result<(), ContractError>`
- **Description**: Marks the invoice as settled by its issuer, retiring the claim. From then on `collateral_value` reports `0` and `is_redeemable` reports `false`, and a later liquidation fails with `AlreadyRedeemed`. Fails with `PastDue` at or after the due date, and with `AlreadyRedeemed`, `AlreadyLiquidated` or `NotInitialized` as applicable.
- **Who Can Call**: The issuer stored by `init`, which must authorize the call.

### `collateral_value`
- **Signature**: `fn collateral_value(env: Env, token: Address) -> i128`
- **Description**: Returns the stored face value while the claim is live, and `0` once it has been redeemed, has been liquidated, or is past its due date. A token that was never initialized also reports `0`.

### `is_redeemable`
- **Signature**: `fn is_redeemable(env: Env, token: Address) -> bool`
- **Description**: `true` only while the claim is live: not redeemed, not liquidated, and before its due date. A token that was never initialized reports `false`.

### `liquidate`
- **Signature**: `fn liquidate(env: Env, token: Address, liquidator: Address) -> Result<(), Error>`
- **Description**: Marks the asset liquidated. This 1:1 model has no bearer balances, so the flag on the record is the whole on-chain effect: the claim stops counting as collateral and stops being redeemable. Liquidation stays available after the due date, which is the default case it exists for, and fails with `AlreadyRedeemed` or `AlreadyLiquidated` otherwise.
- **Who Can Call**: The `liquidator` address, which must authorize the call, since it is the address taking over the claim.

### `register_self`
- **Signature**: `pub fn register_self(env: Env, registry: Address)`
- **Description**: A helper function allowing the contract to authorize its own registration in the `RwaRegistry`.
- **Who Can Call**: Anyone, as it strictly registers the contract's own address.

### `ContractError`

Typed failures, defined with `#[contracterror]`. The shared trait fixes
`liquidate`'s signature to `Result<(), soroban_sdk::Error>`, so that one function
returns the matching host error code, which a consumer converts back with
`ContractError::try_from(error)`. The token's own functions return
`Result<_, ContractError>` directly.

| Variant | Code | Raised when |
| --- | --- | --- |
| `AlreadyInitialized` | 1 | `init` is called on a token that already has an asset record. |
| `NotInitialized` | 2 | `redeem` or `liquidate` is called before `init`. |
| `AlreadyRedeemed` | 3 | The issuer already redeemed the claim. |
| `AlreadyLiquidated` | 4 | A liquidator already took over the claim. |
| `PastDue` | 5 | `redeem` is called at or after the due date. |

## 4. `ToyLendingPool` (`contracts/toy-lending-pool`)

A reference consumer demonstrating how to integrate Ossify-compliant RWAs.

### `borrow_against`
- **Signature**: `pub fn borrow_against(env: Env, registry: Address, token: Address) -> i128`
- **Description**: Mocks borrowing against an RWA token. Validates the token against the registry, checks redeemability, queries its value, and lends out 50%. Emits a `borrow` event.
- **Who Can Call**: Anyone (in this mock implementation).
