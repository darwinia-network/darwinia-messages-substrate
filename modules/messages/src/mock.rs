// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

// From construct_runtime macro
#![allow(clippy::from_over_into)]

// std
use std::{
	collections::{BTreeMap, VecDeque},
	ops::RangeInclusive,
};
// crates.io
use codec::{Decode, Encode};
use scale_info::TypeInfo;
// darwinia-network
use crate as pallet_bridge_messages;
use crate::*;
use bp_messages::{
	source_chain::{DeliveryConfirmationPayments, LaneMessageVerifier, TargetHeaderChain},
	target_chain::{
		DeliveryPayments, DispatchMessage, DispatchMessageData, MessageDispatch,
		ProvedLaneMessages, ProvedMessages, SourceHeaderChain,
	},
	DeliveredMessages, InboundLaneData, LaneId, Message, MessageKey, MessageNonce, MessagePayload,
	OutboundLaneData, UnrewardedRelayer,
};
use bp_runtime::{messages::MessageDispatchResult, Size};
// paritytech
use frame_support::{
	parameter_types,
	weights::{RuntimeDbWeight, Weight},
};
use frame_system::mocking::*;
use sp_core::{ConstU64, H256};
use sp_runtime::{
	testing::Header as SubstrateHeader,
	traits::{BlakeTwo256, IdentityLookup},
	Perbill,
};

pub type AccountId = u64;
pub type Balance = u64;

pub type TestMessageFee = u64;
pub type TestRelayer = u64;
pub type TestDispatchLevelResult = ();

type Block = MockBlock<TestRuntime>;
type UncheckedExtrinsic = MockUncheckedExtrinsic<TestRuntime>;

/// Vec of proved messages, grouped by lane.
pub type MessagesByLaneVec = Vec<(LaneId, ProvedLaneMessages<Message>)>;

/// Maximal outbound payload size.
pub const MAX_OUTBOUND_PAYLOAD_SIZE: u32 = 4096;

/// Account that has balance to use in tests.
pub const ENDOWED_ACCOUNT: AccountId = 0xDEAD;

/// Account id of test relayer.
pub const TEST_RELAYER_A: AccountId = 100;

/// Account id of additional test relayer - B.
pub const TEST_RELAYER_B: AccountId = 101;

/// Account id of additional test relayer - C.
pub const TEST_RELAYER_C: AccountId = 102;

/// Error that is returned by all test implementations.
pub const TEST_ERROR: &str = "Test error";

/// Lane that we're using in tests.
pub const TEST_LANE_ID: LaneId = [0, 0, 0, 1];

/// Secondary lane that we're using in tests.
pub const TEST_LANE_ID_2: LaneId = [0, 0, 0, 2];

/// Inactive outbound lane.
pub const TEST_LANE_ID_3: LaneId = [0, 0, 0, 3];

/// Regular message payload.
pub const REGULAR_PAYLOAD: TestPayload = message_payload(0, 50);

/// Payload that is rejected by `TestTargetHeaderChain`.
pub const PAYLOAD_REJECTED_BY_TARGET_CHAIN: TestPayload = message_payload(1, 50);

#[derive(Decode, Encode, Clone, Debug, PartialEq, Eq, TypeInfo)]
pub struct TestPayload {
	/// Field that may be used to identify messages.
	pub id: u64,
	/// Reject this message by lane verifier?
	pub reject_by_lane_verifier: bool,
	/// Dispatch weight that is declared by the message sender.
	pub declared_weight: Weight,
	/// Message dispatch result.
	///
	/// Note: in correct code `dispatch_result.unspent_weight` will always be <= `declared_weight`,
	/// but for test purposes we'll be making it larger than `declared_weight` sometimes.
	pub dispatch_result: MessageDispatchResult<TestDispatchLevelResult>,
	/// Extra bytes that affect payload size.
	pub extra: Vec<u8>,
}

pub struct AccountIdConverter;
impl sp_runtime::traits::Convert<H256, AccountId> for AccountIdConverter {
	fn convert(hash: H256) -> AccountId {
		hash.to_low_u64_ne()
	}
}

frame_support::construct_runtime! {
	pub enum TestRuntime where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
		Balances: pallet_balances::{Pallet, Call, Event<T>},
		Messages: pallet_bridge_messages::{Pallet, Call, Event<T>},
	}
}

parameter_types! {
	pub const MaximumBlockWeight: Weight = Weight::from_ref_time(1024);
	pub const MaximumBlockLength: u32 = 2 * 1024;
	pub const AvailableBlockRatio: Perbill = Perbill::one();
	pub const DbWeight: RuntimeDbWeight = RuntimeDbWeight { read: 1, write: 2 };
}
impl frame_system::Config for TestRuntime {
	type AccountData = pallet_balances::AccountData<Balance>;
	type AccountId = AccountId;
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockHashCount = ConstU64<250>;
	type BlockLength = ();
	type BlockNumber = u64;
	type BlockWeights = ();
	type DbWeight = DbWeight;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type Header = SubstrateHeader;
	type Index = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type MaxConsumers = frame_support::traits::ConstU32<16>;
	type OnKilledAccount = ();
	type OnNewAccount = ();
	type OnSetCode = ();
	type PalletInfo = PalletInfo;
	type RuntimeCall = RuntimeCall;
	type RuntimeEvent = RuntimeEvent;
	type RuntimeOrigin = RuntimeOrigin;
	type SS58Prefix = ();
	type SystemWeightInfo = ();
	type Version = ();
}

