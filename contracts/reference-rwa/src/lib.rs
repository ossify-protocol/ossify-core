#![no_std]
use ossify_rwa_trait::RwaCollateral;
use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env, Error};

/// Storage keys for the reference token.
#[contracttype]
pub enum DataKey {
    /// The single asset record this contract represents.
    Asset,
}

/// The on-chain record behind the reference invoice token.
///
/// This reference model is deliberately 1:1: one contract instance holds one
/// invoice, which is the shape `SPEC.md` targets for v1.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetRecord {
    /// Face value of the invoice, in 7-decimal units.
    pub face_value: i128,
    /// Ledger timestamp, in seconds, at which the invoice falls due.
    pub due_date: u64,
    /// Set once the issuer settles the invoice.
    pub redeemed: bool,
    /// Set once a liquidator takes over the claim.
    pub liquidated: bool,
    /// The issuer that created the record, and the address allowed to redeem it.
    pub issuer: Address,
}

/// Typed failures returned by the reference token.
///
/// `liquidate` is part of the shared `RwaCollateral` trait, whose signature
/// returns a plain `soroban_sdk::Error`, so the variants below are converted to
/// the matching host error code (the `#[contracterror]` discriminant) on the way
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
    /// The issuer already settled the claim.
    AlreadyRedeemed = 3,
    /// A liquidator already took over the claim.
    AlreadyLiquidated = 4,
    /// The claim is past its due date and can no longer be redeemed.
    PastDue = 5,
}

fn read_asset(env: &Env) -> Option<AssetRecord> {
    env.storage().instance().get(&DataKey::Asset)
}

fn write_asset(env: &Env, asset: &AssetRecord) {
    env.storage().instance().set(&DataKey::Asset, asset);
}

#[contract]
pub struct MockInvoiceToken;

#[contractimpl]
impl MockInvoiceToken {
    /// Creates the asset record for this token.
    ///
    /// The `issuer` must authorize the call. The record can only be created
    /// once: a second call fails with [`ContractError::AlreadyInitialized`]
    /// rather than silently replacing the issuer or the face value.
    pub fn init(
        env: Env,
        issuer: Address,
        face_value: i128,
        due_date: u64,
    ) -> Result<(), ContractError> {
        issuer.require_auth();

        if env.storage().instance().has(&DataKey::Asset) {
            return Err(ContractError::AlreadyInitialized);
        }

        write_asset(
            &env,
            &AssetRecord {
                face_value,
                due_date,
                redeemed: false,
                liquidated: false,
                issuer,
            },
        );

        Ok(())
    }

    /// Marks the invoice as redeemed (settled) by its issuer, retiring the claim.
    ///
    /// The issuer of the record must authorize the call. A claim that is already
    /// redeemed or liquidated, or that is past its due date, is rejected with the
    /// matching [`ContractError`].
    pub fn redeem(env: Env) -> Result<(), ContractError> {
        let mut asset = read_asset(&env).ok_or(ContractError::NotInitialized)?;
        asset.issuer.require_auth();

        if asset.redeemed {
            return Err(ContractError::AlreadyRedeemed);
        }
        if asset.liquidated {
            return Err(ContractError::AlreadyLiquidated);
        }
        if env.ledger().timestamp() >= asset.due_date {
            return Err(ContractError::PastDue);
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
impl RwaCollateral for MockInvoiceToken {
    /// Returns the stored face value while the claim is live, and `0` once it has
    /// been redeemed, has been liquidated, or is past its due date. A token that
    /// was never initialized reports `0` rather than panicking, so consumers can
    /// price it without a `try_` call.
    fn collateral_value(env: Env, _token: Address) -> i128 {
        match read_asset(&env) {
            Some(asset) if asset_is_live(&env, &asset) => asset.face_value,
            _ => 0,
        }
    }

    /// Returns `true` only while the claim is live: not redeemed, not liquidated,
    /// and before its due date.
    fn is_redeemable(env: Env, _token: Address) -> bool {
        match read_asset(&env) {
            Some(asset) => asset_is_live(&env, &asset),
            None => false,
        }
    }

    /// Marks the asset liquidated and hands the claim to `liquidator`.
    ///
    /// This reference model has no bearer balances, so the on-chain effect of a
    /// liquidation is the flag on the asset record: the claim stops being live
    /// for `collateral_value` and `is_redeemable`. The `liquidator` must
    /// authorize the call, since it is the address taking over the claim.
    fn liquidate(env: Env, _token: Address, liquidator: Address) -> Result<(), Error> {
        liquidator.require_auth();

        let mut asset = read_asset(&env).ok_or(Error::from(ContractError::NotInitialized))?;

        if asset.redeemed {
            return Err(ContractError::AlreadyRedeemed.into());
        }
        if asset.liquidated {
            return Err(ContractError::AlreadyLiquidated.into());
        }

        asset.liquidated = true;
        write_asset(&env, &asset);

        Ok(())
    }
}

/// A claim is live before its due date and while it has neither been redeemed by
/// the issuer nor taken over by a liquidator.
fn asset_is_live(env: &Env, asset: &AssetRecord) -> bool {
    !asset.redeemed && !asset.liquidated && env.ledger().timestamp() < asset.due_date
}

mod test;
