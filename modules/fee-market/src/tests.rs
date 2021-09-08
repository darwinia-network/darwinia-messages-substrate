// This file is part of Darwinia.
//
// Copyright (C) 2018-2021 Darwinia Network
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

// --- substrate ---
use crate::{self as pallet_bridges_fee_market, *};
use frame_support::{
	assert_err, assert_ok,
	traits::{GenesisBuild, LockIdentifier},
	PalletId,
};
use frame_system::mocking::*;
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
	RuntimeDebug,
};

type Block = MockBlock<Test>;
type UncheckedExtrinsic = MockUncheckedExtrinsic<Test>;
type Balance = u64;

impl frame_system::Config for Test {
	type BaseCallFilter = ();
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type Origin = Origin;
	type Index = u64;
	type BlockNumber = u64;
	type Hash = H256;
	type Call = Call;
	type Hashing = BlakeTwo256;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Header = Header;
	type Event = Event;
	type BlockHashCount = ();
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<Balance>;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
}

frame_support::parameter_types! {
	pub const ExistentialDeposit: u64 = 1;
}

impl pallet_balances::Config for Test {
	type MaxLocks = ();
	type Balance = Balance;
	type DustRemoval = ();
	type Event = Event;
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = frame_system::Pallet<Test>;
	type WeightInfo = ();
	type MaxReserves = ();
	type ReserveIdentifier = ();
}

frame_support::parameter_types! {
	pub const MinimumPeriod: u64 = 1000;
}
impl pallet_timestamp::Config for Test {
	type Moment = u64;
	type OnTimestampSet = ();
	type MinimumPeriod = MinimumPeriod;
	type WeightInfo = ();
}

frame_support::parameter_types! {
	pub const FeeMarketPalletId: PalletId = PalletId(*b"da/feemk");
	pub const FeeMarketLockId: LockIdentifier = *b"da/feelf";
	pub const MiniumLockValue: Balance = 2;
	pub const MinimumFee: Balance = 2;
	pub const PriorRelayersNumber: u64 = 3;
}

impl Config for Test {
	type PalletId = FeeMarketPalletId;
	type Event = Event;
	type MiniumLockValue = MiniumLockValue;
	type MinimumFee = MinimumFee;
	type PriorRelayersNumber = PriorRelayersNumber;
	type LockId = FeeMarketLockId;
	type RingCurrency = Balances;
	type WeightInfo = ();
}

frame_support::construct_runtime! {
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
		Timestamp: pallet_timestamp::{Pallet, Call, Storage},
		Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>},
		FeeMarket: pallet_bridges_fee_market::{Pallet, Call, Storage, Config, Event<T>},
	}
}
pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::default().build_storage::<Test>().unwrap();
	pallet_balances::GenesisConfig::<Test> {
		balances: vec![(1, 10), (2, 20), (3, 30), (4, 40), (5, 50), (12, 10)],
	}
	.assimilate_storage(&mut t)
	.unwrap();

	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

#[test]
fn test_register_workflow_works() {
	new_test_ext().execute_with(|| {
		assert_eq!(Balances::free_balance(1), 10);
		assert_err!(
			FeeMarket::register(Origin::signed(1), 1, None),
			<Error<Test>>::TooLowLockValue
		);
		assert_err!(
			FeeMarket::register(Origin::signed(1), 50, None),
			<Error<Test>>::InsufficientBalance
		);

		assert_ok!(FeeMarket::register(Origin::signed(1), 5, None));
		assert!(FeeMarket::is_registered(&1));
		assert_eq!(FeeMarket::relayers().len(), 1);
		assert_eq!(Balances::usable_balance(&1), 5);
		assert_eq!(FeeMarket::relayer_locked_balance(&1), 5);
		assert_eq!(FeeMarket::best_relayer(), (1, 2));

		assert_err!(
			FeeMarket::register(Origin::signed(1), 5, None),
			<Error<Test>>::AlreadyRegistered
		);
	});
}

