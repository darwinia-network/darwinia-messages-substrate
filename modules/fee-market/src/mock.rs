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
// --- crates.io ---
use bitvec::prelude::*;
use scale_info::TypeInfo;
// --- paritytech ---
use bp_messages::{
	source_chain::{
		LaneMessageVerifier, MessageDeliveryAndDispatchPayment, SenderOrigin, TargetHeaderChain,
	},
	target_chain::{
		DispatchMessage, MessageDispatch, ProvedLaneMessages, ProvedMessages, SourceHeaderChain,
	},
	DeliveredMessages, InboundLaneData, LaneId, Message, MessageNonce, OutboundLaneData,
	Parameter as MessagesParameter, UnrewardedRelayer,
};
use bp_runtime::{messages::MessageDispatchResult, Size};
use frame_support::{
	traits::{Everything, LockIdentifier},
	weights::{RuntimeDbWeight, Weight},
	PalletId,
};
use frame_system::mocking::*;
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{AccountIdConversion, BlakeTwo256, IdentityLookup, UniqueSaturatedInto},
	FixedU128, Permill,
};
// --- darwinia-network ---
use crate::{
	self as darwinia_fee_market,
	s2s::{
		payment::calculate_rewards, FeeMarketMessageAcceptedHandler,
		FeeMarketMessageConfirmedHandler,
	},
	*,
};

type Block = MockBlock<Test>;
type UncheckedExtrinsic = MockUncheckedExtrinsic<Test>;
pub(crate) type Balance = u64;
pub(crate) type AccountId = u64;

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
	fn size_hint(&self) -> u32 {
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
	fn size_hint(&self) -> u32 {
		0
	}
}

/// Messages delivery proof used in tests.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct TestMessagesDeliveryProof(pub Result<(LaneId, InboundLaneData<TestRelayer>), ()>);
impl Size for TestMessagesDeliveryProof {
	fn size_hint(&self) -> u32 {
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

	fn dispatch_weight(message: &DispatchMessage<TestPayload, TestMessageFee>) -> Weight {
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
