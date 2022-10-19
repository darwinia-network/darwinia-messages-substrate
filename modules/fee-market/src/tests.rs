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

// --- std ---
use std::collections::BTreeMap;
// --- paritytech ---
use bp_messages::{InboundLaneData, UnrewardedRelayersState};
use frame_support::{assert_err, assert_ok, traits::OnFinalize};
use sp_runtime::{traits::AccountIdConversion, DispatchError, ModuleError};
// --- darwinia-network ---
use crate::{
	assert_market_storage, assert_relayer_info,
	mock::{
		receive_messages_delivery_proof, send_regular_message, unrewarded_relayer, AccountId,
		Balances, Event, ExtBuilder, FeeMarket, Messages, Origin, System, Test,
		TestMessageDeliveryAndDispatchPayment, TestMessagesDeliveryProof, REGULAR_PAYLOAD,
		TEST_LANE_ID, TEST_RELAYER_A, TEST_RELAYER_B,
	},
	Config, Error, RewardItem, SlashReport,
};

// enroll_and_lock_collateral

#[test]
fn test_enroll_failed_with_insuffience_balance() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	ExtBuilder::default().with_balances(vec![(1, collater_per_order)]).build().execute_with(|| {
		assert_err!(
			FeeMarket::enroll_and_lock_collateral(Origin::signed(1), collater_per_order + 1, None),
			<Error<Test>>::InsufficientBalance
		);
	});
}

#[test]
fn test_enroll_failed_if_collateral_too_low() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	ExtBuilder::default().with_balances(vec![(1, collater_per_order)]).build().execute_with(|| {
		assert_err!(
			FeeMarket::enroll_and_lock_collateral(Origin::signed(1), collater_per_order - 1, None),
			<Error<Test>>::CollateralTooLow
		);
	});
}

#[test]
fn test_enroll_with_default_quota() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	ExtBuilder::default().with_balances(vec![(1, collater_per_order)]).build().execute_with(|| {
		assert_ok!(FeeMarket::enroll_and_lock_collateral(
			Origin::signed(1),
			collater_per_order,
			None
		));
		assert_eq!(FeeMarket::relayer(&1).unwrap().fee, <Test as Config>::MinimumRelayFee::get());
	});
}

#[test]
fn test_enroll_failed_if_quota_too_low() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	ExtBuilder::default().with_balances(vec![(1, collater_per_order)]).build().execute_with(|| {
		assert_err!(
			FeeMarket::enroll_and_lock_collateral(
				Origin::signed(1),
				collater_per_order,
				Some(<Test as Config>::MinimumRelayFee::get() - 1),
			),
			<Error<Test>>::RelayFeeTooLow
		);
	});
}

#[test]
fn test_enroll_with_correct_balance_changes() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	let init_balance = collater_per_order + 20;
	ExtBuilder::default()
		.with_balances(vec![(1, init_balance)])
		.with_relayers(vec![(1, collater_per_order, None)])
		.build()
		.execute_with(|| {
			assert_err!(
				FeeMarket::enroll_and_lock_collateral(Origin::signed(1), collater_per_order, None),
				<Error<Test>>::AlreadyEnrolled
			);

			assert_relayer_info! {
				"account_id": 1,
				"free_balance": init_balance,
				"usable_balance": init_balance - collater_per_order,
				"is_enrolled": true,
				"collateral": collater_per_order,
				"order_capacity":1,
			}

			assert_market_storage! {
				"relayers": vec![1],
				"assigned_relayers": Vec::<u64>::new(),
				"market_fee": None,
			}
		});
}

#[test]
fn test_enroll_again_failed() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	ExtBuilder::default()
		.with_balances(vec![(1, collater_per_order)])
		.with_relayers(vec![(1, collater_per_order, None)])
		.build()
		.execute_with(|| {
			assert_err!(
				FeeMarket::enroll_and_lock_collateral(Origin::signed(1), collater_per_order, None),
				<Error<Test>>::AlreadyEnrolled
			);
		});
}

// increase_locked_collateral

#[test]
fn test_increase_collateral_with_insuffience_balance() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	ExtBuilder::default()
		.with_balances(vec![(1, collater_per_order)])
		.with_relayers(vec![(1, collater_per_order, None)])
		.build()
		.execute_with(|| {
			assert_err!(
				FeeMarket::increase_locked_collateral(Origin::signed(1), collater_per_order + 1),
				<Error<Test>>::InsufficientBalance
			);
		});
}

#[test]
fn test_increase_collateral_not_enrolled() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	ExtBuilder::default().with_balances(vec![(1, collater_per_order)]).build().execute_with(|| {
		assert_err!(
			FeeMarket::increase_locked_collateral(Origin::signed(1), collater_per_order),
			<Error<Test>>::NotEnrolled
		);
	});
}

#[test]
fn test_increase_collateral_new_collateral_less_than_before() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	ExtBuilder::default()
		.with_balances(vec![(1, collater_per_order)])
		.with_relayers(vec![(1, collater_per_order, None)])
		.build()
		.execute_with(|| {
			assert_err!(
				FeeMarket::increase_locked_collateral(Origin::signed(1), collater_per_order - 1),
				<Error<Test>>::NewCollateralShouldLargerThanBefore
			);
		});
}

#[test]
fn test_increase_collateral_relayer_balance_update_correctly() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	let init_balance = collater_per_order + 20;
	ExtBuilder::default()
		.with_balances(vec![(1, init_balance)])
		.with_relayers(vec![(1, collater_per_order, None)])
		.build()
		.execute_with(|| {
			assert_relayer_info! {
				"account_id": 1,
				"free_balance": init_balance,
				"usable_balance": init_balance - collater_per_order,
				"is_enrolled": true,
				"collateral": collater_per_order,
				"order_capacity":1,
			}

			assert_market_storage! {
				"relayers": vec![1],
				"assigned_relayers": Vec::<u64>::new(),
				"market_fee": None,
			}

			assert_ok!(FeeMarket::increase_locked_collateral(
				Origin::signed(1),
				collater_per_order + 10
			));
			assert_relayer_info! {
				"account_id": 1,
				"free_balance": init_balance,
				"usable_balance": init_balance - collater_per_order - 10,
				"is_enrolled": true,
				"collateral": collater_per_order + 10,
				"order_capacity":1,
			}
		});
}

// decrease_locked_collateral

#[test]
fn test_decrease_collateral_with_insuffience_balance() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	ExtBuilder::default()
		.with_balances(vec![(1, collater_per_order)])
		.with_relayers(vec![(1, collater_per_order, None)])
		.build()
		.execute_with(|| {
			assert_err!(
				FeeMarket::decrease_locked_collateral(Origin::signed(1), collater_per_order + 1),
				<Error<Test>>::InsufficientBalance
			);
		});
}

#[test]
fn test_decrease_collateral_not_enrolled() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	ExtBuilder::default().with_balances(vec![(1, collater_per_order)]).build().execute_with(|| {
		assert_err!(
			FeeMarket::decrease_locked_collateral(Origin::signed(1), collater_per_order),
			<Error<Test>>::NotEnrolled
		);
	});
}

