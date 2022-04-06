use codec::{Compact, Decode, Encode};
use frame_support::weights::Weight;
use scale_info::TypeInfo;
use sp_runtime::FixedU128;

use bp_darwinia_core::DarwiniaLike;
use bp_messages::{LaneId, UnrewardedRelayersState};
use bp_pangolin::Balance;
use bp_runtime::Chain;

/// Unchecked pangolin extrinsic.
pub type UncheckedExtrinsic = bp_pangolin::UncheckedExtrinsic<Call>;

#[allow(clippy::large_enum_variant)]
#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
pub enum Call {
	/// System pallet.
	#[codec(index = 0)]
	System(SystemCall),
	/// Balances pallet.
	#[codec(index = 4)]
	Balances(BalancesCall),
	/// Bridge pangoro grandpa pallet.
	#[codec(index = 45)]
	BridgePangoroGrandpa(BridgePangoroGrandpaCall),
	/// Bridge pangoro messages pallet
	#[codec(index = 43)]
	BridgePangoroMessages(BridgePangoroMessagesCall),
	/// Bridge rococo grandpa pallet
	#[codec(index = 60)]
	BridgeRococoGrandpa(BridgeRococoGrandpaCall),
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
	transfer(bp_pangolin::Address, Compact<Balance>),
}

#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
#[allow(non_camel_case_types)]
pub enum BridgePangoroGrandpaCall {
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
pub enum BridgePangoroMessagesCall {
	#[codec(index = 2)]
	update_pallet_parameter(BridgePolkadotMessagesParameter),
	#[codec(index = 3)]
	send_message(
		LaneId,
		bp_message_dispatch::MessagePayload<
			bp_pangolin::AccountId,
			bp_pangoro::AccountId,
			bp_pangoro::AccountPublic,
			Vec<u8>,
		>,
		bp_pangolin::Balance,
	),
	#[codec(index = 5)]
	receive_messages_proof(
		bp_pangoro::AccountId,
		bridge_runtime_common::messages::target::FromBridgedChainMessagesProof<bp_pangoro::Hash>,
		u32,
		Weight,
	),
	#[codec(index = 6)]
	receive_messages_delivery_proof(
		bridge_runtime_common::messages::source::FromBridgedChainMessagesDeliveryProof<
			bp_pangoro::Hash,
		>,
		UnrewardedRelayersState,
	),
}

#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
#[allow(non_camel_case_types)]
pub enum BridgeRococoGrandpaCall {
	#[codec(index = 0)]
	submit_finality_proof(
		Box<<bp_rococo::Rococo as Chain>::Header>,
		bp_header_chain::justification::GrandpaJustification<<bp_rococo::Rococo as Chain>::Header>,
	),
	#[codec(index = 1)]
	initialize(bp_header_chain::InitializationData<<bp_rococo::Rococo as Chain>::Header>),
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