impl pallet_balances::Config for TestRuntime {
	type AccountStore = frame_system::Pallet<TestRuntime>;
	type Balance = Balance;
	type DustRemoval = ();
	type ExistentialDeposit = ConstU64<1>;
	type MaxLocks = ();
	type MaxReserves = ();
	type ReserveIdentifier = ();
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
}

parameter_types! {
	pub const MaxMessagesToPruneAtOnce: u64 = 10;
	pub const MaxUnrewardedRelayerEntriesAtInboundLane: u64 = 16;
	pub const MaxUnconfirmedMessagesAtInboundLane: u64 = 32;
	pub const TestBridgedChainId: bp_runtime::ChainId = *b"test";
	pub const ActiveOutboundLanes: &'static [LaneId] = &[TEST_LANE_ID, TEST_LANE_ID_2];
}
impl Config for TestRuntime {
	type AccountIdConverter = AccountIdConverter;
	type ActiveOutboundLanes = ActiveOutboundLanes;
	type BridgedChainId = TestBridgedChainId;
	type DeliveryConfirmationPayments = TestDeliveryConfirmationPayments;
	type DeliveryPayments = TestDeliveryPayments;
	type InboundPayload = TestPayload;
	type InboundRelayer = TestRelayer;
	type LaneMessageVerifier = TestLaneMessageVerifier;
	type MaxUnconfirmedMessagesAtInboundLane = MaxUnconfirmedMessagesAtInboundLane;
	type MaxUnrewardedRelayerEntriesAtInboundLane = MaxUnrewardedRelayerEntriesAtInboundLane;
	type MaximalOutboundPayloadSize = frame_support::traits::ConstU32<MAX_OUTBOUND_PAYLOAD_SIZE>;
	type MessageDeliveryAndDispatchPayment = TestMessageDeliveryAndDispatchPayment;
	type MessageDispatch = TestMessageDispatch;
	type OutboundPayload = TestPayload;
	type RuntimeEvent = RuntimeEvent;
	type SourceHeaderChain = TestSourceHeaderChain;
	type TargetHeaderChain = TestTargetHeaderChain;
	type WeightInfo = ();
}

