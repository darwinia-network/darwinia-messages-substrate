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
use std::{
	collections::{BTreeMap, VecDeque},
	ops::RangeInclusive,
};
// crates.io
use bitvec::prelude::*;
use scale_info::TypeInfo;
// darwinia-network
use crate::{
	self as darwinia_fee_market,
	s2s::{
		payment::calculate_rewards, FeeMarketMessageAcceptedHandler,
		FeeMarketMessageConfirmedHandler,
	},
	*,
};
// paritytech
use bp_messages::{
	source_chain::{
		LaneMessageVerifier, MessageDeliveryAndDispatchPayment, SenderOrigin, TargetHeaderChain,
	},
	target_chain::{
		DispatchMessage, MessageDispatch, ProvedLaneMessages, ProvedMessages, SourceHeaderChain,
	},
	DeliveredMessages, InboundLaneData, LaneId, Message, MessageNonce, OutboundLaneData,
	Parameter as MessagesParameter, UnrewardedRelayer, UnrewardedRelayersState,
};
use bp_runtime::{messages::MessageDispatchResult, Size};
use frame_support::{
	assert_err, assert_ok,
	traits::{Everything, LockIdentifier},
	weights::{RuntimeDbWeight, Weight},
	PalletId,
};
use frame_system::mocking::*;
use pallet_bridge_messages::outbound_lane;
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{AccountIdConversion, BlakeTwo256, IdentityLookup, UniqueSaturatedInto},
	FixedU128, ModuleError, Permill,
};

type Block = MockBlock<Test>;
type UncheckedExtrinsic = MockUncheckedExtrinsic<Test>;
type Balance = u64;
type AccountId = u64;

frame_support::parameter_types! {
	pub const DbWeight: RuntimeDbWeight = RuntimeDbWeight { read: 1, write: 2 };
}
impl frame_system::Config for Test {
	type AccountData = pallet_balances::AccountData<Balance>;
	type AccountId = AccountId;
	type BaseCallFilter = Everything;
	type BlockHashCount = ();
	type BlockLength = ();
	type BlockNumber = u64;
	type BlockWeights = ();
	type Call = Call;
	type DbWeight = DbWeight;
	type Event = Event;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type Header = Header;
	type Index = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type MaxConsumers = ConstU32<16>;
	type OnKilledAccount = ();
	type OnNewAccount = ();
	type OnSetCode = ();
	type Origin = Origin;
	type PalletInfo = PalletInfo;
	type SS58Prefix = ();
	type SystemWeightInfo = ();
	type Version = ();
}

frame_support::parameter_types! {
	pub const ExistentialDeposit: u64 = 1;
}
impl pallet_balances::Config for Test {
	type AccountStore = System;
	type Balance = Balance;
	type DustRemoval = ();
	type Event = Event;
	type ExistentialDeposit = ExistentialDeposit;
	type MaxLocks = ();
	type MaxReserves = ();
	type ReserveIdentifier = [u8; 8];
	type WeightInfo = ();
}

frame_support::parameter_types! {
	pub const MinimumPeriod: u64 = 1000;
}
impl pallet_timestamp::Config for Test {
	type MinimumPeriod = MinimumPeriod;
	type Moment = u64;
	type OnTimestampSet = ();
	type WeightInfo = ();
}

// >>> Start mock pallet-bridges-message config data

pub type TestMessageFee = u64;
pub type TestRelayer = u64;
/// Lane that we're using in tests.
pub const TEST_LANE_ID: LaneId = [0, 0, 0, 1];
/// Error that is returned by all test implementations.
pub const TEST_ERROR: &str = "Test error";
/// Account id of test relayer.
pub const TEST_RELAYER_A: AccountId = 100;
/// Account id of additional test relayer - B.
pub const TEST_RELAYER_B: AccountId = 101;
/// Payload that is rejected by `TestTargetHeaderChain`.
pub const PAYLOAD_REJECTED_BY_TARGET_CHAIN: TestPayload = message_payload(1, 50);
/// Regular message payload.
pub const REGULAR_PAYLOAD: TestPayload = message_payload(0, 50);
/// Vec of proved messages, grouped by lane.
pub type MessagesByLaneVec = Vec<(LaneId, ProvedLaneMessages<Message<TestMessageFee>>)>;

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct TestPayload {
	/// Field that may be used to identify messages.
	pub id: u64,
	/// Dispatch weight that is declared by the message sender.
	pub declared_weight: Weight,
	/// Message dispatch result.
	///
	/// Note: in correct code `dispatch_result.unspent_weight` will always be <= `declared_weight`,
	/// but for test purposes we'll be making it larger than `declared_weight` sometimes.
	pub dispatch_result: MessageDispatchResult,
	/// Extra bytes that affect payload size.
	pub extra: Vec<u8>,
}
impl Size for TestPayload {
	fn size(&self) -> u32 {
		16 + self.extra.len() as u32
	}
}
/// Constructs message payload using given arguments and zero unspent weight.
pub const fn message_payload(id: u64, declared_weight: Weight) -> TestPayload {
	TestPayload { id, declared_weight, dispatch_result: dispatch_result(0), extra: Vec::new() }
}

/// Test messages proof.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct TestMessagesProof {
	pub result: Result<MessagesByLaneVec, ()>,
}
impl Size for TestMessagesProof {
	fn size(&self) -> u32 {
		0
	}
}

