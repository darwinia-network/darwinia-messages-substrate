// This file is part of Substrate.

// Copyright (C) 2020 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Autogenerated weights for darwinia_fee_market
//!
//! THIS FILE WAS AUTO-GENERATED USING THE SUBSTRATE BENCHMARK CLI VERSION 3.0.0
//! DATE: 2021-11-16, STEPS: [100, ], REPEAT: 50, LOW RANGE: [], HIGH RANGE: []
//! EXECUTION: Some(Wasm), WASM-EXECUTION: Compiled, CHAIN: Some("dev"), DB CACHE: 128

// Executed Command:
// ./target/release/drml
// benchmark
// --chain
// dev
// --wasm-execution
// compiled
// --pallet
// darwinia_fee_market
// --execution
// wasm
// --extrinsic
// *
// --steps
// 100
// --repeat
// 50
// --raw
// --heap-pages=4096
// --output=./frame/fee-market/src/weight.rs
// --template=./.maintain/frame-weight-template.hbs

#![allow(unused_parens)]
#![allow(unused_imports)]

use frame_support::{
	traits::Get,
	weights::{constants::RocksDbWeight, Weight},
};
use sp_std::marker::PhantomData;

/// Weight functions needed for darwinia_fee_market.
pub trait WeightInfo {
	fn enroll_and_lock_collateral() -> Weight;
	fn increase_locked_collateral() -> Weight;
	fn decrease_locked_collateral() -> Weight;
	fn update_relay_fee() -> Weight;
	fn cancel_enrollment() -> Weight;
	fn set_slash_protect() -> Weight;
	fn set_assigned_relayers_number() -> Weight;
}

// For backwards compatibility and tests
impl WeightInfo for () {
	fn enroll_and_lock_collateral() -> Weight {
		sp_runtime::traits::Zero::zero()
	}

	fn increase_locked_collateral() -> Weight {
		sp_runtime::traits::Zero::zero()
	}

	fn decrease_locked_collateral() -> Weight {
		sp_runtime::traits::Zero::zero()
	}

	fn update_relay_fee() -> Weight {
		sp_runtime::traits::Zero::zero()
	}

	fn cancel_enrollment() -> Weight {
		sp_runtime::traits::Zero::zero()
	}

	fn set_slash_protect() -> Weight {
		sp_runtime::traits::Zero::zero()
	}

	fn set_assigned_relayers_number() -> Weight {
		sp_runtime::traits::Zero::zero()
	}
}
