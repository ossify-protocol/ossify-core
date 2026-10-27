Ossify Core is a set of Soroban smart contracts on Stellar.

<div align="center">
  <img src="./assets/logo.jpg" width="200" alt="Ossify Core Logo" />
</div>

# Ossify Core

[Documentation](https://ossify-protocol.gitbook.io/ossify-core/)

[![Build Status](https://github.com/Aycode01/ossify-core/actions/workflows/ci.yml/badge.svg)](https://github.com/Aycode01/ossify-core/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

Ossify is a shared collateral standard for tokenized Real-World Assets (RWAs). It aims to unify disparate RWA token interfaces (like those for factored invoices) under a single, predictable standard, allowing lending protocols and insurance pools to assess and liquidate any compliant asset through one interface. The standard currently targets 1:1 asset-to-token models; see "Known Limitations" in [SPEC.md](./SPEC.md) for what it does not cover. For full details on the standard's semantic design, read the [SPEC.md](./SPEC.md).

**Maintainer**: Omitogun Ayobami ([@Aycode01](https://github.com/Aycode01))  
**Community**: [Telegram](https://t.me/+KUvycekLJrcxYmM0) | [Discord](https://discord.gg/4w674EYwC)

## Architecture & Layout
- **`contracts/trait/`**: Defines the `RwaCollateral` interface trait.
- **`contracts/registry/`**: A self-registration contract allowing any RWA token to broadcast its compliance with the standard.
- **`contracts/reference-rwa/`**: A reference implementation of a tokenized invoice that stores its asset record (face value, due date, issuer) and models redemption and liquidation.
- **`contracts/toy-lending-pool/`**: A minimal mock lending pool that utilizes the registry and trait to check token collateral values and issue simulated loans.

## Quick Start
To compile the contracts, use the Stellar CLI (v25.2.0+). It performs the spec-shaking step that `soroban-sdk` requires, so a plain `cargo build --target wasm32v1-none --release` will not produce a deployable contract:
```bash
stellar contract build
```

To run the unit tests across the entire workspace:
```bash
cargo test
```

## Contributing
We welcome contributions! Please see our [CONTRIBUTING.md](./CONTRIBUTING.md) for full details on how to get started, claim issues, and submit PRs.

## Contributors
<a href="https://github.com/Aycode01/ossify-core/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=Aycode01/ossify-core" />
</a>