/// Messages delivery proof used in tests.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct TestMessagesDeliveryProof(pub Result<(LaneId, InboundLaneData<TestRelayer>), ()>);
impl Size for TestMessagesDeliveryProof {
	fn size(&self) -> u32 {
		0
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum TestMessagesParameter {
	TokenConversionRate(FixedU128),
}
impl MessagesParameter for TestMessagesParameter {
	fn save(&self) {
		match *self {
			TestMessagesParameter::TokenConversionRate(conversion_rate) =>
				TokenConversionRate::set(&conversion_rate),
		}
	}
}

/// Target header chain that is used in tests.
#[derive(Debug, Default)]
pub struct TestTargetHeaderChain;
impl TargetHeaderChain<TestPayload, TestRelayer> for TestTargetHeaderChain {
	type Error = &'static str;
	type MessagesDeliveryProof = TestMessagesDeliveryProof;

	fn verify_message(payload: &TestPayload) -> Result<(), Self::Error> {
		if *payload == PAYLOAD_REJECTED_BY_TARGET_CHAIN {
			Err(TEST_ERROR)
		} else {
			Ok(())
		}
	}

	fn verify_messages_delivery_proof(
		proof: Self::MessagesDeliveryProof,
	) -> Result<(LaneId, InboundLaneData<TestRelayer>), Self::Error> {
		proof.0.map_err(|_| TEST_ERROR)
	}
}

/// Lane message verifier that is used in tests.
#[derive(Debug, Default)]
pub struct TestLaneMessageVerifier;
impl LaneMessageVerifier<Origin, AccountId, TestPayload, TestMessageFee>
	for TestLaneMessageVerifier
{
	type Error = &'static str;

	fn verify_message(
		_submitter: &Origin,
		delivery_and_dispatch_fee: &TestMessageFee,
		_lane: &LaneId,
		_lane_outbound_data: &OutboundLaneData,
		_payload: &TestPayload,
	) -> Result<(), Self::Error> {
		if let Some(market_fee) = FeeMarket::market_fee() {
			if *delivery_and_dispatch_fee < market_fee {
				return Err(TEST_ERROR);
			}
		} else {
			return Err(TEST_ERROR);
		}
		Ok(())
	}
}

/// Message fee payment system that is used in tests.
#[derive(Debug, Default)]
pub struct TestMessageDeliveryAndDispatchPayment;
impl TestMessageDeliveryAndDispatchPayment {
	/// Reject all payments.
	pub fn reject_payments() {
		frame_support::storage::unhashed::put(b":reject-message-fee:", &true);
	}

	/// Returns true if given fee has been paid by given submitter.
	pub fn is_fee_paid(submitter: AccountId, fee: TestMessageFee) -> bool {
		let raw_origin: Result<frame_system::RawOrigin<_>, _> = Origin::signed(submitter).into();
		frame_support::storage::unhashed::get(b":message-fee:") == Some((raw_origin.unwrap(), fee))
	}

	/// Returns true if given relayer has been rewarded with given balance. The reward-paid flag is
	/// cleared after the call.
	pub fn is_reward_paid(relayer: AccountId, fee: TestMessageFee) -> bool {
		let key = (b":relayer-reward:", relayer, fee).encode();
		frame_support::storage::unhashed::take::<bool>(&key).is_some()
	}
}
impl MessageDeliveryAndDispatchPayment<Origin, AccountId, TestMessageFee>
	for TestMessageDeliveryAndDispatchPayment
{
	type Error = &'static str;

	fn pay_delivery_and_dispatch_fee(
		submitter: &Origin,
		fee: &TestMessageFee,
		_relayer_fund_account: &AccountId,
	) -> Result<(), Self::Error> {
		if frame_support::storage::unhashed::get(b":reject-message-fee:") == Some(true) {
			return Err(TEST_ERROR);
		}

		let raw_origin: Result<frame_system::RawOrigin<_>, _> = submitter.clone().into();
		frame_support::storage::unhashed::put(b":message-fee:", &(raw_origin.unwrap(), fee));
		Ok(())
	}

	fn pay_relayers_rewards(
		lane_id: LaneId,
		messages_relayers: VecDeque<UnrewardedRelayer<AccountId>>,
		confirmation_relayer: &AccountId,
		received_range: &RangeInclusive<MessageNonce>,
		relayer_fund_account: &AccountId,
	) {
		let rewards_items = calculate_rewards::<Test, ()>(
			lane_id,
			messages_relayers,
			confirmation_relayer.clone(),
			received_range,
			relayer_fund_account,
		);

		let mut deliver_sum = BTreeMap::<AccountId, Balance>::new();
		let mut confirm_sum = Balance::zero();
		let mut assigned_relayers_sum = BTreeMap::<AccountId, Balance>::new();
		let mut treasury_sum = Balance::zero();
		for item in rewards_items {
			for (k, v) in item.to_assigned_relayers.iter() {
				assigned_relayers_sum
					.entry(k.clone())
					.and_modify(|r| *r = r.saturating_add(v.clone()))
					.or_insert(*v);
			}

			if let Some(reward) = item.to_treasury {
				treasury_sum = treasury_sum.saturating_add(reward);
			}

			if let Some((id, reward)) = item.to_message_relayer {
				deliver_sum
					.entry(id)
					.and_modify(|r| *r = r.saturating_add(reward))
					.or_insert(reward);
			}

			if let Some((_id, reward)) = item.to_confirm_relayer {
				confirm_sum = confirm_sum.saturating_add(reward);
			}
		}

		let confimation_key = (b":relayer-reward:", confirmation_relayer, confirm_sum).encode();
		frame_support::storage::unhashed::put(&confimation_key, &true);

		for (relayer, reward) in &deliver_sum {
			let key = (b":relayer-reward:", relayer, reward).encode();
			frame_support::storage::unhashed::put(&key, &true);
		}

		for (relayer, reward) in &assigned_relayers_sum {
			let key = (b":relayer-reward:", relayer, reward).encode();
			frame_support::storage::unhashed::put(&key, &true);
		}

		let treasury_account: AccountId =
			<Test as Config>::TreasuryPalletId::get().into_account_truncating();
		let treasury_key = (b":relayer-reward:", &treasury_account, treasury_sum).encode();
		frame_support::storage::unhashed::put(&treasury_key, &true);
	}
}
/// Source header chain that is used in tests.
#[derive(Debug)]
pub struct TestSourceHeaderChain;
impl SourceHeaderChain<TestMessageFee> for TestSourceHeaderChain {
	type Error = &'static str;
	type MessagesProof = TestMessagesProof;

	fn verify_messages_proof(
		proof: Self::MessagesProof,
		_messages_count: u32,
	) -> Result<ProvedMessages<Message<TestMessageFee>>, Self::Error> {
		proof.result.map(|proof| proof.into_iter().collect()).map_err(|_| TEST_ERROR)
	}
}

/// Source header chain that is used in tests.
#[derive(Debug)]
pub struct TestMessageDispatch;
impl MessageDispatch<AccountId, TestMessageFee> for TestMessageDispatch {
	type DispatchPayload = TestPayload;

	fn dispatch_weight(message: &mut DispatchMessage<TestPayload, TestMessageFee>) -> Weight {
		match message.data.payload.as_ref() {
			Ok(payload) => payload.declared_weight,
			Err(_) => 0,
		}
	}

	fn pre_dispatch(
		_relayer_account: &AccountId,
		_message: &DispatchMessage<TestPayload, TestMessageFee>,
	) -> Result<(), &'static str> {
		Ok(())
	}

	fn dispatch(
		_relayer_account: &AccountId,
		message: DispatchMessage<TestPayload, TestMessageFee>,
	) -> MessageDispatchResult {
		match message.data.payload.as_ref() {
			Ok(payload) => payload.dispatch_result.clone(),
			Err(_) => dispatch_result(0),
		}
	}
}

pub struct AccountIdConverter;
impl sp_runtime::traits::Convert<H256, AccountId> for AccountIdConverter {
	fn convert(hash: H256) -> AccountId {
		hash.to_low_u64_ne()
	}
}

// >>> End mock pallet-bridges-message config data

frame_support::parameter_types! {
	pub const MaxMessagesToPruneAtOnce: u64 = 10;
	pub const MaxUnrewardedRelayerEntriesAtInboundLane: u64 = 16;
	pub const MaxUnconfirmedMessagesAtInboundLane: u64 = 32;
	pub storage TokenConversionRate: FixedU128 = 1.into();
	pub const TestBridgedChainId: bp_runtime::ChainId = *b"test";
}

impl pallet_bridge_messages::Config for Test {
	type AccountIdConverter = AccountIdConverter;
	type BridgedChainId = TestBridgedChainId;
	type Event = Event;
	type InboundMessageFee = TestMessageFee;
	type InboundPayload = TestPayload;
	type InboundRelayer = TestRelayer;
	type LaneMessageVerifier = TestLaneMessageVerifier;
	type MaxMessagesToPruneAtOnce = MaxMessagesToPruneAtOnce;
	type MaxUnconfirmedMessagesAtInboundLane = MaxUnconfirmedMessagesAtInboundLane;
	type MaxUnrewardedRelayerEntriesAtInboundLane = MaxUnrewardedRelayerEntriesAtInboundLane;
	type MaximalOutboundPayloadSize = frame_support::traits::ConstU32<4096>;
	type MessageDeliveryAndDispatchPayment = TestMessageDeliveryAndDispatchPayment;
	type MessageDispatch = TestMessageDispatch;
	type OnDeliveryConfirmed = FeeMarketMessageConfirmedHandler<Self, ()>;
	type OnMessageAccepted = FeeMarketMessageAcceptedHandler<Self, ()>;
	type OutboundMessageFee = TestMessageFee;
	type OutboundPayload = TestPayload;
	type Parameter = TestMessagesParameter;
	type SourceHeaderChain = TestSourceHeaderChain;
	type TargetHeaderChain = TestTargetHeaderChain;
	type WeightInfo = ();
}

impl SenderOrigin<AccountId> for Origin {
	fn linked_account(&self) -> Option<AccountId> {
		match self.caller {
			OriginCaller::system(frame_system::RawOrigin::Signed(ref submitter)) =>
				Some(submitter.clone()),
			_ => None,
		}
	}
}

frame_support::parameter_types! {
	pub const TreasuryPalletId: PalletId = PalletId(*b"da/trsry");
	pub const FeeMarketLockId: LockIdentifier = *b"da/feelf";
	pub const MinimumRelayFee: Balance = 30;
	pub const CollateralPerOrder: Balance = 100;
	pub const Slot: u64 = 50;

	pub const DutyRelayersRewardRatio: Permill = Permill::from_percent(20);
	pub const MessageRelayersRewardRatio: Permill = Permill::from_percent(80);
	pub const ConfirmRelayersRewardRatio: Permill = Permill::from_percent(20);
	pub const AssignedRelayerSlashRatio: Permill = Permill::from_percent(20);
	pub const TreasuryPalletAccount: u64 = 666;
}

pub struct TestSlasher;
impl<T: Config<I>, I: 'static> Slasher<T, I> for TestSlasher {
	fn cal_slash_amount(
		collateral_per_order: BalanceOf<T, I>,
		timeout: T::BlockNumber,
	) -> BalanceOf<T, I> {
		let slash_each_block = 2;
		let slash_value = UniqueSaturatedInto::<u128>::unique_saturated_into(timeout)
			.saturating_mul(UniqueSaturatedInto::<u128>::unique_saturated_into(slash_each_block))
			.unique_saturated_into();
		sp_std::cmp::min(collateral_per_order, slash_value)
	}
}