#[test]
fn test_relayer_register_update_price() {
	new_test_ext().execute_with(|| {
		assert_ok!(FeeMarket::register(Origin::signed(1), 5, Some(10)));
		assert_ok!(FeeMarket::register(Origin::signed(2), 5, Some(11)));
		assert_ok!(FeeMarket::register(Origin::signed(3), 5, Some(12)));
		assert_ok!(FeeMarket::register(Origin::signed(4), 5, Some(13)));

		assert_eq!(FeeMarket::relayers(), vec![1, 2, 3, 4]);
		assert_eq!(FeeMarket::prior_relayers().len(), 3);
		assert_eq!(FeeMarket::best_relayer(), (3, 12));
	});
}

#[test]
fn test_update_locked_balance_success() {
	new_test_ext().execute_with(|| {
		assert_err!(
			FeeMarket::update_locked_balance(Origin::signed(1), 5),
			<Error::<Test>>::RegisterBeforeUpdateLock
		);
		assert_ok!(FeeMarket::register(Origin::signed(1), 5, None));
		assert!(FeeMarket::is_registered(&1));

		// update lock balance from 5 to 8
		assert_ok!(FeeMarket::update_locked_balance(Origin::signed(1), 8));
		assert_eq!(Balances::usable_balance(&1), 2);
		assert_eq!(FeeMarket::relayer_locked_balance(&1), 8);
		assert_eq!(FeeMarket::best_relayer(), (1, 2));
	});
}

#[test]
fn test_update_locked_balance_failed() {
	new_test_ext().execute_with(|| {
		assert_ok!(FeeMarket::register(Origin::signed(1), 5, None));

		// update lock balance from 5 to 8
		assert_ok!(FeeMarket::update_locked_balance(Origin::signed(1), 8));
		// update lock balance from 8 to 8
		assert_err!(
			FeeMarket::update_locked_balance(Origin::signed(1), 3),
			<Error<Test>>::InvalidNewLockValue
		);
		// update lock balance from 8 to 3
		assert_err!(
			FeeMarket::update_locked_balance(Origin::signed(1), 3),
			<Error<Test>>::InvalidNewLockValue
		);
		assert_eq!(Balances::usable_balance(&1), 2);
		assert_eq!(FeeMarket::relayer_locked_balance(&1), 8);
	});
}

#[test]
fn test_cancel_register() {
	new_test_ext().execute_with(|| {
		assert_err!(
			FeeMarket::cancel_register(Origin::signed(1)),
			<Error<Test>>::RegisterBeforeUpdateLock
		);

		assert_ok!(FeeMarket::register(Origin::signed(1), 5, None));
		assert!(FeeMarket::is_registered(&1));
		assert_eq!(Balances::usable_balance(&1), 5);
		assert_eq!(FeeMarket::relayer_locked_balance(&1), 5);

		assert_ok!(FeeMarket::cancel_register(Origin::signed(1)));
		assert_eq!(FeeMarket::relayer_locked_balance(&1), 0);
		assert!(!FeeMarket::is_registered(&1));
	});
}

#[test]
fn test_cancel_register_and_update_price() {
	new_test_ext().execute_with(|| {
		assert_ok!(FeeMarket::register(Origin::signed(1), 5, None));
		assert_ok!(FeeMarket::register(Origin::signed(2), 5, None));
		assert_ok!(FeeMarket::register(Origin::signed(3), 5, None));
		assert_ok!(FeeMarket::register(Origin::signed(4), 5, None));
		assert_ok!(FeeMarket::register(Origin::signed(5), 5, None));
		assert_eq!(FeeMarket::relayers(), vec![1, 2, 3, 4, 5]);
		assert_eq!(FeeMarket::prior_relayers()[0], (1, 2));
		assert_eq!(FeeMarket::prior_relayers()[1], (2, 2));
		assert_eq!(FeeMarket::prior_relayers()[2], (3, 2));
		assert_eq!(FeeMarket::best_relayer(), (3, 2));

		assert_ok!(FeeMarket::cancel_register(Origin::signed(1)));
		assert_ok!(FeeMarket::cancel_register(Origin::signed(5)));
		assert!(!FeeMarket::is_registered(&1));
		assert!(!FeeMarket::is_registered(&5));
		assert_eq!(FeeMarket::relayers(), vec![2, 3, 4]);
		assert_eq!(FeeMarket::prior_relayers()[0], (2, 2));
		assert_eq!(FeeMarket::prior_relayers()[1], (3, 2));
		assert_eq!(FeeMarket::prior_relayers()[2], (4, 2));
		assert_eq!(FeeMarket::best_relayer(), (4, 2));
	});
}

