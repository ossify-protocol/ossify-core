# Ossify Wave Submission Draft

**Project Name**: Ossify
**Repository URL**: [https://github.com/Aycode01/ossify-core](https://github.com/Aycode01/ossify-core)

---

## 1. Project Description
Real-World Asset (RWA) projects on Soroban each use a different on-chain state model, so lending pools and insurance protocols have to build a custom integration for every tokenized claim they want to accept. Ossify targets that fragmentation with two pieces: a shared `RwaCollateral` trait that any RWA token can implement, and an on-chain `RwaRegistry` where token contracts self-register, so consumers can discover compliant tokens and read their collateral value, redemption status, and liquidation path through one interface. Both are written in Rust on Soroban, have unit tests for the registry and the toy lending pool, and are deployed to the Stellar testnet. The scope of v1 is the 1:1 asset-to-token model, meaning one contract per claim, as used by invoice platforms such as Invoice Liquidity Network and Kora Protocol (see [RESEARCH.md](./RESEARCH.md)). Fractionalized and vault-style tokens such as StellarSettle and TradeFlow-Core are not natively supported: `collateral_value` reports the whole pool there, leaving consumers to work out fractional shares themselves. See "Known Limitations" in [SPEC.md](./SPEC.md). Registration also only records that a token authorized its own listing; it is not an audit of that token's valuation or liquidation logic.

## 2. Testnet Contract Deployments (Verified)
The core infrastructure and reference implementations are live on the Stellar Testnet:

- **Registry**: `CB3OM6CSKYXJZUVFJAZWS3IO7JCMSAZSFOCCE35WJLXTUGRI3HJLITY4`
- **Reference RWA (Mock Invoice)**: `CCGFQHBWBLBLRHI33OI7CW6JMKVFAUVB4C372R2KPFZCLQJB2ZGWSYQD`
- **Toy Lending Pool**: `CBFB44ZY6KZP7A3FWPZ5TMMJO3EZO2JWAM5H6IF5P5VPMBEHVEHLOMON`
- **Reference Warehouse Receipt**: `CASMBLVUAUQ6M6WVGZH5RRGLKVKRVXWERG5XP7XSKCPTSPADG7XR5SI2` (registered in the registry above; `get_registered` lists it alongside the reference RWA token)

*(All deployments can be verified on Stellar Expert using the IDs above).*

## 3. Documentation
The full documentation site content is available in the `docs/` directory of the repository, covering:
- Protocol mechanics and standard lifecycle
- Smart contract reference
- Adopter Guide for RWA issuers
- Consumer Guide for DeFi protocols

## 4. Roadmap Issues (Filed)
The following 10 roadmap items are filed as GitHub issues, each with acceptance criteria. See the [issues tab](https://github.com/Aycode01/ossify-core/issues) for the full list.
- **SDK**: [`feat: create ossify-sdk adapter package`](https://github.com/Aycode01/ossify-core/issues/1) for platforms using alternative DTO models (e.g., LiquiFact).
- **Fractional vaults**: [`feat: integrate fractionalized vaults into the standard`](https://github.com/Aycode01/ossify-core/issues/3) and the follow-up [`spike: fractional-share adapter for vault-style RWA tokens`](https://github.com/Aycode01/ossify-core/issues/8).
- **Consumers**: [`feat: build second reference consumer (Insurance Pool)`](https://github.com/Aycode01/ossify-core/issues/2).
- **Reference tokens**: [`feat: add second reference token modelling a warehouse receipt`](https://github.com/Aycode01/ossify-core/issues/7).
- **Registry**: [`feat(registry): add paginated get_registered for large token lists`](https://github.com/Aycode01/ossify-core/issues/4) and [`feat(registry): add get_collateral_info view for indexers`](https://github.com/Aycode01/ossify-core/issues/5).
- **Tests**: [`test(reference-rwa): cover liquidation and redemption logic`](https://github.com/Aycode01/ossify-core/issues/6) and [`test(registry): add property-based tests for register and deregister`](https://github.com/Aycode01/ossify-core/issues/9).
- **Docs**: [`docs: add a worked end-to-end testnet example`](https://github.com/Aycode01/ossify-core/issues/10).

