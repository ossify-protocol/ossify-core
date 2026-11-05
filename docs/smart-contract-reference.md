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
- **Description**: Returns a list of all currently registered RWA token addresses. This function is kept for backward compatibility, but it is unbounded: it reads and returns the whole list in one invocation, so it is not suitable for a large registry. New callers should use `get_registered_page` and size their response with `get_registered_count`.
- **Who Can Call**: Anyone.

### `get_registered_count`
- **Signature**: `pub fn get_registered_count(env: Env) -> u32`
- **Description**: Returns the number of registered token addresses.
- **Who Can Call**: Anyone.

### `get_registered_page`
- **Signature**: `pub fn get_registered_page(env: Env, cursor: Option<u32>, limit: u32) -> Result<RegistrationPage, RegistryError>`
- **Description**: Returns one page of registered token addresses in registration order. Pass `cursor: None` for the first page, then the `next_cursor` from the previous page. `limit` must be between `1` and `MAX_PAGE_SIZE` (50). A cursor at or past the end of the list returns an empty page with `next_cursor: None` rather than an error. `RegistrationPage` holds `tokens: Vec<Address>` and `next_cursor: Option<u32>`.
- **Who Can Call**: Anyone.

### `get_collateral_info`
- **Signature**: `pub fn get_collateral_info(env: Env, token: Address) -> Option<CollateralInfo>`
- **Description**: Returns one aggregate view of a registered token: `CollateralInfo { token, collateral_value, is_redeemable }`. The values are read live through the token's `RwaCollateral` implementation, so they reflect the token as of the current ledger. Returns `None` for an unregistered token. A registered token that traps in either trait call makes this call trap as well; the registry does not price a broken token as zero.
- **Who Can Call**: Anyone.

### `refresh_collateral_info`
- **Signature**: `pub fn refresh_collateral_info(env: Env, token: Address, collateral_value: i128, is_redeemable: bool) -> Result<CollateralInfo, RegistryError>`
- **Description**: Publishes the values a registered token reports for itself as a `collateral_info_updated` event, and returns the same `CollateralInfo`. The token must authorize the call. It supplies the values because Soroban rejects contract re-entry: a token calling here is already on the call stack, so the registry cannot read it back in the same invocation. The registry does not verify the supplied values; `get_collateral_info` remains the authoritative live read. Fails with `NotRegistered` for an unlisted token.
- **Who Can Call**: The `token` contract itself (enforced via `token.require_auth()`).

### `deregister`
- **Signature**: `pub fn deregister(env: Env, token: Address)`
- **Description**: Removes a token from the registry. Emits a `deregister` event.
- **Who Can Call**: The `token` contract itself (enforced via `token.require_auth()`).

### Events

| Event | Topic | Data |
| --- | --- | --- |
| `Register` | `register` | `token` |
| `Deregister` | `deregister` | `token` |
| `CollateralInfoUpdated` | `collateral_info_updated` | `(token, collateral_value, is_redeemable)` |

### `RegistryError`

Typed failures returned by `get_registered_page` and `refresh_collateral_info`, defined with `#[contracterror]`.

| Variant | Code | Raised when |
| --- | --- | --- |
| `InvalidLimit` | 1 | `limit` is zero. |
| `LimitTooLarge` | 2 | `limit` is greater than `MAX_PAGE_SIZE` (50). |
| `NotRegistered` | 3 | `refresh_collateral_info` is called for a token that is not listed. |

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

A reference consumer demonstrating how to integrate Ossify-compliant RWAs. It
records one position per borrower and collateral token, so the same collateral
cannot back two loans at once, and no settlement asset moves: a loan is the
stored position and repayment is that position being cleared.

### `borrow_against`
- **Signature**: `pub fn borrow_against(env: Env, borrower: Address, registry: Address, token: Address) -> Result<i128, ContractError>`
- **Description**: Validates the token against the registry, checks redeemability, queries its value through the trait client, stores a `Position` for the borrower and lends out 50% of the reported value. Fails with `NotRegistered`, `NotRedeemable` or `PositionAlreadyOpen` instead of panicking. Emits a `borrow` event.
- **Who Can Call**: The `borrower` address, which must authorize the call.

### `repay`
- **Signature**: `pub fn repay(env: Env, borrower: Address, token: Address) -> Result<i128, ContractError>`
- **Description**: Clears the borrower's open position in full and returns the amount owed, which frees the collateral to be borrowed against again. Fails with `NoOpenPosition` when there is nothing to repay. Emits a `repay` event.
- **Who Can Call**: The `borrower` address, which must authorize the call.

### `get_position`
- **Signature**: `pub fn get_position(env: Env, borrower: Address, token: Address) -> Option<Position>`
- **Description**: Returns the borrower's open position against `token`, or `None`. The `Position` holds the collateral value read when the loan was issued and the amount owed.
- **Who Can Call**: Anyone.

### Events

| Event | Topic | Data |
| --- | --- | --- |
| `Borrow` | `borrow` | `(borrower, token, amount_lent)` |
| `Repay` | `repay` | `(borrower, token, amount_repaid)` |

Both publish a list as their data, the shape the pool published before it stored
positions; `Borrow` gained the borrower in that change.