impl Config for Test {
	type AssignedRelayerSlashRatio = AssignedRelayerSlashRatio;
	type CollateralPerOrder = CollateralPerOrder;
	type ConfirmRelayersRewardRatio = ConfirmRelayersRewardRatio;
	type Currency = Balances;
	type DutyRelayersRewardRatio = DutyRelayersRewardRatio;
	type Event = Event;
	type LockId = FeeMarketLockId;
	type MessageRelayersRewardRatio = MessageRelayersRewardRatio;
	type MinimumRelayFee = MinimumRelayFee;
	type Slasher = TestSlasher;
	type Slot = Slot;
	type TreasuryPalletId = TreasuryPalletId;
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
		FeeMarket: darwinia_fee_market::{Pallet, Call, Storage, Event<T>},
		Messages: pallet_bridge_messages::{Pallet, Call, Event<T>},
	}
}
pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::default().build_storage::<Test>().unwrap();
	pallet_balances::GenesisConfig::<Test> {
		balances: vec![
			(1, 150),
			(2, 200),
			(3, 350),
			(4, 220),
			(5, 350),
			(6, 500),
			(7, 500),
			(8, 500),
			(12, 2000),
			(13, 2000),
			(14, 2000),
		],
	}
	.assimilate_storage(&mut t)
	.unwrap();

	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

