use codec::{Compact, Decode, Encode};
use frame_support::weights::Weight;
use scale_info::TypeInfo;
use sp_runtime::FixedU128;

use bp_darwinia_core::DarwiniaLike;
use bp_messages::{LaneId, UnrewardedRelayersState};
use bp_pangoro::Balance;
use bp_runtime::Chain;

/// Unchecked pangoro extrinsic.
pub type UncheckedExtrinsic = bp_pangoro::UncheckedExtrinsic<Call>;

#[allow(clippy::large_enum_variant)]
#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
pub enum Call {
	/// System pallet.
	#[codec(index = 0)]
	System(SystemCall),
	/// Balances pallet.
	#[codec(index = 4)]
	Balances(BalancesCall),
	/// Bridge pangolin grandpa pallet.
	#[codec(index = 19)]
	BridgePangolinGrandpa(BridgePangolinGrandpaCall),
	/// Bridge pangolin messages pallet
	#[codec(index = 17)]
	BridgePangolinMessages(BridgePangolinMessagesCall),
	/// Feemarket pallet
	#[codec(index = 22)]
	Feemarket(FeemarketCall),
}

#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
#[allow(non_camel_case_types)]
pub enum SystemCall {
	#[codec(index = 1)]
	remark(Vec<u8>),
}

#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
#[allow(non_camel_case_types)]
pub enum BalancesCall {
	#[codec(index = 0)]
	transfer(bp_pangoro::Address, Compact<Balance>),
}

#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
#[allow(non_camel_case_types)]
pub enum BridgePangolinGrandpaCall {
	#[codec(index = 0)]
	submit_finality_proof(
		Box<<DarwiniaLike as Chain>::Header>,
		bp_header_chain::justification::GrandpaJustification<<DarwiniaLike as Chain>::Header>,
	),
	#[codec(index = 1)]
	initialize(bp_header_chain::InitializationData<<DarwiniaLike as Chain>::Header>),
}

#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
#[allow(non_camel_case_types)]
pub enum BridgePangolinMessagesCall {
	#[codec(index = 2)]
	update_pallet_parameter(BridgePolkadotMessagesParameter),
	#[codec(index = 3)]
	send_message(
		LaneId,
		bp_message_dispatch::MessagePayload<
			bp_pangoro::AccountId,
			bp_pangolin::AccountId,
			bp_pangolin::AccountPublic,
			Vec<u8>,
		>,
		bp_pangoro::Balance,
	),
	#[codec(index = 5)]
	receive_messages_proof(
		bp_pangolin::AccountId,
		bridge_runtime_common::messages::target::FromBridgedChainMessagesProof<bp_pangolin::Hash>,
		u32,
		Weight,
	),
	#[codec(index = 6)]
	receive_messages_delivery_proof(
		bridge_runtime_common::messages::source::FromBridgedChainMessagesDeliveryProof<
			bp_pangolin::Hash,
		>,
		UnrewardedRelayersState,
	),
}

#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
pub enum BridgePolkadotMessagesParameter {
	#[codec(index = 0)]
	PolkadotToKusamaConversionRate(FixedU128),
}

impl sp_runtime::traits::Dispatchable for Call {
	type Origin = ();
	type Config = ();
	type Info = ();
	type PostInfo = ();

	fn dispatch(self, _origin: Self::Origin) -> sp_runtime::DispatchResultWithInfo<Self::PostInfo> {
		unimplemented!("The Call is not expected to be dispatched.")
	}
}

/// Feemarket call
#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
#[allow(non_camel_case_types)]
enum FeemarketCall {
	#[codec(index = 0)]
	enroll_and_lock_collateral(bp_pangoro::Balance, Option<bp_pangoro::Balance>),
	#[codec(index = 1)]
	update_locked_collateral(bp_pangoro::Balance),
	#[codec(index = 2)]
	update_relay_fee(bp_pangoro::Balance),
	#[codec(index = 3)]
	cancel_enrollment(),
	#[codec(index = 4)]
	set_slash_protect(bp_pangoro::Balance),
	#[codec(index = 5)]
	set_assigned_relayers_number(u32),
}