#[test]
fn test_locked_ring_list_works() {
	new_test_ext().execute_with(|| {
		assert_ok!(FeeMarket::register(Origin::signed(1), 5, None));
		assert_ok!(FeeMarket::register(Origin::signed(2), 10, None));
		assert_ok!(FeeMarket::register(Origin::signed(3), 15, None));
		assert_ok!(FeeMarket::register(Origin::signed(4), 20, None));

		assert_eq!(FeeMarket::relayer_locked_balance(&1), 5);
		assert_eq!(FeeMarket::relayer_locked_balance(&2), 10);
		assert_eq!(FeeMarket::relayer_locked_balance(&3), 15);
		assert_eq!(FeeMarket::relayer_locked_balance(&4), 20);

		assert_ok!(FeeMarket::update_locked_balance(Origin::signed(1), 6));
		assert_ok!(FeeMarket::update_locked_balance(Origin::signed(2), 11));
		assert_ok!(FeeMarket::update_locked_balance(Origin::signed(3), 16));
		assert_ok!(FeeMarket::update_locked_balance(Origin::signed(4), 21));

		assert_eq!(FeeMarket::relayer_locked_balance(&1), 6);
		assert_eq!(FeeMarket::relayer_locked_balance(&2), 11);
		assert_eq!(FeeMarket::relayer_locked_balance(&3), 16);
		assert_eq!(FeeMarket::relayer_locked_balance(&4), 21);

		assert_ok!(FeeMarket::cancel_register(Origin::signed(1)));
		assert_ok!(FeeMarket::cancel_register(Origin::signed(2)));
		assert_ok!(FeeMarket::cancel_register(Origin::signed(3)));
		assert_ok!(FeeMarket::cancel_register(Origin::signed(4)));

		assert_eq!(FeeMarket::relayer_locked_balance(&1), 0);
		assert_eq!(FeeMarket::relayer_locked_balance(&2), 0);
		assert_eq!(FeeMarket::relayer_locked_balance(&3), 0);
		assert_eq!(FeeMarket::relayer_locked_balance(&4), 0);
	});
}

#[test]
fn test_update_price_basic_storage_works() {
	new_test_ext().execute_with(|| {
		assert_ok!(FeeMarket::register(Origin::signed(1), 5, None));
		assert_err!(FeeMarket::update_fee(Origin::signed(1), 1), <Error<Test>>::TooLowFee);

		assert_ok!(FeeMarket::update_fee(Origin::signed(1), 2));
		assert_eq!(FeeMarket::relayer_price(&1), 2);
		assert_eq!(FeeMarket::relayers(), vec![1]);
	});
}

#[test]
fn test_few_relayer_duplicate_update_one_price() {
	new_test_ext().execute_with(|| {
		assert_ok!(FeeMarket::register(Origin::signed(1), 5, None));
		assert_ok!(FeeMarket::update_fee(Origin::signed(1), 2));
		assert_ok!(FeeMarket::update_fee(Origin::signed(1), 2));

		assert_eq!(FeeMarket::relayers(), vec![1]);
		assert_eq!(FeeMarket::prior_relayers()[0], (1, 2));
		assert_eq!(FeeMarket::prior_relayers().len(), 1);
		assert_eq!(FeeMarket::best_relayer(), (1, 2));
	});
}

