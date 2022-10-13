// This file is part of Darwinia.
//
// Copyright (C) 2018-2022 Darwinia Network
// SPDX-License-Identifier: GPL-3.0
//
// Darwinia is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Darwinia is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Darwinia. If not, see <https://www.gnu.org/licenses/>.

#![cfg_attr(not(feature = "std"), no_std)]

mod copy_paste_from_darwinia {
	// --- darwinia-network ---
	use bp_darwinia_core::*;
	// --- paritytech ---
	use sp_version::RuntimeVersion;

	pub const VERSION: RuntimeVersion = RuntimeVersion {
		spec_name: sp_runtime::create_runtime_str!("Pangolin Parachain Alpha"),
		impl_name: sp_runtime::create_runtime_str!("Pangolin Parachain Alpha"),
		authoring_version: 1,
		spec_version: 3,
		impl_version: 1,
		apis: sp_version::create_apis_vec![[]],
		transaction_version: 1,
		state_version: 0,
	};

	pub const EXISTENTIAL_DEPOSIT: Balance = 0;
}
pub use copy_paste_from_darwinia::*;

pub use bp_darwinia_core::*;

/// PangolinParachain Chain.
pub type PangolinParachain = DarwiniaLike;

/// Name of the With-PangolinParachain GRANDPA pallet instance that is deployed at bridged chains.
pub const WITH_PANGOLIN_PARACHAIN_GRANDPA_PALLET_NAME: &str = "BridgePangolinParachainAlphaGrandpa";
/// Name of the With-PangolinParachain messages pallet instance that is deployed at bridged chains.
pub const WITH_PANGOLIN_PARACHAIN_MESSAGES_PALLET_NAME: &str = "BridgePangolinParachainAlphaMessages";
