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
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Darwinia. If not, see <https://www.gnu.org/licenses/>.

//! Benchmarking
#![cfg(feature = "runtime-benchmarks")]

// --- paritytech ---
use frame_benchmarking::{account, benchmarks_instance_pallet};
use frame_support::assert_ok;
use frame_system::RawOrigin;
use sp_runtime::traits::Saturating;
// --- darwinia-network ---
use super::*;
use crate::Pallet as FeeMarket;

const SEED: u32 = 0;
const INIT_RELAYERS_NUMBER: u32 = 10;

fn init_market<T: Config<I>, I: 'static>() {
	let collateral_per_order = T::CollateralPerOrder::get();
	for i in 0..INIT_RELAYERS_NUMBER {
		let relayer: T::AccountId = account("source", i, SEED);
		T::Currency::make_free_balance_be(
			&relayer,
			collateral_per_order.saturating_mul(10u32.into()),
		);

		assert_ok!(<FeeMarket<T, I>>::enroll_and_lock_collateral(
			RawOrigin::Signed(relayer).into(),
			collateral_per_order.saturating_mul(2u32.into()),
			None,
		));
	}
	assert!(<FeeMarket<T, I>>::market_fee().is_some());
}

benchmarks_instance_pallet! {
	enroll_and_lock_collateral {
		init_market::<T, I>();

		let collateral_per_order = T::CollateralPerOrder::get();
		let relayer: T::AccountId = account("source", 100, SEED);
		T::Currency::make_free_balance_be(&relayer, collateral_per_order.saturating_mul(5u32.into()));
	}: enroll_and_lock_collateral(RawOrigin::Signed(relayer.clone()), collateral_per_order.saturating_mul(5u32.into()), None)
	verify {
		assert!(<FeeMarket<T, I>>::is_enrolled(&relayer));
	}

	increase_locked_collateral {
		init_market::<T, I>();

		let relayer: T::AccountId = account("source", 1, SEED);
		let collateral_per_order = T::CollateralPerOrder::get();
	}: increase_locked_collateral(RawOrigin::Signed(relayer.clone()), collateral_per_order.saturating_mul(5u32.into()))
	verify {
		let relayer = <FeeMarket<T, I>>::relayer(&relayer).unwrap();
		assert_eq!(relayer.collateral,  collateral_per_order.saturating_mul(5u32.into()));
	}

	decrease_locked_collateral {
		init_market::<T, I>();

		let relayer: T::AccountId = account("source", 1, SEED);
		let collateral_per_order = T::CollateralPerOrder::get();
	}: decrease_locked_collateral(RawOrigin::Signed(relayer.clone()), collateral_per_order.saturating_mul(1u32.into()))
	verify {
		let relayer = <FeeMarket<T, I>>::relayer(&relayer).unwrap();
		assert_eq!(relayer.collateral,  collateral_per_order.saturating_mul(1u32.into()));
	}

	update_relay_fee {
		init_market::<T, I>();

		let relayer: T::AccountId = account("source", 1, SEED);
	}: update_relay_fee(RawOrigin::Signed(relayer.clone()), T::MinimumRelayFee::get().saturating_mul(2u32.into()))
	verify {
		let relayer = <FeeMarket<T, I>>::relayer(&relayer).unwrap();
		assert_eq!(relayer.fee,  T::MinimumRelayFee::get().saturating_mul(2u32.into()));
	}

	cancel_enrollment {
		init_market::<T, I>();

		let relayer: T::AccountId = account("source", 1, SEED);
	}: cancel_enrollment(RawOrigin::Signed(relayer.clone()))
	verify {
		assert!(!<FeeMarket<T, I>>::is_enrolled(&relayer));
	}

	set_slash_protect {
	}:set_slash_protect(RawOrigin::Root, T::CollateralPerOrder::get().saturating_mul(1u32.into()))

	set_assigned_relayers_number{
		init_market::<T, I>();
	}: set_assigned_relayers_number(RawOrigin::Root, 5)
	verify {
		assert_eq!(<FeeMarket<T, I>>::assigned_relayers().unwrap().len(), 5);
	}
}