#[test]
fn test_few_relayer_update_one_price() {
	new_test_ext().execute_with(|| {
		assert_ok!(FeeMarket::register(Origin::signed(1), 5, None));
		assert_ok!(FeeMarket::register(Origin::signed(2), 5, None));
		assert_ok!(FeeMarket::update_fee(Origin::signed(1), 4));
		assert_ok!(FeeMarket::update_fee(Origin::signed(2), 4));

		assert_eq!(FeeMarket::prior_relayers()[0], (1, 4));
		assert_eq!(FeeMarket::prior_relayers()[1], (2, 4));
		assert_eq!(FeeMarket::prior_relayers().len(), 2);
		assert_eq!(FeeMarket::best_relayer(), (2, 4));
	});
}

#[test]
fn test_few_relayer_update_more_price() {
	new_test_ext().execute_with(|| {
		assert_ok!(FeeMarket::register(Origin::signed(1), 5, None));
		assert_ok!(FeeMarket::register(Origin::signed(2), 5, None));
		assert_ok!(FeeMarket::update_fee(Origin::signed(1), 2));
		assert_ok!(FeeMarket::update_fee(Origin::signed(2), 3));

		assert_eq!(FeeMarket::relayers(), vec![1, 2]);
		assert_eq!(FeeMarket::prior_relayers()[0], (1, 2));
		assert_eq!(FeeMarket::prior_relayers()[1], (2, 3));
		assert_eq!(FeeMarket::prior_relayers().len(), 2);
		assert_eq!(FeeMarket::best_relayer(), (2, 3));
	});
}

#[test]
fn test_mul_relayer_update_one_price() {
	new_test_ext().execute_with(|| {
		assert_ok!(FeeMarket::register(Origin::signed(1), 5, None));
		assert_ok!(FeeMarket::register(Origin::signed(2), 5, None));
		assert_ok!(FeeMarket::register(Origin::signed(3), 5, None));
		assert_ok!(FeeMarket::register(Origin::signed(4), 5, None));
		assert_ok!(FeeMarket::update_fee(Origin::signed(1), 10));
		assert_ok!(FeeMarket::update_fee(Origin::signed(2), 10));
		assert_ok!(FeeMarket::update_fee(Origin::signed(3), 10));
		assert_ok!(FeeMarket::update_fee(Origin::signed(4), 10));

		assert_eq!(FeeMarket::relayers(), vec![1, 2, 3, 4]);
		assert_eq!(FeeMarket::prior_relayers().len(), 3);
		assert_eq!(FeeMarket::prior_relayers()[0], (1, 10));
		assert_eq!(FeeMarket::prior_relayers()[1], (2, 10));
		assert_eq!(FeeMarket::prior_relayers()[2], (3, 10));
		assert_eq!(FeeMarket::best_relayer(), (3, 10));
	});
}

#[test]
fn test_mul_relayer_update_diff_price() {
	new_test_ext().execute_with(|| {
		assert_ok!(FeeMarket::register(Origin::signed(1), 5, None));
		assert_ok!(FeeMarket::register(Origin::signed(2), 5, None));
		assert_ok!(FeeMarket::register(Origin::signed(3), 5, None));
		assert_ok!(FeeMarket::register(Origin::signed(4), 5, None));
		assert_ok!(FeeMarket::update_fee(Origin::signed(1), 10));
		assert_ok!(FeeMarket::update_fee(Origin::signed(2), 20));
		assert_ok!(FeeMarket::update_fee(Origin::signed(3), 30));
		assert_ok!(FeeMarket::update_fee(Origin::signed(4), 40));

		assert_eq!(FeeMarket::relayers(), vec![1, 2, 3, 4]);
		assert_eq!(FeeMarket::prior_relayers().len(), 3);
		assert_eq!(FeeMarket::prior_relayers()[0], (1, 10));
		assert_eq!(FeeMarket::prior_relayers()[1], (2, 20));
		assert_eq!(FeeMarket::prior_relayers()[2], (3, 30));
		assert_eq!(FeeMarket::best_relayer(), (3, 30));
	});
}