impl Size for TestPayload {
	fn size(&self) -> u32 {
		16 + self.extra.len() as u32
	}
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
impl From<Result<Vec<Message>, ()>> for TestMessagesProof {
	fn from(result: Result<Vec<Message>, ()>) -> Self {
		Self {
			result: result.map(|messages| {
				let mut messages_by_lane: BTreeMap<LaneId, ProvedLaneMessages<Message>> =
					BTreeMap::new();
				for message in messages {
					messages_by_lane.entry(message.key.lane_id).or_default().messages.push(message);
				}
				messages_by_lane.into_iter().collect()
			}),
		}
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
impl LaneMessageVerifier<RuntimeOrigin, TestPayload> for TestLaneMessageVerifier {
	type Error = &'static str;

	fn verify_message(
		_submitter: &RuntimeOrigin,
		_lane: &LaneId,
		_lane_outbound_data: &OutboundLaneData,
		payload: &TestPayload,
	) -> Result<(), Self::Error> {
		if !payload.reject_by_lane_verifier {
			Ok(())
		} else {
			Err(TEST_ERROR)
		}
	}
}

/// Reward payments at the target chain during delivery transaction.
#[derive(Debug, Default)]
pub struct TestDeliveryPayments;
impl TestDeliveryPayments {
	/// Returns true if given relayer has been rewarded with given balance. The reward-paid flag is
	/// cleared after the call.
	pub fn is_reward_paid(relayer: AccountId) -> bool {
		let key = (b":delivery-relayer-reward:", relayer).encode();
		frame_support::storage::unhashed::take::<bool>(&key).is_some()
	}
}

impl DeliveryPayments<AccountId> for TestDeliveryPayments {
	type Error = &'static str;

	fn pay_reward(
		relayer: AccountId,
		_total_messages: MessageNonce,
		_valid_messages: MessageNonce,
		_actual_weight: Weight,
	) {
		let key = (b":delivery-relayer-reward:", relayer).encode();
		frame_support::storage::unhashed::put(&key, &true);
	}
}

/// Reward payments at the source chain during delivery confirmation transaction.
#[derive(Debug, Default)]
pub struct TestDeliveryConfirmationPayments;

impl TestDeliveryConfirmationPayments {
	/// Returns true if given relayer has been rewarded with given balance. The reward-paid flag is
	/// cleared after the call.
	pub fn is_reward_paid(relayer: AccountId, fee: TestMessageFee) -> bool {
		let key = (b":relayer-reward:", relayer, fee).encode();
		frame_support::storage::unhashed::take::<bool>(&key).is_some()
	}
}
impl DeliveryConfirmationPayments<AccountId> for TestDeliveryConfirmationPayments {
	type Error = &'static str;

	fn pay_reward(
		_lane_id: LaneId,
		messages_relayers: VecDeque<UnrewardedRelayer<AccountId>>,
		_confirmation_relayer: &AccountId,
		received_range: &RangeInclusive<MessageNonce>,
		_relayer_fund_account: &AccountId,
	) {
		// Relayers rewards, grouped by relayer account id.
		type RelayersRewards<AccountId, Balance> = BTreeMap<AccountId, RelayerRewards<Balance>>;

		// Single relayer rewards.
		#[derive(Default)]
		struct RelayerRewards<Balance> {
			// Total rewards that are to be paid to the relayer.
			reward: Balance,
			// Total number of messages relayed by this relayer.
			messages: MessageNonce,
		}

		let relayers_rewards = calc_relayers_rewards(messages_relayers, received_range);
		for (relayer, reward) in &relayers_rewards {
			let key = (b":relayer-reward:", relayer, reward).encode();
			frame_support::storage::unhashed::put(&key, &true);
		}
	}
}

/// Source header chain that is used in tests.
#[derive(Debug)]
pub struct TestSourceHeaderChain;
impl SourceHeaderChain for TestSourceHeaderChain {
	type Error = &'static str;
	type MessagesProof = TestMessagesProof;

	fn verify_messages_proof(
		proof: Self::MessagesProof,
		_messages_count: u32,
	) -> Result<ProvedMessages<Message>, Self::Error> {
		proof.result.map(|proof| proof.into_iter().collect()).map_err(|_| TEST_ERROR)
	}
}

/// Source header chain that is used in tests.
#[derive(Debug)]
pub struct TestMessageDispatch;
impl MessageDispatch<AccountId> for TestMessageDispatch {
	type DispatchLevelResult = TestDispatchLevelResult;
	type DispatchPayload = TestPayload;

	fn dispatch_weight(message: &mut DispatchMessage<TestPayload>) -> Weight {
		match message.data.payload.as_ref() {
			Ok(payload) => payload.declared_weight,
			Err(_) => Weight::zero(),
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
		message: DispatchMessage<TestPayload>,
	) -> MessageDispatchResult<TestDispatchLevelResult> {
		match message.data.payload.as_ref() {
			Ok(payload) => payload.dispatch_result.clone(),
			Err(_) => dispatch_result(0),
		}
	}
}

/// Return test lane message with given nonce and payload.
pub fn message(nonce: MessageNonce, payload: TestPayload) -> Message {
	Message { key: MessageKey { lane_id: TEST_LANE_ID, nonce }, payload: payload.encode() }
}

/// Return valid outbound message data, constructed from given payload.
pub fn outbound_message_data(payload: TestPayload) -> MessagePayload {
	payload.encode()
}

/// Return valid inbound (dispatch) message data, constructed from given payload.
pub fn inbound_message_data(payload: TestPayload) -> DispatchMessageData<TestPayload> {
	DispatchMessageData { payload: Ok(payload) }
}

/// Constructs message payload using given arguments and zero unspent weight.
pub const fn message_payload(id: u64, declared_weight: u64) -> TestPayload {
	TestPayload {
		id,
		reject_by_lane_verifier: false,
		declared_weight: Weight::from_ref_time(declared_weight),
		dispatch_result: dispatch_result(0),
		extra: Vec::new(),
	}
}

/// Returns message dispatch result with given unspent weight.
pub const fn dispatch_result(
	unspent_weight: u64,
) -> MessageDispatchResult<TestDispatchLevelResult> {
	MessageDispatchResult {
		unspent_weight: Weight::from_ref_time(unspent_weight),
		dispatch_level_result: (),
	}
}

/// Constructs unrewarded relayer entry from nonces range and relayer id.
pub fn unrewarded_relayer(
	begin: MessageNonce,
	end: MessageNonce,
	relayer: TestRelayer,
) -> UnrewardedRelayer<TestRelayer> {
	UnrewardedRelayer { relayer, messages: DeliveredMessages { begin, end } }
}

/// Run pallet test.
pub fn run_test<T>(test: impl FnOnce() -> T) -> T {
	let mut t = frame_system::GenesisConfig::default().build_storage::<TestRuntime>().unwrap();
	pallet_balances::GenesisConfig::<TestRuntime> { balances: vec![(ENDOWED_ACCOUNT, 1_000_000)] }
		.assimilate_storage(&mut t)
		.unwrap();
	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(test)
}