/// Returns message dispatch result with given unspent weight.
pub const fn dispatch_result(unspent_weight: Weight) -> MessageDispatchResult {
	MessageDispatchResult {
		dispatch_result: true,
		unspent_weight,
		dispatch_fee_paid_during_dispatch: true,
	}
}

/// Constructs unrewarded relayer entry from nonces range and relayer id.
pub fn unrewarded_relayer(
	begin: MessageNonce,
	end: MessageNonce,
	relayer: TestRelayer,
) -> UnrewardedRelayer<TestRelayer> {
	UnrewardedRelayer {
		relayer,
		messages: DeliveredMessages {
			begin,
			end,
			dispatch_results: if end >= begin {
				bitvec![u8, Msb0; 1; (end - begin + 1) as _]
			} else {
				Default::default()
			},
		},
	}
}

#[test]
fn test_call_relayer_enroll_works() {
	new_test_ext().execute_with(|| {
		assert_eq!(Balances::free_balance(1), 150);
		assert_err!(
			FeeMarket::enroll_and_lock_collateral(Origin::signed(1), 200, None),
			<Error<Test>>::InsufficientBalance
		);
		assert_err!(
			FeeMarket::enroll_and_lock_collateral(Origin::signed(1), 99, None),
			<Error<Test>>::CollateralTooLow
		);

		assert_ok!(FeeMarket::enroll_and_lock_collateral(Origin::signed(1), 100, None));
		assert!(FeeMarket::is_enrolled(&1));
		assert_eq!(FeeMarket::relayers().unwrap().len(), 1);
		assert_eq!(Balances::free_balance(1), 150);
		assert_eq!(Balances::usable_balance(&1), 50);
		assert_eq!(FeeMarket::relayer_locked_collateral(&1), 100);
		assert_eq!(FeeMarket::market_fee(), None);
		assert_err!(
			FeeMarket::enroll_and_lock_collateral(Origin::signed(1), 100, None),
			<Error<Test>>::AlreadyEnrolled
		);
	});
}

#[test]
fn test_call_relayer_increase_lock_collateral_works() {
	new_test_ext().execute_with(|| {
		assert_err!(
			FeeMarket::update_locked_collateral(Origin::signed(12), 100),
			<Error::<Test>>::NotEnrolled
		);

		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(12), 200, None);
		assert_eq!(FeeMarket::relayer_locked_collateral(&12), 200);

		// Increase locked collateral from 200 to 500
		assert_ok!(FeeMarket::update_locked_collateral(Origin::signed(12), 500));
		assert_eq!(FeeMarket::relayer_locked_collateral(&12), 500);

		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(13), 200, None);
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(14), 300, None);
		let market_fee = FeeMarket::market_fee().unwrap();
		let _ = send_regular_message(market_fee);
		let _ = send_regular_message(market_fee);
		assert_ok!(FeeMarket::update_locked_collateral(Origin::signed(12), 800));
		assert_ok!(FeeMarket::update_locked_collateral(Origin::signed(13), 800));
		assert_ok!(FeeMarket::update_locked_collateral(Origin::signed(14), 800));
		assert_eq!(FeeMarket::relayer_locked_collateral(&12), 800);
		assert_eq!(FeeMarket::relayer_locked_collateral(&13), 800);
		assert_eq!(FeeMarket::relayer_locked_collateral(&14), 800);
	});
}

#[test]
fn test_call_relayer_decrease_lock_collateral_works() {
	new_test_ext().execute_with(|| {
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(12), 800, None);
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(13), 800, None);
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(14), 800, None);
		let market_fee = FeeMarket::market_fee().unwrap();
		let _ = send_regular_message(market_fee);
		let _ = send_regular_message(market_fee);
		let _ = send_regular_message(market_fee);
		let _ = send_regular_message(market_fee);

		assert_err!(
			FeeMarket::update_locked_collateral(Origin::signed(12), 300),
			<Error::<Test>>::StillHasOrdersNotConfirmed
		);
		assert_ok!(FeeMarket::update_locked_collateral(Origin::signed(12), 400));
		assert_eq!(FeeMarket::relayer_locked_collateral(&12), 400);
		assert_ok!(FeeMarket::update_locked_collateral(Origin::signed(13), 500));
		assert_eq!(FeeMarket::relayer_locked_collateral(&13), 500);
		assert_ok!(FeeMarket::update_locked_collateral(Origin::signed(14), 700));
		assert_eq!(FeeMarket::relayer_locked_collateral(&14), 700);
	});
}

