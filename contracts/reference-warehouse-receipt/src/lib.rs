#![no_std]
use ossify_rwa_trait::RwaCollateral;
use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env, Error};

/// Storage keys for the reference receipt token.
#[contracttype]
pub enum DataKey {
    /// The single asset record this contract represents.
    Asset,
}

/// The on-chain record behind the warehouse receipt.
///
/// Like the invoice reference token this is a 1:1 model: one contract instance
/// represents one receipt. What differs is the lifecycle. An invoice is a claim
/// on a payment due at a date; a receipt is a claim on goods held in storage.
/// Its value comes from a stored quantity and unit price rather than a single
/// face value, and it is redeemed by the depositor who hands the receipt back
/// to collect the goods, not by the warehouse that issued it.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetRecord {
    /// The warehouse operator that issued the receipt and created the record.
    pub warehouse: Address,
    /// The depositor holding the receipt, and the address allowed to redeem it.
    pub depositor: Address,
    /// Units of goods held in storage.
    pub quantity: u32,
    /// Value of one unit, in 7-decimal units.
    pub unit_price: i128,
    /// Ledger timestamp, in seconds, at which the storage period ends.
    pub storage_expiry: u64,
    /// Set once the depositor collects the goods.
    pub redeemed: bool,
    /// Set once a liquidator takes over the claim after a default.
    pub defaulted: bool,
}

/// Typed failures returned by the receipt token.
///
/// `liquidate` is part of the shared `RwaCollateral` trait, whose signature
/// returns a plain `soroban_sdk::Error`, so its variants are converted to the
/// matching host error code (the `#[contracterror]` discriminant) on the way
/// out. Consumers can convert the host error back with
/// `ContractError::try_from(error)`.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ContractError {
    /// `init` was called on a token that already has an asset record.
    AlreadyInitialized = 1,
    /// The asset record has not been created yet.
    NotInitialized = 2,
    /// The depositor already collected the goods.
    AlreadyRedeemed = 3,
    /// A liquidator already took over the claim.
    AlreadyDefaulted = 4,
    /// The storage period has ended and the receipt can no longer be redeemed.
    PastMaturity = 5,
    /// `quantity` or `unit_price` was not positive, or their product overflows.
    InvalidAmount = 6,
}

fn read_asset(env: &Env) -> Option<AssetRecord> {
    env.storage().instance().get(&DataKey::Asset)
}

fn write_asset(env: &Env, asset: &AssetRecord) {
    env.storage().instance().set(&DataKey::Asset, asset);
}

#[contract]
pub struct WarehouseReceiptToken;

#[contractimpl]
impl WarehouseReceiptToken {
    /// Creates the asset record for this token.
    ///
    /// The `warehouse` must authorize the call, since it is the operator
    /// attesting to the goods in storage. The record can only be created once: a
    /// second call fails with [`ContractError::AlreadyInitialized`] instead of
    /// silently replacing the parties or the valuation.
    pub fn init(
        env: Env,
        warehouse: Address,
        depositor: Address,
        quantity: u32,
        unit_price: i128,
        storage_expiry: u64,
    ) -> Result<(), ContractError> {
        warehouse.require_auth();

        if env.storage().instance().has(&DataKey::Asset) {
            return Err(ContractError::AlreadyInitialized);
        }

        if quantity == 0
            || unit_price <= 0
            || i128::from(quantity).checked_mul(unit_price).is_none()
        {
            return Err(ContractError::InvalidAmount);
        }

        write_asset(
            &env,
            &AssetRecord {
                warehouse,
                depositor,
                quantity,
                unit_price,
                storage_expiry,
                redeemed: false,
                defaulted: false,
            },
        );

        Ok(())
    }

    /// Marks the receipt as redeemed by its depositor, retiring the claim.
    ///
    /// The depositor of the record must authorize the call. A receipt that is
    /// already redeemed, already defaulted, or past its storage expiry is
    /// rejected with the matching [`ContractError`].
    pub fn redeem(env: Env) -> Result<(), ContractError> {
        let mut asset = read_asset(&env).ok_or(ContractError::NotInitialized)?;
        asset.depositor.require_auth();

        if asset.redeemed {
            return Err(ContractError::AlreadyRedeemed);
        }
        if asset.defaulted {
            return Err(ContractError::AlreadyDefaulted);
        }
        if env.ledger().timestamp() >= asset.storage_expiry {
            return Err(ContractError::PastMaturity);
        }

        asset.redeemed = true;
        write_asset(&env, &asset);

        Ok(())
    }

    /// Registers this contract's own address in an Ossify registry.
    ///
    /// The registry checks `token.require_auth()`, which this call satisfies
    /// because the address being registered is the contract's own address.
    pub fn register_self(env: Env, registry: Address) {
        let registry_client = ossify_registry::RwaRegistryClient::new(&env, &registry);
        registry_client.register(&env.current_contract_address());
    }
}

#[contractimpl]
impl RwaCollateral for WarehouseReceiptToken {
    /// Returns the stored quantity multiplied by the stored unit price while the
    /// receipt is live, and `0` once it has been redeemed, has defaulted, or is
    /// past its storage expiry. A token that was never initialized reports `0`
    /// rather than panicking, so consumers can price it without a `try_` call.
    fn collateral_value(env: Env, _token: Address) -> i128 {
        match read_asset(&env) {
            Some(asset) if asset_is_live(&env, &asset) => {
                // `init` rejects values whose product overflows, so this cannot
                // overflow for a record that was written by it.
                i128::from(asset.quantity) * asset.unit_price
            }
            _ => 0,
        }
    }

    /// Returns `true` only while the receipt is live: not redeemed, not
    /// defaulted, and before its storage expiry.
    fn is_redeemable(env: Env, _token: Address) -> bool {
        match read_asset(&env) {
            Some(asset) => asset_is_live(&env, &asset),
            None => false,
        }
    }

    /// Marks the receipt as defaulted and hands the claim to `liquidator`.
    ///
    /// A default is the warehouse failing to produce the stored goods. This
    /// reference model has no bearer balances, so the on-chain effect is the flag
    /// on the asset record: the receipt stops being live for `collateral_value`
    /// and `is_redeemable`. The `liquidator` must authorize the call, since it is
    /// the address taking over the claim. Liquidation stays available after the
    /// storage expiry, which is the case it exists for.
    fn liquidate(env: Env, _token: Address, liquidator: Address) -> Result<(), Error> {
        liquidator.require_auth();

        let mut asset = read_asset(&env).ok_or(Error::from(ContractError::NotInitialized))?;

        if asset.redeemed {
            return Err(ContractError::AlreadyRedeemed.into());
        }
        if asset.defaulted {
            return Err(ContractError::AlreadyDefaulted.into());
        }

        asset.defaulted = true;
        write_asset(&env, &asset);

        Ok(())
    }
}

/// A receipt is live before its storage expiry and while it has neither been
/// redeemed by the depositor nor taken over by a liquidator after a default.
fn asset_is_live(env: &Env, asset: &AssetRecord) -> bool {
    !asset.redeemed && !asset.defaulted && env.ledger().timestamp() < asset.storage_expiry
}

mod test;