#[test]
fn test_decrease_collateral_new_collateral_more_than_before() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	ExtBuilder::default()
		.with_balances(vec![(1, collater_per_order * 2)])
		.with_relayers(vec![(1, collater_per_order, None)])
		.build()
		.execute_with(|| {
			assert_err!(
				FeeMarket::decrease_locked_collateral(Origin::signed(1), collater_per_order + 1),
				<Error<Test>>::NewCollateralShouldLessThanBefore
			);
		});
}

#[test]
fn test_decrease_collateral_are_not_allowed_when_occupied() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	let default_fee = <Test as Config>::MinimumRelayFee::get();
	ExtBuilder::default()
		.with_balances(vec![
			(1, collater_per_order * 3),
			(2, collater_per_order * 3),
			(3, collater_per_order * 3),
		])
		.with_relayers(vec![
			(1, collater_per_order * 2, None),
			(2, collater_per_order * 2, None),
			(3, collater_per_order * 2, None),
		])
		.build()
		.execute_with(|| {
			assert_market_storage! {
				"relayers": vec![1, 2, 3],
				"assigned_relayers": vec![1, 2, 3],
				"market_fee": Some(default_fee),
			}
			assert_relayer_info! {
				"account_id": 1,
				"free_balance": collater_per_order * 3,
				"usable_balance": collater_per_order,
				"is_enrolled": true,
				"collateral": collater_per_order * 2,
				"order_capacity": 2,
			}
			assert_relayer_info! {
				"account_id": 2,
				"free_balance": collater_per_order * 3,
				"usable_balance": collater_per_order,
				"is_enrolled": true,
				"collateral": collater_per_order * 2,
				"order_capacity": 2,
			}
			assert_relayer_info! {
				"account_id": 3,
				"free_balance": collater_per_order * 3,
				"usable_balance": collater_per_order,
				"is_enrolled": true,
				"collateral": collater_per_order * 2,
				"order_capacity": 2,
			}

			let _ = send_regular_message(1, default_fee);
			let _ = send_regular_message(1, default_fee);

			assert_err!(
				FeeMarket::decrease_locked_collateral(
					Origin::signed(1),
					collater_per_order * 2 - 1
				),
				<Error<Test>>::StillHasOrdersNotConfirmed
			);

			assert_relayer_info! {
				"account_id": 1,
				"free_balance": collater_per_order * 3,
				"usable_balance": collater_per_order,
				"is_enrolled": true,
				"collateral": collater_per_order * 2,
				"order_capacity": 0,
			}
			assert_relayer_info! {
				"account_id": 2,
				"free_balance": collater_per_order * 3,
				"usable_balance": collater_per_order,
				"is_enrolled": true,
				"collateral": collater_per_order * 2,
				"order_capacity": 0,
			}
			assert_relayer_info! {
				"account_id": 3,
				"free_balance": collater_per_order * 3,
				"usable_balance": collater_per_order,
				"is_enrolled": true,
				"collateral": collater_per_order * 2,
				"order_capacity": 0,
			}
		});
}

#[test]
fn test_decrease_collateral_without_occuiped() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	ExtBuilder::default()
		.with_balances(vec![(1, collater_per_order * 2)])
		.with_relayers(vec![(1, collater_per_order, None)])
		.build()
		.execute_with(|| {
			assert_relayer_info! {
				"account_id": 1,
				"free_balance": collater_per_order * 2,
				"usable_balance": collater_per_order,
				"is_enrolled": true,
				"collateral": collater_per_order,
				"order_capacity": 1,
			}

			assert_ok!(FeeMarket::decrease_locked_collateral(
				Origin::signed(1),
				collater_per_order - 10
			));

			assert_relayer_info! {
				"account_id": 1,
				"free_balance": collater_per_order * 2,
				"usable_balance": collater_per_order + 10,
				"is_enrolled": true,
				"collateral": collater_per_order - 10,
				"order_capacity": 0,
			}
		});
}

// update_relay_fee

#[test]
fn test_update_relayer_fee_failed_if_not_enroll() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	ExtBuilder::default()
		.with_balances(vec![(1, collater_per_order)])
		.with_relayers(vec![])
		.build()
		.execute_with(|| {
			assert_err!(
				FeeMarket::update_relay_fee(Origin::signed(1), 1),
				<Error<Test>>::NotEnrolled
			);
		});
}

#[test]
fn test_update_relayer_fee_failed_if_new_fee_too_low() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	let default_fee = <Test as Config>::MinimumRelayFee::get();
	ExtBuilder::default()
		.with_balances(vec![(1, collater_per_order)])
		.with_relayers(vec![(1, collater_per_order, None)])
		.build()
		.execute_with(|| {
			assert_err!(
				FeeMarket::update_relay_fee(Origin::signed(1), default_fee - 1),
				<Error<Test>>::RelayFeeTooLow
			);
		});
}

#[test]
fn test_update_relayer_fee_works() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	let default_fee = <Test as Config>::MinimumRelayFee::get();
	ExtBuilder::default()
		.with_balances(vec![(1, collater_per_order)])
		.with_relayers(vec![(1, collater_per_order, None)])
		.build()
		.execute_with(|| {
			assert_ok!(FeeMarket::update_relay_fee(Origin::signed(1), default_fee + 10),);
			assert_eq!(FeeMarket::relayer(&1).unwrap().fee, default_fee + 10);
		});
}

// cancel_enrollment

#[test]
fn test_cancel_enroll_failed_if_not_enroll() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	ExtBuilder::default()
		.with_balances(vec![(1, collater_per_order)])
		.with_relayers(vec![])
		.build()
		.execute_with(|| {
			assert_err!(
				FeeMarket::cancel_enrollment(Origin::signed(1)),
				<Error<Test>>::NotEnrolled
			);
		});
}

#[test]
fn test_cancel_enroll_failed_if_not_occuipied() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	let default_fee = <Test as Config>::MinimumRelayFee::get();
	ExtBuilder::default()
		.with_balances(vec![
			(1, collater_per_order),
			(2, collater_per_order),
			(3, collater_per_order),
		])
		.with_relayers(vec![
			(1, collater_per_order, None),
			(2, collater_per_order, None),
			(3, collater_per_order, None),
		])
		.build()
		.execute_with(|| {
			let _ = send_regular_message(1, default_fee);

			assert_err!(
				FeeMarket::cancel_enrollment(Origin::signed(1)),
				<Error<Test>>::OccupiedRelayer
			);
			assert_err!(
				FeeMarket::cancel_enrollment(Origin::signed(2)),
				<Error<Test>>::OccupiedRelayer
			);
			assert_err!(
				FeeMarket::cancel_enrollment(Origin::signed(3)),
				<Error<Test>>::OccupiedRelayer
			);
		});
}