#[test]
fn test_call_relayer_cancel_registration_works() {
	new_test_ext().execute_with(|| {
		assert_err!(FeeMarket::cancel_enrollment(Origin::signed(1)), <Error<Test>>::NotEnrolled);

		assert_ok!(FeeMarket::enroll_and_lock_collateral(Origin::signed(1), 100, None));
		assert!(FeeMarket::is_enrolled(&1));
		assert_ok!(FeeMarket::cancel_enrollment(Origin::signed(1)));
		assert!(!FeeMarket::is_enrolled(&1));

		System::set_block_number(2);
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(1), 100, None);
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(2), 110, Some(50));
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(3), 120, Some(100));
		assert_eq!(FeeMarket::relayers().unwrap(), vec![1, 2, 3]);
		assert_eq!(
			FeeMarket::assigned_relayers().unwrap(),
			vec![
				Relayer::<AccountId, Balance>::new(1, 100, 30),
				Relayer::<AccountId, Balance>::new(2, 110, 50),
				Relayer::<AccountId, Balance>::new(3, 120, 100),
			]
		);
		let _ = send_regular_message(FeeMarket::market_fee().unwrap());
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

		// clean order info, then 3 is able to cancel enrollment.
		System::set_block_number(3);
		assert_ok!(Messages::receive_messages_delivery_proof(
			Origin::signed(5),
			TestMessagesDeliveryProof(Ok((
				TEST_LANE_ID,
				InboundLaneData {
					relayers: vec![unrewarded_relayer(1, 1, TEST_RELAYER_A)].into_iter().collect(),
					..Default::default()
				}
			))),
			UnrewardedRelayersState {
				unrewarded_relayer_entries: 1,
				total_messages: 1,
				last_delivered_nonce: 1,
				..Default::default()
			},
		));
		assert_ok!(FeeMarket::cancel_enrollment(Origin::signed(3)));
		assert_eq!(FeeMarket::relayers().unwrap(), vec![1, 2]);
		assert!(FeeMarket::assigned_relayers().is_none());
		assert!(FeeMarket::market_fee().is_none());
	});
}

#[test]
fn test_call_relayer_update_fee_works() {
	new_test_ext().execute_with(|| {
		assert_err!(FeeMarket::update_relay_fee(Origin::signed(1), 1), <Error<Test>>::NotEnrolled);
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(1), 100, None);
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(2), 110, Some(50));
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(3), 120, Some(100));
		assert_eq!(FeeMarket::market_fee(), Some(100));
		assert_err!(
			FeeMarket::update_relay_fee(Origin::signed(1), 1),
			<Error<Test>>::RelayFeeTooLow
		);

		assert_eq!(FeeMarket::relayer(&1).unwrap().fee, 30);
		assert_ok!(FeeMarket::update_relay_fee(Origin::signed(1), 40));
		assert_eq!(FeeMarket::relayer(&1).unwrap().fee, 40);

		assert_ok!(FeeMarket::update_relay_fee(Origin::signed(3), 150));
		assert_eq!(FeeMarket::relayer(&3).unwrap().fee, 150);
		assert_eq!(FeeMarket::market_fee(), Some(150));
	});
}

#[test]
fn test_rpc_market_fee_works() {
	new_test_ext().execute_with(|| {
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(1), 100, None);
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(2), 110, Some(40));
		assert!(FeeMarket::market_fee().is_none());

		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(3), 200, Some(40));
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(4), 120, Some(40));
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(5), 150, Some(50));
		assert_eq!(FeeMarket::market_fee(), Some(40));
		assert_eq!(
			FeeMarket::assigned_relayers().unwrap(),
			vec![
				Relayer::<AccountId, Balance>::new(1, 100, 30),
				Relayer::<AccountId, Balance>::new(3, 200, 40),
				Relayer::<AccountId, Balance>::new(4, 120, 40),
			]
		);
	});
}

fn send_regular_message(fee: Balance) -> (LaneId, u64) {
	let message_nonce = outbound_lane::<Test, ()>(TEST_LANE_ID).data().latest_generated_nonce + 1;
	assert_ok!(Messages::send_message(Origin::signed(1), TEST_LANE_ID, REGULAR_PAYLOAD, fee));

	(TEST_LANE_ID, message_nonce)
}

fn receive_messages_delivery_proof() {
	assert_ok!(Messages::receive_messages_delivery_proof(
		Origin::signed(1),
		TestMessagesDeliveryProof(Ok((
			TEST_LANE_ID,
			InboundLaneData {
				last_confirmed_nonce: 1,
				relayers: vec![UnrewardedRelayer {
					relayer: 0,
					messages: DeliveredMessages::new(1, true),
				}]
				.into_iter()
				.collect(),
			},
		))),
		UnrewardedRelayersState {
			unrewarded_relayer_entries: 1,
			total_messages: 1,
			last_delivered_nonce: 1,
			..Default::default()
		},
	));
}

