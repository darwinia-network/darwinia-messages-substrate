# Darwinia Bridges Substrate Common

This is a collection of components for building bridges.

These components include Substrate pallets for syncing headers, passing arbitrary messages, as well
as libraries for building relayers to provide cross-chain communication capabilities.

## Contents

- [Installation](#installation)
- [High-Level Architecture](#high-level-architecture)
- [Project Layout](#project-layout)

## Installation

To get up and running you need both stable and nightly Rust. Rust nightly is used to build the Web
Assembly (WASM) runtime for the node. You can configure the WASM support as so:

```bash
rustup install nightly
rustup target add wasm32-unknown-unknown --toolchain nightly
```

Once this is configured you can build and test the repo as follows:

```
git clone https://github.com/darwinia-network/darwinia-messages-substrate.git
cd darwinia-messages-substrate
cargo build --all
cargo test --all
```
## High-Level Architecture

This repo has support for bridging foreign chains together using a combination of Substrate pallets
and external processes called relayers. A bridge chain is one that is able to follow the consensus
of a foreign chain independently. For example, consider the case below where we want to bridge two
Substrate based chains.

```
+---------------+                 +---------------+
|               |                 |               |
|     Crab      |                 |    Darwinia   |
|               |                 |               |
+-------+-------+                 +-------+-------+
        ^                                 ^
        |       +---------------+         |
        |       |               |         |
        +-----> | Bridge Relay  | <-------+
                |               |
                +---------------+
```

The Crab chain must be able to accept Darwinia headers and verify their integrity. It does this by
using a runtime module designed to track GRANDPA finality. Since two blockchains can't interact
directly they need an external service, called a relayer, to communicate. The relayer will subscribe
to new Crab headers via RPC and submit them to the Darwinia chain for verification.

## Project Layout

Here's an overview of how the project is laid out. The `modules` which are used to build the blockchain's logic (a.k.a the runtime) and
the `relays` which are used to pass messages between chains.

```
├── modules         // Substrate Runtime Modules (a.k.a Pallets)
│  ├── grandpa      // On-Chain GRANDPA Light Client
│  ├── messages     // Cross Chain Message Passing
│  ├── dispatch     // Target Chain Message Execution
│  └──  ...
├── primitives      // Code shared between modules, runtimes, and relays
│  └──  ...
├── relays          // Application for sending headers and messages between chains
│  └──  ...
└── scripts         // Useful development and maintenance scripts
```