#[test]
fn test_cancel_enroll_ok_if_order_confirmed() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	let default_fee = <Test as Config>::MinimumRelayFee::get();
	ExtBuilder::default()
		.with_balances(vec![
			(1, collater_per_order),
			(2, collater_per_order),
			(3, collater_per_order),
			(5, collater_per_order),
		])
		.with_relayers(vec![
			(1, collater_per_order, None),
			(2, collater_per_order, None),
			(3, collater_per_order, None),
		])
		.build()
		.execute_with(|| {
			let _ = send_regular_message(1, default_fee);

			System::set_block_number(3);
			assert_ok!(Messages::receive_messages_delivery_proof(
				Origin::signed(5),
				TestMessagesDeliveryProof(Ok((
					TEST_LANE_ID,
					InboundLaneData {
						relayers: vec![unrewarded_relayer(1, 1, TEST_RELAYER_A)]
							.into_iter()
							.collect(),
						..Default::default()
					}
				))),
				UnrewardedRelayersState {
					unrewarded_relayer_entries: 1,
					total_messages: 1,
					..Default::default()
				},
			));

			assert_ok!(FeeMarket::cancel_enrollment(Origin::signed(1)));
			assert_ok!(FeeMarket::cancel_enrollment(Origin::signed(2)));
			assert_ok!(FeeMarket::cancel_enrollment(Origin::signed(3)));
		});
}

#[test]
fn test_cancel_enroll_relayers_market_update_correctly() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	let default_fee = <Test as Config>::MinimumRelayFee::get();
	ExtBuilder::default()
		.with_balances(vec![
			(1, collater_per_order * 3),
			(2, collater_per_order * 3),
			(3, collater_per_order * 3),
		])
		.with_relayers(vec![
			(1, collater_per_order * 2, None),
			(2, collater_per_order * 2, None),
			(3, collater_per_order * 2, None),
		])
		.build()
		.execute_with(|| {
			assert_market_storage! {
				"relayers": vec![1, 2, 3],
				"assigned_relayers": vec![1, 2, 3],
				"market_fee": Some(default_fee),
			}

			assert_ok!(FeeMarket::cancel_enrollment(Origin::signed(1)));
			assert_ok!(FeeMarket::cancel_enrollment(Origin::signed(2)));
			assert_ok!(FeeMarket::cancel_enrollment(Origin::signed(3)));

			assert_market_storage! {
				"relayers": Vec::<u64>::new(),
				"assigned_relayers": Vec::<u64>::new(),
				"market_fee": None,
			}
		});
}

#[test]
fn test_cancel_enroll_relayers_balances_update_correctly() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	ExtBuilder::default()
		.with_balances(vec![
			(1, collater_per_order * 3),
			(2, collater_per_order * 3),
			(3, collater_per_order * 3),
		])
		.with_relayers(vec![
			(1, collater_per_order * 2, None),
			(2, collater_per_order * 2, None),
			(3, collater_per_order * 2, None),
		])
		.build()
		.execute_with(|| {
			assert_ok!(FeeMarket::cancel_enrollment(Origin::signed(1)));
			assert_ok!(FeeMarket::cancel_enrollment(Origin::signed(2)));
			assert_ok!(FeeMarket::cancel_enrollment(Origin::signed(3)));

			assert_relayer_info! {
				"account_id": 1,
				"free_balance": collater_per_order * 3,
				"usable_balance": collater_per_order * 3,
				"is_enrolled": false,
				"collateral": 0,
				"order_capacity": 0,
			}
			assert_relayer_info! {
				"account_id": 2,
				"free_balance": collater_per_order * 3,
				"usable_balance": collater_per_order * 3,
				"is_enrolled": false,
				"collateral": 0,
				"order_capacity": 0,
			}
			assert_relayer_info! {
				"account_id": 3,
				"free_balance": collater_per_order * 3,
				"usable_balance": collater_per_order * 3,
				"is_enrolled": false,
				"collateral": 0,
				"order_capacity": 0,
			}
		});
}

// Test market update

#[test]
fn test_market_fee_generate_failed_without_enough_relayers() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	ExtBuilder::default()
		.with_balances(vec![
			(1, collater_per_order),
			(2, collater_per_order),
			(3, collater_per_order),
		])
		.with_relayers(vec![(1, collater_per_order, None), (2, collater_per_order, None)])
		.build()
		.execute_with(|| {
			assert_market_storage! {
				"relayers": vec![1, 2],
				"assigned_relayers": Vec::<u64>::new(),
				"market_fee": None,
			}
		});
}

#[test]
fn test_market_fee_generate_failed_with_enough_relayers() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	let default_fee = <Test as Config>::MinimumRelayFee::get();
	ExtBuilder::default()
		.with_balances(vec![
			(1, collater_per_order),
			(2, collater_per_order),
			(3, collater_per_order),
		])
		.with_relayers(vec![
			(1, collater_per_order, None),
			(2, collater_per_order, None),
			(3, collater_per_order, None),
		])
		.build()
		.execute_with(|| {
			assert_market_storage! {
				"relayers": vec![1, 2, 3],
				"assigned_relayers": vec![1, 2, 3],
				"market_fee": Some(default_fee),
			}
		});
}

#[test]
fn test_market_fee_generate_sort_the_same_collater_different_fee() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	let default_fee = <Test as Config>::MinimumRelayFee::get();
	ExtBuilder::default()
		.with_balances(vec![
			(1, collater_per_order),
			(2, collater_per_order),
			(3, collater_per_order),
			(4, collater_per_order),
			(5, collater_per_order),
		])
		.with_relayers(vec![
			(1, collater_per_order, Some(default_fee + 10)),
			(2, collater_per_order, Some(default_fee + 30)),
			(3, collater_per_order, Some(default_fee + 50)),
			(4, collater_per_order, Some(default_fee + 70)),
			(5, collater_per_order, Some(default_fee + 90)),
		])
		.build()
		.execute_with(|| {
			assert_market_storage! {
				"relayers": vec![1, 2, 3, 4, 5],
				"assigned_relayers": vec![1, 2, 3],
				"market_fee": Some(default_fee + 50),
			}
		});
}

#[test]
fn test_market_fee_generate_sort_the_same_quota_different_collater() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	let default_fee = <Test as Config>::MinimumRelayFee::get();
	ExtBuilder::default()
		.with_balances(vec![
			(1, collater_per_order * 2),
			(2, collater_per_order * 2),
			(3, collater_per_order * 2),
			(4, collater_per_order * 2),
			(5, collater_per_order * 2),
		])
		.with_relayers(vec![
			(1, collater_per_order + 10, Some(default_fee)),
			(2, collater_per_order + 20, Some(default_fee)),
			(3, collater_per_order + 30, Some(default_fee)),
			(4, collater_per_order + 40, Some(default_fee)),
			(5, collater_per_order + 50, Some(default_fee)),
		])
		.build()
		.execute_with(|| {
			assert_market_storage! {
				"relayers": vec![1, 2, 3, 4, 5],
				"assigned_relayers": vec![5, 4, 3],
				"market_fee": Some(default_fee),
			}
		});
}

#[test]
fn test_market_fee_update_after_new_relayer_enroll() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	let default_fee = <Test as Config>::MinimumRelayFee::get();
	ExtBuilder::default()
		.with_balances(vec![
			(1, collater_per_order),
			(2, collater_per_order),
			(3, collater_per_order),
			(4, collater_per_order),
		])
		.with_relayers(vec![
			(1, collater_per_order, Some(default_fee + 10)),
			(2, collater_per_order, Some(default_fee + 20)),
			(3, collater_per_order, Some(default_fee + 30)),
		])
		.build()
		.execute_with(|| {
			assert_market_storage! {
				"relayers": vec![1, 2, 3],
				"assigned_relayers": vec![1, 2, 3],
				"market_fee": Some(default_fee + 30),
			}

			let _ = FeeMarket::enroll_and_lock_collateral(
				Origin::signed(4),
				collater_per_order,
				Some(default_fee + 25),
			);

			assert_market_storage! {
				"relayers": vec![1, 2, 3, 4],
				"assigned_relayers": vec![1, 2, 4],
				"market_fee": Some(default_fee + 25),
			}
		});
}