#[test]
fn test_callback_order_creation() {
	new_test_ext().execute_with(|| {
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(2), 200, None);
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(3), 210, None);
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(4), 220, None);
		System::set_block_number(2);

		let assigned_relayers = FeeMarket::assigned_relayers().unwrap();
		let market_fee = FeeMarket::market_fee().unwrap();
		let (lane, message_nonce) = send_regular_message(market_fee);
		assert!(FeeMarket::market_fee().is_some());
		assert!(FeeMarket::assigned_relayers().is_some());

		let order = FeeMarket::order((&lane, &message_nonce)).unwrap();
		let relayers = order.assigned_relayers_slice();
		assert_eq!(relayers[0].id, assigned_relayers.get(0).unwrap().id);
		assert_eq!(relayers[1].id, assigned_relayers.get(1).unwrap().id);
		assert_eq!(relayers[2].id, assigned_relayers.get(2).unwrap().id);
		assert_eq!(order.sent_time, 2);

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
fn test_callback_no_order_created_when_fee_market_not_ready() {
	new_test_ext().execute_with(|| {
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(1), 100, None);
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(2), 100, None);
		System::set_block_number(2);

		assert!(FeeMarket::assigned_relayers().is_none());
		assert_err!(
			Messages::send_message(Origin::signed(1), TEST_LANE_ID, REGULAR_PAYLOAD, 200),
			DispatchError::Module(ModuleError {
				index: 4,
				error: [3, 0, 0, 0],
				message: Some("MessageRejectedByLaneVerifier")
			})
		);
	});
}

#[test]
fn test_callback_order_confirm() {
	new_test_ext().execute_with(|| {
		System::set_block_number(2);
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(1), 100, None);
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(2), 110, None);
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(3), 120, None);
		let market_fee = FeeMarket::market_fee().unwrap();
		let (lane, message_nonce) = send_regular_message(market_fee);
		let order = FeeMarket::order((&lane, &message_nonce)).unwrap();
		assert_eq!(order.confirm_time, None);

		System::set_block_number(4);
		receive_messages_delivery_proof();
		let order = FeeMarket::order((&lane, &message_nonce)).unwrap();
		assert_eq!(order.confirm_time, Some(4));
		assert!(FeeMarket::market_fee().is_some());
		assert!(FeeMarket::assigned_relayers().is_some());
	});
}