### `ContractError`

| Variant | Code | Raised when |
| --- | --- | --- |
| `NotRegistered` | 1 | The token is not listed in the registry the pool was pointed at. |
| `NotRedeemable` | 2 | The token reports that it is not currently redeemable. |
| `PositionAlreadyOpen` | 3 | The borrower already has an open position against this collateral. |
| `NoOpenPosition` | 4 | `repay` is called without an open position for this borrower and token. |

### Storage and limits

Positions are held in persistent storage keyed by borrower and token, and the
entry's TTL is extended when it is created (see `POSITION_TTL_THRESHOLD` and
`POSITION_TTL_EXTEND_TO`), so an outstanding loan cannot be archived out from
under the borrower. This is still a mock: there is no interest, no partial
repayment, no settlement asset and no LTV health check.

## 5. `WarehouseReceiptToken` (`contracts/reference-warehouse-receipt`)

A second reference implementation, modelling a warehouse receipt for goods held
in storage. It is a 1:1 model like the invoice token, but the value is a stored
`quantity * unit_price` rather than a face value, the depositor holding the
receipt authorizes redemption, and the claim expires at a storage expiry rather
than a payment due date. Registering it in the same registry as the invoice token
is covered by `test_registry_lists_both_reference_tokens`.

### `init`
- **Signature**: `pub fn init(env: Env, warehouse: Address, depositor: Address, quantity: u32, unit_price: i128, storage_expiry: u64) -> Result<(), ContractError>`
- **Description**: Creates the asset record: the warehouse operator, the depositor, `quantity` of goods, `unit_price` in 7-decimal units, `storage_expiry` as a ledger timestamp in seconds, and the `redeemed` and `defaulted` flags, both starting `false`. Rejects a zero quantity, a non-positive unit price, or a product that overflows `i128` with `InvalidAmount`. A second call fails with `AlreadyInitialized` instead of replacing the record.
- **Who Can Call**: The `warehouse` address, which must authorize the call.

### `redeem`
- **Signature**: `pub fn redeem(env: Env) -> Result<(), ContractError>`
- **Description**: Marks the receipt as collected by its depositor, retiring the claim. From then on `collateral_value` reports `0` and `is_redeemable` reports `false`. Fails with `PastMaturity` at or after the storage expiry, and with `AlreadyRedeemed`, `AlreadyDefaulted` or `NotInitialized` as applicable.
- **Who Can Call**: The `depositor` stored by `init`, which must authorize the call.

### `collateral_value`
- **Signature**: `fn collateral_value(env: Env, token: Address) -> i128`
- **Description**: Returns the stored `quantity * unit_price` while the receipt is live, and `0` once it has been redeemed, has defaulted, is past its storage expiry, or was never initialized.

### `is_redeemable`
- **Signature**: `fn is_redeemable(env: Env, token: Address) -> bool`
- **Description**: `true` only while the receipt is live: not redeemed, not defaulted, and before its storage expiry.

### `liquidate`
- **Signature**: `fn liquidate(env: Env, token: Address, liquidator: Address) -> Result<(), Error>`
- **Description**: Marks the receipt defaulted, which is the warehouse failing to produce the stored goods. Liquidation stays available after the storage expiry, which is the default case it exists for, and fails with `AlreadyRedeemed` or `AlreadyDefaulted` otherwise.
- **Who Can Call**: The `liquidator` address, which must authorize the call, since it is the address taking over the claim.

### `register_self`
- **Signature**: `pub fn register_self(env: Env, registry: Address)`
- **Description**: A helper function allowing the contract to authorize its own registration in the `RwaRegistry`.
- **Who Can Call**: Anyone, as it strictly registers the contract's own address.

### `ContractError`

| Variant | Code | Raised when |
| --- | --- | --- |
| `AlreadyInitialized` | 1 | `init` is called on a token that already has an asset record. |
| `NotInitialized` | 2 | `redeem` or `liquidate` is called before `init`. |
| `AlreadyRedeemed` | 3 | The depositor already collected the goods. |
| `AlreadyDefaulted` | 4 | A liquidator already took over the claim. |
| `PastMaturity` | 5 | `redeem` is called at or after the storage expiry. |
| `InvalidAmount` | 6 | `quantity` or `unit_price` is not positive, or their product overflows `i128`. |

### Testnet deployment

Deployed on 2026-11-05 at `CASMBLVUAUQ6M6WVGZH5RRGLKVKRVXWERG5XP7XSKCPTSPADG7XR5SI2`
with `quantity = 500`, `unit_price = 20000000` and storage expiry `1800000000`, and
registered in the registry at
`CB3OM6CSKYXJZUVFJAZWS3IO7JCMSAZSFOCCE35WJLXTUGRI3HJLITY4`. On that registry,
`get_registered` returns both the invoice token
(`CCGFQHBWBLBLRHI33OI7CW6JMKVFAUVB4C372R2KPFZCLQJB2ZGWSYQD`) and this receipt, and
the token's `collateral_value` and `is_redeemable` answered `10000000000` and
`true`. That registry was deployed before the pagination and collateral-info
views existed, so `get_registered_count` and `get_collateral_info` are not yet
callable on it.