#[test]
fn test_market_fee_update_after_increase_collateral() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	let default_fee = <Test as Config>::MinimumRelayFee::get();
	ExtBuilder::default()
		.with_balances(vec![
			(1, collater_per_order),
			(2, collater_per_order * 2),
			(3, collater_per_order),
		])
		.with_relayers(vec![
			(1, collater_per_order, None),
			(2, collater_per_order, None),
			(3, collater_per_order, None),
		])
		.build()
		.execute_with(|| {
			assert_market_storage! {
				"relayers": vec![1, 2, 3],
				"assigned_relayers": vec![1, 2, 3],
				"market_fee": Some(default_fee),
			}

			assert_ok!(FeeMarket::increase_locked_collateral(
				Origin::signed(2),
				collater_per_order + 1,
			));

			assert_market_storage! {
				"relayers": vec![1, 2, 3],
				"assigned_relayers": vec![2, 1, 3],
				"market_fee": Some(default_fee),
			}
		});
}

#[test]
fn test_market_fee_update_after_decrease_collateral() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	let default_fee = <Test as Config>::MinimumRelayFee::get();
	ExtBuilder::default()
		.with_balances(vec![
			(1, collater_per_order * 2),
			(2, collater_per_order * 2),
			(3, collater_per_order * 2),
		])
		.with_relayers(vec![
			(1, collater_per_order * 2, None),
			(2, collater_per_order * 2, None),
			(3, collater_per_order * 2, None),
		])
		.build()
		.execute_with(|| {
			assert_market_storage! {
				"relayers": vec![1, 2, 3],
				"assigned_relayers": vec![1, 2, 3],
				"market_fee": Some(default_fee),
			}

			assert_ok!(FeeMarket::decrease_locked_collateral(
				Origin::signed(2),
				collater_per_order * 2 - 1,
			));

			assert_market_storage! {
				"relayers": vec![1, 2, 3],
				"assigned_relayers": vec![1, 3, 2],
				"market_fee": Some(default_fee),
			}
		});
}

#[test]
fn test_market_fee_update_after_update_fee() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	let default_fee = <Test as Config>::MinimumRelayFee::get();
	ExtBuilder::default()
		.with_balances(vec![
			(1, collater_per_order),
			(2, collater_per_order),
			(3, collater_per_order),
			(4, collater_per_order),
		])
		.with_relayers(vec![
			(1, collater_per_order, Some(default_fee + 10)),
			(2, collater_per_order, Some(default_fee + 20)),
			(3, collater_per_order, Some(default_fee + 30)),
			(4, collater_per_order, Some(default_fee + 40)),
		])
		.build()
		.execute_with(|| {
			assert_market_storage! {
				"relayers": vec![1, 2, 3, 4],
				"assigned_relayers": vec![1, 2, 3],
				"market_fee": Some(default_fee + 30),
			}

			assert_ok!(FeeMarket::update_relay_fee(Origin::signed(4), default_fee + 25,));

			assert_market_storage! {
				"relayers": vec![1, 2, 3, 4],
				"assigned_relayers": vec![1, 2, 4],
				"market_fee": Some(default_fee + 25),
			}
		});
}

#[test]
fn test_market_fee_update_after_cancel_enroll() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	let default_fee = <Test as Config>::MinimumRelayFee::get();
	ExtBuilder::default()
		.with_balances(vec![
			(1, collater_per_order),
			(2, collater_per_order),
			(3, collater_per_order),
			(4, collater_per_order),
		])
		.with_relayers(vec![
			(1, collater_per_order, None),
			(2, collater_per_order, None),
			(3, collater_per_order, None),
			(4, collater_per_order, None),
		])
		.build()
		.execute_with(|| {
			assert_market_storage! {
				"relayers": vec![1, 2, 3, 4],
				"assigned_relayers": vec![1, 2, 3],
				"market_fee": Some(default_fee),
			}

			assert_ok!(FeeMarket::cancel_enrollment(Origin::signed(1)));

			assert_market_storage! {
				"relayers": vec![2, 3, 4],
				"assigned_relayers": vec![2, 3, 4],
				"market_fee": Some(default_fee),
			}
		});
}

#[test]
fn test_market_fee_update_after_adjust_assigned_relayers_number() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	let default_fee = <Test as Config>::MinimumRelayFee::get();
	ExtBuilder::default()
		.with_balances(vec![
			(1, collater_per_order),
			(2, collater_per_order),
			(3, collater_per_order),
		])
		.with_relayers(vec![
			(1, collater_per_order, None),
			(2, collater_per_order, None),
			(3, collater_per_order, None),
		])
		.build()
		.execute_with(|| {
			assert_market_storage! {
				"relayers": vec![1, 2, 3],
				"assigned_relayers": vec![1, 2, 3],
				"market_fee": Some(default_fee),
			}

			assert_ok!(FeeMarket::set_assigned_relayers_number(Origin::root(), 2));

			assert_market_storage! {
				"relayers": vec![1, 2, 3],
				"assigned_relayers": vec![1, 2],
				"market_fee": Some(default_fee),
			}
		});
}

#[test]
fn test_market_fee_update_after_order_create() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	let default_fee = <Test as Config>::MinimumRelayFee::get();
	ExtBuilder::default()
		.with_balances(vec![
			(1, collater_per_order * 3),
			(2, collater_per_order * 3),
			(3, collater_per_order * 3),
			(4, collater_per_order * 3),
		])
		.with_relayers(vec![
			(1, collater_per_order * 1, Some(default_fee + 10)),
			(2, collater_per_order * 2, Some(default_fee + 20)),
			(3, collater_per_order * 2, Some(default_fee + 30)),
			(4, collater_per_order * 2, Some(default_fee + 40)),
		])
		.build()
		.execute_with(|| {
			assert_market_storage! {
				"relayers": vec![1, 2, 3, 4],
				"assigned_relayers": vec![1, 2, 3],
				"market_fee": Some(default_fee + 30),
			}

			System::set_block_number(2);
			let _ = send_regular_message(1, default_fee + 30);

			assert_market_storage! {
				"relayers": vec![1, 2, 3, 4],
				"assigned_relayers": vec![2, 3, 4],
				"market_fee": Some(default_fee + 40),
			}
		});
}

#[test]
fn test_market_fee_update_after_order_comfirm() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	let default_fee = <Test as Config>::MinimumRelayFee::get();
	ExtBuilder::default()
		.with_balances(vec![
			(1, collater_per_order * 3),
			(2, collater_per_order * 3),
			(3, collater_per_order * 3),
			(4, collater_per_order * 3),
		])
		.with_relayers(vec![
			(1, collater_per_order * 1, Some(default_fee + 10)),
			(2, collater_per_order * 2, Some(default_fee + 20)),
			(3, collater_per_order * 2, Some(default_fee + 30)),
			(4, collater_per_order * 2, Some(default_fee + 40)),
		])
		.build()
		.execute_with(|| {
			assert_market_storage! {
				"relayers": vec![1, 2, 3, 4],
				"assigned_relayers": vec![1, 2, 3],
				"market_fee": Some(default_fee + 30),
			}

			System::set_block_number(2);
			let _ = send_regular_message(1, default_fee + 30);

			System::set_block_number(3);
			receive_messages_delivery_proof();

			assert_market_storage! {
				"relayers": vec![1, 2, 3, 4],
				"assigned_relayers": vec![1, 2, 3],
				"market_fee": Some(default_fee + 30),
			}
		});
}

