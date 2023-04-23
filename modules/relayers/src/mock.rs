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

#![cfg(test)]

use crate as pallet_bridge_relayers;

use bp_messages::LaneId;
use bp_relayers::PaymentProcedure;
use frame_support::{parameter_types, weights::RuntimeDbWeight};
use sp_core::H256;
use sp_runtime::{
	testing::Header as SubstrateHeader,
	traits::{BlakeTwo256, IdentityLookup},
};

pub type AccountId = u64;
pub type Balance = u64;

type Block = frame_system::mocking::MockBlock<TestRuntime>;
type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<TestRuntime>;

frame_support::construct_runtime! {
	pub enum TestRuntime where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
		Balances: pallet_balances::{Pallet, Event<T>},
		Relayers: pallet_bridge_relayers::{Pallet, Call, Event<T>},
	}
}

impl pallet_balances::Config for TestRuntime {
	type AccountStore = frame_system::Pallet<TestRuntime>;
	type Balance = Balance;
	type DustRemoval = ();
	type ExistentialDeposit = frame_support::traits::ConstU64<1>;
	type MaxLocks = ();
	type MaxReserves = ();
	type ReserveIdentifier = ();
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
}

parameter_types! {
	pub const TestBridgedChainId: bp_runtime::ChainId = *b"test";
	pub ActiveOutboundLanes: &'static [bp_messages::LaneId] = &[[0, 0, 0, 0]];
}

// we're not testing messages pallet here, so values in this config might be crazy
impl pallet_bridge_messages::Config for TestRuntime {
	type ActiveOutboundLanes = ActiveOutboundLanes;
	type BridgedChainId = TestBridgedChainId;
	type InboundPayload = ();
	type InboundRelayer = AccountId;
	type LaneMessageVerifier = ForbidOutboundMessages;
	type MaxUnconfirmedMessagesAtInboundLane = frame_support::traits::ConstU64<8>;
	type MaxUnrewardedRelayerEntriesAtInboundLane = frame_support::traits::ConstU64<8>;
	type MaximalOutboundPayloadSize = frame_support::traits::ConstU32<1024>;
	type MessageDeliveryAndDispatchPayment = ();
	type MessageDispatch = ForbidInboundMessages;
	type OutboundPayload = ();
	type RuntimeEvent = RuntimeEvent;
	type SourceHeaderChain = ForbidInboundMessages;
	type TargetHeaderChain = ForbidOutboundMessages;
	type WeightInfo = ();
}

impl pallet_bridge_relayers::Config for TestRuntime {
	type PaymentProcedure = TestPaymentProcedure;
	type Reward = Balance;
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
}

/// Message lane that we're using in tests.
pub const TEST_LANE_ID: LaneId = [0, 0, 0, 0];

/// Regular relayer that may receive rewards.
pub const REGULAR_RELAYER: AccountId = 1;

/// Relayer that can't receive rewards.
pub const FAILING_RELAYER: AccountId = 2;

/// Payment procedure that rejects payments to the `FAILING_RELAYER`.
pub struct TestPaymentProcedure;

impl PaymentProcedure<AccountId, Balance> for TestPaymentProcedure {
	type Error = ();

	fn pay_reward(
		relayer: &AccountId,
		_lane_id: LaneId,
		_reward: Balance,
	) -> Result<(), Self::Error> {
		match *relayer {
			FAILING_RELAYER => Err(()),
			_ => Ok(()),
		}
	}
}

/// Run pallet test.
pub fn run_test<T>(test: impl FnOnce() -> T) -> T {
	let t = frame_system::GenesisConfig::default().build_storage::<TestRuntime>().unwrap();
	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(test)
}