#[test]
fn test_payment_cal_rewards_normally_single_message() {
	new_test_ext().execute_with(|| {
		// Send message
		System::set_block_number(2);
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(1), 100, Some(30));
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(2), 110, Some(50));
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(3), 120, Some(100));
		let market_fee = FeeMarket::market_fee().unwrap();
		let (lane, message_nonce) = send_regular_message(market_fee);

		// Receive delivery message proof
		System::set_block_number(4); // confirmed at block 4, the first slot
		assert_ok!(Messages::receive_messages_delivery_proof(
			Origin::signed(5),
			TestMessagesDeliveryProof(Ok((
				TEST_LANE_ID,
				InboundLaneData {
					relayers: vec![unrewarded_relayer(1, 1, TEST_RELAYER_A)].into_iter().collect(),
					..Default::default()
				}
			))),
			UnrewardedRelayersState {
				unrewarded_relayer_entries: 1,
				total_messages: 1,
				last_delivered_nonce: 1,
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
	new_test_ext().execute_with(|| {
		System::set_block_number(2);
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(5), 300, Some(30));
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(6), 300, Some(50));
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(7), 300, Some(100));

		// Send message
		let market_fee = FeeMarket::market_fee().unwrap();
		let (_, message_nonce1) = send_regular_message(market_fee);
		let (_, message_nonce2) = send_regular_message(market_fee);
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
				last_delivered_nonce: 2,
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
	new_test_ext().execute_with(|| {
		System::set_block_number(2);
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(5), 300, Some(30));
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(6), 300, Some(50));
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(7), 300, Some(100));

		// Send message
		let market_fee = FeeMarket::market_fee().unwrap();
		let _ = send_regular_message(market_fee);

		assert_eq!(FeeMarket::relayer_locked_collateral(&5), 300);
		assert_eq!(FeeMarket::relayer_locked_collateral(&6), 300);
		assert_eq!(FeeMarket::relayer_locked_collateral(&7), 300);

		System::set_block_number(55); // confirmed at block 55, the second slot
		assert_ok!(Messages::receive_messages_delivery_proof(
			Origin::signed(1),
			TestMessagesDeliveryProof(Ok((
				TEST_LANE_ID,
				InboundLaneData {
					relayers: vec![unrewarded_relayer(1, 1, TEST_RELAYER_A),].into_iter().collect(),
					..Default::default()
				}
			))),
			UnrewardedRelayersState {
				unrewarded_relayer_entries: 1,
				total_messages: 1,
				last_delivered_nonce: 1,
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
	new_test_ext().execute_with(|| {
		System::set_block_number(2);
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(5), 300, Some(30));
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(6), 300, Some(50));
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(7), 300, Some(100));

		// Send message
		let market_fee = FeeMarket::market_fee().unwrap();
		let _ = send_regular_message(market_fee);

		assert_eq!(FeeMarket::relayer_locked_collateral(&5), 300);
		assert_eq!(FeeMarket::relayer_locked_collateral(&6), 300);
		assert_eq!(FeeMarket::relayer_locked_collateral(&7), 300);

		System::set_block_number(105); // confirmed at block 55, the third slot
		assert_ok!(Messages::receive_messages_delivery_proof(
			Origin::signed(1),
			TestMessagesDeliveryProof(Ok((
				TEST_LANE_ID,
				InboundLaneData {
					relayers: vec![unrewarded_relayer(1, 1, TEST_RELAYER_A),].into_iter().collect(),
					..Default::default()
				}
			))),
			UnrewardedRelayersState {
				unrewarded_relayer_entries: 1,
				total_messages: 1,
				last_delivered_nonce: 1,
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
	new_test_ext().execute_with(|| {
		System::set_block_number(2);
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(1), 100, Some(30));
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(2), 110, Some(50));
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(3), 120, Some(100));

		// Send message
		let market_fee = FeeMarket::market_fee().unwrap();
		let (_, _) = send_regular_message(market_fee);

		// The first time receive delivery message proof
		System::set_block_number(4);
		assert_ok!(Messages::receive_messages_delivery_proof(
			Origin::signed(5),
			TestMessagesDeliveryProof(Ok((
				TEST_LANE_ID,
				InboundLaneData {
					relayers: vec![unrewarded_relayer(1, 1, TEST_RELAYER_A)].into_iter().collect(),
					..Default::default()
				}
			))),
			UnrewardedRelayersState {
				unrewarded_relayer_entries: 1,
				total_messages: 1,
				last_delivered_nonce: 1,
				..Default::default()
			},
		));
		// The second time receive delivery message proof
		assert_ok!(Messages::receive_messages_delivery_proof(
			Origin::signed(6),
			TestMessagesDeliveryProof(Ok((
				TEST_LANE_ID,
				InboundLaneData {
					relayers: vec![unrewarded_relayer(1, 1, TEST_RELAYER_A)].into_iter().collect(),
					..Default::default()
				}
			))),
			UnrewardedRelayersState {
				unrewarded_relayer_entries: 1,
				total_messages: 1,
				last_delivered_nonce: 1,
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
	new_test_ext().execute_with(|| {
		// Send message
		System::set_block_number(2);
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(6), 400, Some(30));
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(7), 400, Some(50));
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(8), 400, Some(100));
		assert_eq!(FeeMarket::relayer_locked_collateral(&6), 400);
		let market_fee = FeeMarket::market_fee().unwrap();
		let (_, _) = send_regular_message(market_fee);

		// Receive delivery message proof
		System::set_block_number(2000);
		assert_ok!(Messages::receive_messages_delivery_proof(
			Origin::signed(5),
			TestMessagesDeliveryProof(Ok((
				TEST_LANE_ID,
				InboundLaneData {
					relayers: vec![unrewarded_relayer(1, 1, TEST_RELAYER_A)].into_iter().collect(),
					..Default::default()
				}
			))),
			UnrewardedRelayersState {
				unrewarded_relayer_entries: 1,
				total_messages: 1,
				last_delivered_nonce: 1,
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
	new_test_ext().execute_with(|| {
		// Send message
		System::set_block_number(2);
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(6), 400, Some(30));
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(7), 400, Some(50));
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(8), 400, Some(100));
		assert_eq!(FeeMarket::relayer_locked_collateral(&6), 400);
		let market_fee = FeeMarket::market_fee().unwrap();
		let (_, _) = send_regular_message(market_fee);
		assert_ok!(FeeMarket::set_slash_protect(Origin::root(), 50));

		// Receive delivery message proof
		System::set_block_number(2000);
		assert_ok!(Messages::receive_messages_delivery_proof(
			Origin::signed(5),
			TestMessagesDeliveryProof(Ok((
				TEST_LANE_ID,
				InboundLaneData {
					relayers: vec![unrewarded_relayer(1, 1, TEST_RELAYER_A)].into_iter().collect(),
					..Default::default()
				}
			))),
			UnrewardedRelayersState {
				unrewarded_relayer_entries: 1,
				total_messages: 1,
				last_delivered_nonce: 1,
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
	new_test_ext().execute_with(|| {
		// Send message
		System::set_block_number(2);
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(6), 400, Some(30));
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(7), 400, Some(50));
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(8), 400, Some(100));
		assert_eq!(FeeMarket::relayer_locked_collateral(&6), 400);
		let market_fee = FeeMarket::market_fee().unwrap();
		let (_, _) = send_regular_message(market_fee);
		assert_ok!(FeeMarket::set_slash_protect(Origin::root(), 50));

		// Receive delivery message proof
		System::set_block_number(2000);
		assert_ok!(Messages::receive_messages_delivery_proof(
			Origin::signed(5),
			TestMessagesDeliveryProof(Ok((
				TEST_LANE_ID,
				InboundLaneData {
					relayers: vec![unrewarded_relayer(1, 1, TEST_RELAYER_A)].into_iter().collect(),
					..Default::default()
				}
			))),
			UnrewardedRelayersState {
				unrewarded_relayer_entries: 1,
				total_messages: 1,
				last_delivered_nonce: 1,
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
	new_test_ext().execute_with(|| {
		System::set_block_number(2);
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(6), 400, Some(300));
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(7), 400, Some(500));
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(8), 400, Some(1000));

		// Send message
		let market_fee = FeeMarket::market_fee().unwrap();
		let (_, message_nonce1) = send_regular_message(market_fee);
		let (_, message_nonce2) = send_regular_message(market_fee);
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
				last_delivered_nonce: 2,
				..Default::default()
			},
		));

		assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(5, 594));
		assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(TEST_RELAYER_A, 1232));
		assert!(TestMessageDeliveryAndDispatchPayment::is_reward_paid(TEST_RELAYER_B, 1146));
	});
}

#[test]
fn test_clean_order_state_at_the_end_of_block() {
	new_test_ext().execute_with(|| {
		System::set_block_number(2);
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(6), 400, None);
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(7), 400, None);
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(8), 400, None);
		let market_fee = FeeMarket::market_fee().unwrap();
		let (lane1, nonce1) = send_regular_message(market_fee);
		let (lane2, nonce2) = send_regular_message(market_fee);
		System::set_block_number(3);
		let (lane3, nonce3) = send_regular_message(market_fee);
		let (lane4, nonce4) = send_regular_message(market_fee);

		System::set_block_number(10);
		assert_ok!(Messages::receive_messages_delivery_proof(
			Origin::signed(5),
			TestMessagesDeliveryProof(Ok((
				TEST_LANE_ID,
				InboundLaneData {
					relayers: vec![
						unrewarded_relayer(1, 2, TEST_RELAYER_A),
						unrewarded_relayer(3, 4, TEST_RELAYER_B)
					]
					.into_iter()
					.collect(),
					..Default::default()
				}
			))),
			UnrewardedRelayersState {
				unrewarded_relayer_entries: 2,
				total_messages: 4,
				last_delivered_nonce: 4,
				..Default::default()
			},
		));
		assert!(FeeMarket::order((&lane1, &nonce1)).is_some());
		assert!(FeeMarket::order((&lane2, &nonce2)).is_some());
		assert!(FeeMarket::order((&lane3, &nonce3)).is_some());
		assert!(FeeMarket::order((&lane4, &nonce4)).is_some());

		// Check in next block
		FeeMarket::on_finalize(10);
		System::set_block_number(1);
		assert!(FeeMarket::order((&lane1, &nonce1)).is_none());
		assert!(FeeMarket::order((&lane2, &nonce2)).is_none());
		assert!(FeeMarket::order((&lane3, &nonce3)).is_none());
		assert!(FeeMarket::order((&lane4, &nonce4)).is_none());
	});
}

#[test]
fn test_fee_verification_when_send_message() {
	new_test_ext().execute_with(|| {
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(1), 100, None);
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(2), 100, None);
		assert!(FeeMarket::market_fee().is_none());

		// Case 1: When fee market are not ready, but somebody send messages
		assert_err!(
			Messages::send_message(Origin::signed(1), TEST_LANE_ID, REGULAR_PAYLOAD, 200),
			DispatchError::Module(ModuleError {
				index: 4,
				error: [3, 0, 0, 0],
				message: Some("MessageRejectedByLaneVerifier")
			})
		);

		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(3), 100, Some(50));
		// Case 2: The fee market is ready, but the order fee is too low
		assert_err!(
			Messages::send_message(Origin::signed(1), TEST_LANE_ID, REGULAR_PAYLOAD, 49),
			DispatchError::Module(ModuleError {
				index: 4,
				error: [3, 0, 0, 0],
				message: Some("MessageRejectedByLaneVerifier")
			})
		);

		// Case 3: Normal workflow
		assert_ok!(Messages::send_message(Origin::signed(1), TEST_LANE_ID, REGULAR_PAYLOAD, 50),);
	});
}