// Test Order

#[test]
fn test_order_create_if_market_ready() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	let default_fee = <Test as Config>::MinimumRelayFee::get();
	ExtBuilder::default()
		.with_balances(vec![
			(1, collater_per_order),
			(2, collater_per_order),
			(3, collater_per_order),
		])
		.with_relayers(vec![
			(1, collater_per_order, None),
			(2, collater_per_order, None),
			(3, collater_per_order, None),
		])
		.build()
		.execute_with(|| {
			System::set_block_number(2);
			let (lane, message_nonce) = send_regular_message(1, default_fee);
			let order = FeeMarket::order((&lane, &message_nonce)).unwrap();
			let relayers = order.assigned_relayers_slice();
			System::assert_has_event(Event::FeeMarket(crate::Event::OrderCreated(
				lane,
				message_nonce,
				order.fee(),
				vec![relayers[0].id, relayers[1].id, relayers[2].id],
				order.range_end(),
			)));
		});
}

#[test]
fn test_order_create_if_market_not_ready() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	ExtBuilder::default()
		.with_balances(vec![(1, collater_per_order)])
		.with_relayers(vec![(1, collater_per_order, None)])
		.build()
		.execute_with(|| {
			System::set_block_number(2);
			assert_err!(
				Messages::send_message(Origin::signed(1), TEST_LANE_ID, REGULAR_PAYLOAD, 200),
				DispatchError::Module(ModuleError {
					index: 4,
					error: [2, 0, 0, 0],
					message: Some("MessageRejectedByLaneVerifier")
				})
			);
		});
}

#[test]
fn test_order_create_then_order_capacity_reduce_by_one() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	let default_fee = <Test as Config>::MinimumRelayFee::get();
	ExtBuilder::default()
		.with_balances(vec![
			(1, collater_per_order * 3),
			(2, collater_per_order * 3),
			(3, collater_per_order * 3),
		])
		.with_relayers(vec![
			(1, collater_per_order * 2, None),
			(2, collater_per_order * 2, None),
			(3, collater_per_order * 2, None),
		])
		.build()
		.execute_with(|| {
			assert_relayer_info! {
				"account_id": 1,
				"free_balance": collater_per_order * 3,
				"usable_balance": collater_per_order,
				"is_enrolled": true,
				"collateral": collater_per_order * 2,
				"order_capacity": 2,
			}
			assert_relayer_info! {
				"account_id": 2,
				"free_balance": collater_per_order * 3,
				"usable_balance": collater_per_order,
				"is_enrolled": true,
				"collateral": collater_per_order * 2,
				"order_capacity": 2,
			}
			assert_relayer_info! {
				"account_id": 3,
				"free_balance": collater_per_order * 3,
				"usable_balance": collater_per_order,
				"is_enrolled": true,
				"collateral": collater_per_order * 2,
				"order_capacity": 2,
			}

			System::set_block_number(2);
			let _ = send_regular_message(1, default_fee);

			assert_relayer_info! {
				"account_id": 1,
				"free_balance": collater_per_order * 3,
				"usable_balance": collater_per_order,
				"is_enrolled": true,
				"collateral": collater_per_order * 2,
				"order_capacity": 1,
			}
			assert_relayer_info! {
				"account_id": 2,
				"free_balance": collater_per_order * 3,
				"usable_balance": collater_per_order,
				"is_enrolled": true,
				"collateral": collater_per_order * 2,
				"order_capacity": 1,
			}
			assert_relayer_info! {
				"account_id": 3,
				"free_balance": collater_per_order * 3,
				"usable_balance": collater_per_order,
				"is_enrolled": true,
				"collateral": collater_per_order * 2,
				"order_capacity": 1,
			}

			System::set_block_number(3);
			let _ = send_regular_message(1, default_fee);

			assert_relayer_info! {
				"account_id": 1,
				"free_balance": collater_per_order * 3,
				"usable_balance": collater_per_order,
				"is_enrolled": true,
				"collateral": collater_per_order * 2,
				"order_capacity": 0,
			}
			assert_relayer_info! {
				"account_id": 2,
				"free_balance": collater_per_order * 3,
				"usable_balance": collater_per_order,
				"is_enrolled": true,
				"collateral": collater_per_order * 2,
				"order_capacity": 0,
			}
			assert_relayer_info! {
				"account_id": 3,
				"free_balance": collater_per_order * 3,
				"usable_balance": collater_per_order,
				"is_enrolled": true,
				"collateral": collater_per_order * 2,
				"order_capacity": 0,
			}
		});
}

#[test]
fn test_order_confirm_works() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	let default_fee = <Test as Config>::MinimumRelayFee::get();
	ExtBuilder::default()
		.with_balances(vec![
			(1, collater_per_order),
			(2, collater_per_order),
			(3, collater_per_order),
		])
		.with_relayers(vec![
			(1, collater_per_order, None),
			(2, collater_per_order, None),
			(3, collater_per_order, None),
		])
		.build()
		.execute_with(|| {
			System::set_block_number(2);
			let (lane, message_nonce) = send_regular_message(1, default_fee);
			assert!(FeeMarket::order((&lane, &message_nonce)).is_some());

			System::set_block_number(4);
			receive_messages_delivery_proof();
			let order = FeeMarket::order((&lane, &message_nonce)).unwrap();
			assert_eq!(order.confirm_time, Some(4));
		});
}

#[test]
fn test_order_clean_at_the_end_of_block() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	let default_fee = <Test as Config>::MinimumRelayFee::get();
	ExtBuilder::default()
		.with_balances(vec![
			(1, collater_per_order),
			(2, collater_per_order),
			(3, collater_per_order),
		])
		.with_relayers(vec![
			(1, collater_per_order, None),
			(2, collater_per_order, None),
			(3, collater_per_order, None),
		])
		.build()
		.execute_with(|| {
			System::set_block_number(2);
			let (lane, message_nonce) = send_regular_message(1, default_fee);
			assert!(FeeMarket::order((&lane, &message_nonce)).is_some());

			System::set_block_number(4);
			receive_messages_delivery_proof();
			assert!(FeeMarket::order((&lane, &message_nonce)).is_some());

			FeeMarket::on_finalize(4);
			assert!(FeeMarket::order((&lane, &message_nonce)).is_none());
		});
}

