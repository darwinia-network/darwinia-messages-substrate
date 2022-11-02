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

	pub const EXISTENTIAL_DEPOSIT: Balance = 100 * MICRO;

	pub const SESSION_LENGTH: BlockNumber = 4 * HOURS;
}
pub use copy_paste_from_darwinia::*;

pub use bp_darwinia_core::*;

/// DarwiniaParachain Chain.
pub type DarwiniaParachain = DarwiniaLike;

/// Name of the With-DarwiniaParachain messages pallet instance that is deployed at bridged chains.
pub const WITH_DARWINIA_PARACHAIN_MESSAGES_PALLET_NAME: &str = "BridgeDarwiniaParachainMessages";