#[test]
fn test_relayer_occupied_result() {
	new_test_ext().execute_with(|| {
		System::set_block_number(2);
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(5), 300, None);
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(6), 300, None);
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(7), 300, None);

		// Send message
		let market_fee = FeeMarket::market_fee().unwrap();
		let _ = send_regular_message(market_fee);
		let _ = send_regular_message(market_fee);

		assert_eq!(FeeMarket::occupied(&5), Some((2, 200)));
		assert_eq!(FeeMarket::occupied(&6), Some((2, 200)));
		assert_eq!(FeeMarket::occupied(&7), Some((2, 200)));
		assert_eq!(FeeMarket::usable_order_capacity(&5), 1);
		assert_eq!(FeeMarket::usable_order_capacity(&6), 1);
		assert_eq!(FeeMarket::usable_order_capacity(&7), 1);
		receive_messages_delivery_proof();
		assert_eq!(FeeMarket::occupied(&5), Some((1, 100)));
		assert_eq!(FeeMarket::occupied(&6), Some((1, 100)));
		assert_eq!(FeeMarket::occupied(&7), Some((1, 100)));
		assert_eq!(FeeMarket::usable_order_capacity(&5), 2);
		assert_eq!(FeeMarket::usable_order_capacity(&6), 2);
		assert_eq!(FeeMarket::usable_order_capacity(&7), 2);
	});
}

#[test]
fn test_relayer_update_order_capacity() {
	new_test_ext().execute_with(|| {
		System::set_block_number(2);
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(5), 300, None);
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(6), 300, None);
		let _ = FeeMarket::enroll_and_lock_collateral(Origin::signed(7), 300, None);

		let market_fee = FeeMarket::market_fee().unwrap();
		let _ = send_regular_message(market_fee);
		let _ = send_regular_message(market_fee);
		let _ = send_regular_message(market_fee);

		assert_eq!(FeeMarket::occupied(&5), Some((3, 300)));
		assert_eq!(FeeMarket::usable_order_capacity(&5), 0);
		assert_eq!(FeeMarket::usable_order_capacity(&6), 0);
		assert_eq!(FeeMarket::usable_order_capacity(&7), 0);
		assert!(FeeMarket::market_fee().is_none());

		System::set_block_number(10);
		assert_ok!(Messages::receive_messages_delivery_proof(
			Origin::signed(5),
			TestMessagesDeliveryProof(Ok((
				TEST_LANE_ID,
				InboundLaneData {
					relayers: vec![unrewarded_relayer(1, 3, TEST_RELAYER_A),].into_iter().collect(),
					..Default::default()
				}
			))),
			UnrewardedRelayersState {
				unrewarded_relayer_entries: 1,
				total_messages: 3,
				last_delivered_nonce: 3,
				..Default::default()
			},
		));

		assert_eq!(FeeMarket::occupied(&5), None);
		assert_eq!(FeeMarket::usable_order_capacity(&5), 3);
		assert_eq!(FeeMarket::usable_order_capacity(&6), 3);
		assert_eq!(FeeMarket::usable_order_capacity(&7), 3);
		assert!(FeeMarket::market_fee().is_some());
	});
}