#[test]
fn test_order_confirm_then_order_capacity_increase_by_one() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	let default_fee = <Test as Config>::MinimumRelayFee::get();
	ExtBuilder::default()
		.with_balances(vec![
			(1, collater_per_order * 3),
			(2, collater_per_order * 3),
			(3, collater_per_order * 3),
		])
		.with_relayers(vec![
			(1, collater_per_order * 2, None),
			(2, collater_per_order * 2, None),
			(3, collater_per_order * 2, None),
		])
		.build()
		.execute_with(|| {
			assert_relayer_info! {
				"account_id": 1,
				"free_balance": collater_per_order * 3,
				"usable_balance": collater_per_order,
				"is_enrolled": true,
				"collateral": collater_per_order * 2,
				"order_capacity": 2,
			}
			assert_relayer_info! {
				"account_id": 2,
				"free_balance": collater_per_order * 3,
				"usable_balance": collater_per_order,
				"is_enrolled": true,
				"collateral": collater_per_order * 2,
				"order_capacity": 2,
			}
			assert_relayer_info! {
				"account_id": 3,
				"free_balance": collater_per_order * 3,
				"usable_balance": collater_per_order,
				"is_enrolled": true,
				"collateral": collater_per_order * 2,
				"order_capacity": 2,
			}

			System::set_block_number(2);
			let _ = send_regular_message(1, default_fee);

			assert_relayer_info! {
				"account_id": 1,
				"free_balance": collater_per_order * 3,
				"usable_balance": collater_per_order,
				"is_enrolled": true,
				"collateral": collater_per_order * 2,
				"order_capacity": 1,
			}
			assert_relayer_info! {
				"account_id": 2,
				"free_balance": collater_per_order * 3,
				"usable_balance": collater_per_order,
				"is_enrolled": true,
				"collateral": collater_per_order * 2,
				"order_capacity": 1,
			}
			assert_relayer_info! {
				"account_id": 3,
				"free_balance": collater_per_order * 3,
				"usable_balance": collater_per_order,
				"is_enrolled": true,
				"collateral": collater_per_order * 2,
				"order_capacity": 1,
			}

			System::set_block_number(3);
			receive_messages_delivery_proof();

			assert_relayer_info! {
				"account_id": 1,
				"free_balance": collater_per_order * 3,
				"usable_balance": collater_per_order,
				"is_enrolled": true,
				"collateral": collater_per_order * 2,
				"order_capacity": 2,
			}
			assert_relayer_info! {
				"account_id": 2,
				"free_balance": collater_per_order * 3,
				"usable_balance": collater_per_order,
				"is_enrolled": true,
				"collateral": collater_per_order * 2,
				"order_capacity": 2,
			}
			assert_relayer_info! {
				"account_id": 3,
				"free_balance": collater_per_order * 3,
				"usable_balance": collater_per_order,
				"is_enrolled": true,
				"collateral": collater_per_order * 2,
				"order_capacity": 2,
			}
		});
}

// Test payment

#[test]
fn test_payment_cal_rewards_normally_single_message() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	ExtBuilder::default()
		.with_balances(vec![
			(1, collater_per_order),
			(2, collater_per_order),
			(3, collater_per_order),
			(4, collater_per_order),
		])
		.with_relayers(vec![
			(1, collater_per_order, Some(30)),
			(2, collater_per_order, Some(50)),
			(3, collater_per_order, Some(100)),
		])
		.build()
		.execute_with(|| {
			// Send message
			System::set_block_number(2);
			let market_fee = FeeMarket::market_fee().unwrap();
			let (lane, message_nonce) = send_regular_message(1, market_fee);

			// Receive delivery message proof
			System::set_block_number(4); // confirmed at block 4, the first slot
			assert_ok!(Messages::receive_messages_delivery_proof(
				Origin::signed(5),
				TestMessagesDeliveryProof(Ok((
					TEST_LANE_ID,
					InboundLaneData {
						relayers: vec![unrewarded_relayer(1, 1, TEST_RELAYER_A)]
							.into_iter()
							.collect(),
						..Default::default()
					}
				))),
				UnrewardedRelayersState {
					unrewarded_relayer_entries: 1,
					total_messages: 1,
					..Default::default()
				},
			));

			// Rewards Analysis:
			//  1. The order's assigned_relayers: [(1, 30, 2-52),(2, 50, 52-102),(3, 100, 102-152)]
			//  2. The order's fee: 100
			// For message delivery relayer(id=100): 30 * 80% = 24
			// For message confirm relayer(id=5): 30 * 20% = 6
			// For each assigned_relayer(id=1, 2, 3): (100 - 30) * 20% / 3 = 4
			// For treasury: 100 - (24 + 6) - (4 * 3) = 58
			let t: AccountId = <Test as Config>::TreasuryPalletId::get().into_account_truncating();
			assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(t, 58));
			assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(1, 4));
			assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(2, 4));
			assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(3, 4));
			assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(5, 6));
			assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(TEST_RELAYER_A, 24));

			System::assert_has_event(Event::FeeMarket(crate::Event::OrderReward(
				lane,
				message_nonce,
				RewardItem {
					to_assigned_relayers: BTreeMap::from_iter([(1, 4), (2, 4), (3, 4)]),
					to_treasury: Some(58),
					to_message_relayer: Some((100, 24)),
					to_confirm_relayer: Some((5, 6)),
				},
			)));
		});
}

#[test]
fn test_payment_cal_rewards_normally_multi_message() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	ExtBuilder::default()
		.with_balances(vec![
			(5, collater_per_order * 3),
			(6, collater_per_order * 3),
			(7, collater_per_order * 3),
		])
		.with_relayers(vec![
			(5, collater_per_order * 3, Some(30)),
			(6, collater_per_order * 3, Some(50)),
			(7, collater_per_order * 3, Some(100)),
		])
		.build()
		.execute_with(|| {
			System::set_block_number(2);

			// Send message
			let market_fee = FeeMarket::market_fee().unwrap();
			let (_, message_nonce1) = send_regular_message(1, market_fee);
			let (_, message_nonce2) = send_regular_message(1, market_fee);
			assert_eq!(message_nonce1 + 1, message_nonce2);

			// Receive delivery message proof
			System::set_block_number(4); // confirmed at block 4, the first slot
			assert_ok!(Messages::receive_messages_delivery_proof(
				Origin::signed(1),
				TestMessagesDeliveryProof(Ok((
					TEST_LANE_ID,
					InboundLaneData {
						relayers: vec![
							unrewarded_relayer(1, 1, TEST_RELAYER_A),
							unrewarded_relayer(2, 2, TEST_RELAYER_B)
						]
						.into_iter()
						.collect(),
						..Default::default()
					}
				))),
				UnrewardedRelayersState {
					unrewarded_relayer_entries: 2,
					total_messages: 2,
					..Default::default()
				},
			));

			// Rewards order1 Analysis(The same with order2):
			//  1. The order's assigned_relayers: [(5, 30, 2-52),(6, 50, 52-102),(7, 100, 102-152)]
			//  2. The order's fee: 100
			// For message delivery relayer(id=100): 30 * 80% = 24
			// For message confirm relayer(id=1): 30 * 20% = 6
			// For each assigned_relayer(id=5, 6, 7): (100 - 30) * 20% / 3 = 4
			// For treasury: 100 - (24 + 6) - (4 * 3) = 58
			let t: AccountId = <Test as Config>::TreasuryPalletId::get().into_account_truncating();
			assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(t, 116));
			assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(5, 8));
			assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(6, 8));
			assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(7, 8));
			assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(1, 12));
			assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(TEST_RELAYER_A, 24));
			assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(TEST_RELAYER_B, 24));
		});
}

#[test]
fn test_payment_cal_rewards_when_order_confirmed_in_second_slot() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	ExtBuilder::default()
		.with_balances(vec![
			(5, collater_per_order * 3),
			(6, collater_per_order * 3),
			(7, collater_per_order * 3),
		])
		.with_relayers(vec![
			(5, collater_per_order * 3, Some(30)),
			(6, collater_per_order * 3, Some(50)),
			(7, collater_per_order * 3, Some(100)),
		])
		.build()
		.execute_with(|| {
			System::set_block_number(2);

			// Send message
			let market_fee = FeeMarket::market_fee().unwrap();
			let _ = send_regular_message(1, market_fee);

			System::set_block_number(55); // confirmed at block 55, the second slot
			assert_ok!(Messages::receive_messages_delivery_proof(
				Origin::signed(1),
				TestMessagesDeliveryProof(Ok((
					TEST_LANE_ID,
					InboundLaneData {
						relayers: vec![unrewarded_relayer(1, 1, TEST_RELAYER_A),]
							.into_iter()
							.collect(),
						..Default::default()
					}
				))),
				UnrewardedRelayersState {
					unrewarded_relayer_entries: 1,
					total_messages: 1,
					..Default::default()
				},
			));

			assert_eq!(FeeMarket::relayer_locked_collateral(&5), 240);
			assert_eq!(FeeMarket::relayer_locked_collateral(&6), 300);
			assert_eq!(FeeMarket::relayer_locked_collateral(&7), 300);

			// Rewards Analysis:
			//  1. The order's assigned_relayers: [(5, 30, 2-52),(6, 50, 52-102),(7, 100, 102-152)]
			//  2. The order's fee: 100
			//  3. The slash for relayer(id=5): 300 * 20% = 60
			// For message delivery relayer(id=100): (50 + 60) * 80% = 88
			// For message confirm relayer(id=1): (50 + 60) * 20% = 22
			// For each assigned_relayer(id=6, 7): (100 - 50) * 20% / 2 = 5
			// For treasury: 100 - 50 - (5 * 2) = 40
			let t: AccountId = <Test as Config>::TreasuryPalletId::get().into_account_truncating();
			assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(t, 40));
			assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(6, 5));
			assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(7, 5));
			assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(1, 22));
			assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(TEST_RELAYER_A, 88));
		});
}

#[test]
fn test_payment_cal_rewards_when_order_confirmed_in_third_slot() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	ExtBuilder::default()
		.with_balances(vec![
			(5, collater_per_order * 3),
			(6, collater_per_order * 3),
			(7, collater_per_order * 3),
		])
		.with_relayers(vec![
			(5, collater_per_order * 3, Some(30)),
			(6, collater_per_order * 3, Some(50)),
			(7, collater_per_order * 3, Some(100)),
		])
		.build()
		.execute_with(|| {
			System::set_block_number(2);

			// Send message
			let market_fee = FeeMarket::market_fee().unwrap();
			let _ = send_regular_message(1, market_fee);

			System::set_block_number(105); // confirmed at block 55, the third slot
			assert_ok!(Messages::receive_messages_delivery_proof(
				Origin::signed(1),
				TestMessagesDeliveryProof(Ok((
					TEST_LANE_ID,
					InboundLaneData {
						relayers: vec![unrewarded_relayer(1, 1, TEST_RELAYER_A),]
							.into_iter()
							.collect(),
						..Default::default()
					}
				))),
				UnrewardedRelayersState {
					unrewarded_relayer_entries: 1,
					total_messages: 1,
					..Default::default()
				},
			));

			assert_eq!(FeeMarket::relayer_locked_collateral(&5), 240);
			assert_eq!(FeeMarket::relayer_locked_collateral(&6), 240);
			assert_eq!(FeeMarket::relayer_locked_collateral(&7), 300);

			// Rewards Analysis:
			//  1. The order's assigned_relayers: [(5, 30, 2-52),(6, 50, 52-102),(7, 100, 102-152)]
			//  2. The order's fee: 100
			//  3. The slash for relayer(id=5, 6): 300 * 20% = 60
			// For message delivery relayer(id=100): (100 + 60 * 2) * 80% = 176
			// For message confirm relayer(id=1): (100 + 60 * 2) * 20% = 44
			// For each assigned_relayer(id=7): (100 - 100) * 20% = 0
			// For treasury: 100 - 100 - (0 * 2) = 0
			let t: AccountId = <Test as Config>::TreasuryPalletId::get().into_account_truncating();
			assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(t, 0));
			assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(7, 0));
			assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(1, 44));
			assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(TEST_RELAYER_A, 176));
		});
}

#[test]
fn test_payment_cal_reward_with_duplicated_delivery_proof() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	ExtBuilder::default()
		.with_balances(vec![
			(1, collater_per_order * 2),
			(2, collater_per_order * 2),
			(3, collater_per_order * 2),
		])
		.with_relayers(vec![(1, 100, Some(30)), (2, 110, Some(50)), (3, 120, Some(100))])
		.build()
		.execute_with(|| {
			System::set_block_number(2);

			// Send message
			let market_fee = FeeMarket::market_fee().unwrap();
			let (_, _) = send_regular_message(1, market_fee);

			// The first time receive delivery message proof
			System::set_block_number(4);
			assert_ok!(Messages::receive_messages_delivery_proof(
				Origin::signed(5),
				TestMessagesDeliveryProof(Ok((
					TEST_LANE_ID,
					InboundLaneData {
						relayers: vec![unrewarded_relayer(1, 1, TEST_RELAYER_A)]
							.into_iter()
							.collect(),
						..Default::default()
					}
				))),
				UnrewardedRelayersState {
					unrewarded_relayer_entries: 1,
					total_messages: 1,
					..Default::default()
				},
			));
			// The second time receive delivery message proof
			assert_ok!(Messages::receive_messages_delivery_proof(
				Origin::signed(6),
				TestMessagesDeliveryProof(Ok((
					TEST_LANE_ID,
					InboundLaneData {
						relayers: vec![unrewarded_relayer(1, 1, TEST_RELAYER_A)]
							.into_iter()
							.collect(),
						..Default::default()
					}
				))),
				UnrewardedRelayersState {
					unrewarded_relayer_entries: 1,
					total_messages: 1,
					..Default::default()
				},
			));

			// Rewards Analysis:
			//  1. The order's assigned_relayers: [(1, 30, 2-52),(2, 50, 52-102),(3, 100, 102-152)]
			//  2. The order's fee: 100
			// For message delivery relayer(id=100): 30 * 80% = 24
			// For message confirm relayer(id=5): 30 * 20% = 6
			// For each assigned_relayer(id=1, 2, 3): (100 - 30) * 20% / 3 = 4
			// For treasury: 100 - (24 + 6) - (4 * 3) = 58
			let t: AccountId = <Test as Config>::TreasuryPalletId::get().into_account_truncating();
			assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(t, 58));
			assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(1, 4));
			assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(2, 4));
			assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(3, 4));
			assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(5, 6));
			assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(TEST_RELAYER_A, 24));
		});
}

#[test]
fn test_payment_with_slash_and_reduce_order_capacity() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	ExtBuilder::default()
		.with_balances(vec![
			(6, collater_per_order * 5),
			(7, collater_per_order * 5),
			(8, collater_per_order * 5),
		])
		.with_relayers(vec![
			(6, collater_per_order * 4, Some(30)),
			(7, collater_per_order * 4, Some(50)),
			(8, collater_per_order * 4, Some(100)),
		])
		.build()
		.execute_with(|| {
			// Send message
			System::set_block_number(2);
			assert_eq!(FeeMarket::relayer_locked_collateral(&6), 400);
			let market_fee = FeeMarket::market_fee().unwrap();
			let (_, _) = send_regular_message(1, market_fee);

			// Receive delivery message proof
			System::set_block_number(2000);
			assert_ok!(Messages::receive_messages_delivery_proof(
				Origin::signed(5),
				TestMessagesDeliveryProof(Ok((
					TEST_LANE_ID,
					InboundLaneData {
						relayers: vec![unrewarded_relayer(1, 1, TEST_RELAYER_A)]
							.into_iter()
							.collect(),
						..Default::default()
					}
				))),
				UnrewardedRelayersState {
					unrewarded_relayer_entries: 1,
					total_messages: 1,
					..Default::default()
				},
			));
			assert!(FeeMarket::is_enrolled(&6));
			assert!(FeeMarket::is_enrolled(&6));
			assert!(FeeMarket::is_enrolled(&6));
			assert_eq!(FeeMarket::relayer_locked_collateral(&6), 220);
			assert_eq!(FeeMarket::relayer_locked_collateral(&7), 220);
			assert_eq!(FeeMarket::relayer_locked_collateral(&8), 220);
			assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(5, 128));
			assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(TEST_RELAYER_A, 512));
		});
}

#[test]
fn test_payment_slash_with_protect() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	ExtBuilder::default()
		.with_balances(vec![
			(6, collater_per_order * 5),
			(7, collater_per_order * 5),
			(8, collater_per_order * 5),
		])
		.with_relayers(vec![
			(6, collater_per_order * 4, Some(30)),
			(7, collater_per_order * 4, Some(50)),
			(8, collater_per_order * 4, Some(100)),
		])
		.build()
		.execute_with(|| {
			// Send message
			System::set_block_number(2);
			let market_fee = FeeMarket::market_fee().unwrap();
			let (_, _) = send_regular_message(1, market_fee);
			assert_ok!(FeeMarket::set_slash_protect(Origin::root(), 50));

			// Receive delivery message proof
			System::set_block_number(2000);
			assert_ok!(Messages::receive_messages_delivery_proof(
				Origin::signed(5),
				TestMessagesDeliveryProof(Ok((
					TEST_LANE_ID,
					InboundLaneData {
						relayers: vec![unrewarded_relayer(1, 1, TEST_RELAYER_A)]
							.into_iter()
							.collect(),
						..Default::default()
					}
				))),
				UnrewardedRelayersState {
					unrewarded_relayer_entries: 1,
					total_messages: 1,
					..Default::default()
				},
			));
			assert!(FeeMarket::is_enrolled(&6));
			assert!(FeeMarket::is_enrolled(&6));
			assert!(FeeMarket::is_enrolled(&6));
			assert_eq!(FeeMarket::relayer_locked_collateral(&6), 350);
			assert_eq!(FeeMarket::relayer_locked_collateral(&7), 350);
			assert_eq!(FeeMarket::relayer_locked_collateral(&8), 350);
			assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(5, 50));
			assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(TEST_RELAYER_A, 200));
		});
}

#[test]
fn test_payment_slash_event() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	ExtBuilder::default()
		.with_balances(vec![
			(6, collater_per_order * 5),
			(7, collater_per_order * 5),
			(8, collater_per_order * 5),
		])
		.with_relayers(vec![
			(6, collater_per_order * 4, Some(30)),
			(7, collater_per_order * 4, Some(50)),
			(8, collater_per_order * 4, Some(100)),
		])
		.build()
		.execute_with(|| {
			// Send message
			System::set_block_number(2);
			let market_fee = FeeMarket::market_fee().unwrap();
			let (_, _) = send_regular_message(1, market_fee);
			assert_ok!(FeeMarket::set_slash_protect(Origin::root(), 50));

			// Receive delivery message proof
			System::set_block_number(2000);
			assert_ok!(Messages::receive_messages_delivery_proof(
				Origin::signed(5),
				TestMessagesDeliveryProof(Ok((
					TEST_LANE_ID,
					InboundLaneData {
						relayers: vec![unrewarded_relayer(1, 1, TEST_RELAYER_A)]
							.into_iter()
							.collect(),
						..Default::default()
					}
				))),
				UnrewardedRelayersState {
					unrewarded_relayer_entries: 1,
					total_messages: 1,
					..Default::default()
				},
			));

			System::assert_has_event(Event::FeeMarket(crate::Event::FeeMarketSlash(SlashReport {
				lane: TEST_LANE_ID,
				message: 1,
				sent_time: 2,
				confirm_time: Some(2000),
				delay_time: Some(1848),
				account_id: 6,
				amount: 50,
			})));
			System::assert_has_event(Event::FeeMarket(crate::Event::FeeMarketSlash(SlashReport {
				lane: TEST_LANE_ID,
				message: 1,
				sent_time: 2,
				confirm_time: Some(2000),
				delay_time: Some(1848),
				account_id: 7,
				amount: 50,
			})));
			System::assert_has_event(Event::FeeMarket(crate::Event::FeeMarketSlash(SlashReport {
				lane: TEST_LANE_ID,
				message: 1,
				sent_time: 2,
				confirm_time: Some(2000),
				delay_time: Some(1848),
				account_id: 8,
				amount: 50,
			})));
		});
}

#[test]
fn test_payment_with_multiple_message_out_of_deadline() {
	let collater_per_order = <Test as Config>::CollateralPerOrder::get();
	ExtBuilder::default()
		.with_balances(vec![
			(6, collater_per_order * 5),
			(7, collater_per_order * 5),
			(8, collater_per_order * 5),
		])
		.with_relayers(vec![
			(6, collater_per_order * 4, Some(300)),
			(7, collater_per_order * 4, Some(500)),
			(8, collater_per_order * 4, Some(1000)),
		])
		.build()
		.execute_with(|| {
			System::set_block_number(2);

			// Send message
			let market_fee = FeeMarket::market_fee().unwrap();
			let (_, message_nonce1) = send_regular_message(1, market_fee);
			let (_, message_nonce2) = send_regular_message(1, market_fee);
			assert_eq!(message_nonce1 + 1, message_nonce2);

			// Receive delivery message proof
			System::set_block_number(2000);
			assert_ok!(Messages::receive_messages_delivery_proof(
				Origin::signed(5),
				TestMessagesDeliveryProof(Ok((
					TEST_LANE_ID,
					InboundLaneData {
						relayers: vec![
							unrewarded_relayer(1, 1, TEST_RELAYER_A),
							unrewarded_relayer(2, 2, TEST_RELAYER_B)
						]
						.into_iter()
						.collect(),
						..Default::default()
					}
				))),
				UnrewardedRelayersState {
					unrewarded_relayer_entries: 2,
					total_messages: 2,
					..Default::default()
				},
			));

			assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(5, 594));
			assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(TEST_RELAYER_A, 1232));
			assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(TEST_RELAYER_B, 1146));
		});
}
