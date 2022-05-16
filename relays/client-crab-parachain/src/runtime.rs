use codec::{Compact, Decode, Encode};
use frame_support::weights::Weight;
use scale_info::TypeInfo;
use sp_runtime::FixedU128;

use bp_darwinia_core::DarwiniaLike;
use bp_messages::{LaneId, UnrewardedRelayersState};
use bp_crab_parachain::Balance;
use bp_runtime::Chain;

/// Unchecked crab extrinsic.
pub type UncheckedExtrinsic = bp_crab_parachain::UncheckedExtrinsic<Call>;

#[allow(clippy::large_enum_variant)]
#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
pub enum Call {
	/// System pallet.
	#[codec(index = 0)]
	System(SystemCall),
	/// Balances pallet.
	#[codec(index = 5)]
	Balances(BalancesCall),
	/// Bridge pangoro grandpa pallet.
	#[codec(index = 20)]
	BridgeCrabGrandpa(BridgeCrabGrandpaCall),
	/// Bridge pangoro messages pallet
	#[codec(index = 21)]
	BridgeCrabMessages(BridgeCrabMessagesCall),
	/// Feemarket pallet
	#[codec(index = 23)]
	CrabFeemarket(FeemarketCall),
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
	transfer(bp_crab_parachain::Address, Compact<Balance>),
}

#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
#[allow(non_camel_case_types)]
pub enum BridgeCrabGrandpaCall {
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
pub enum BridgeCrabMessagesCall {
	#[codec(index = 2)]
	update_pallet_parameter(BridgePolkadotMessagesParameter),
	#[codec(index = 3)]
	send_message(
		LaneId,
		bp_message_dispatch::MessagePayload<
			bp_crab_parachain::AccountId,
			bp_crab::AccountId,
			bp_crab::AccountPublic,
			Vec<u8>,
		>,
		bp_crab_parachain::Balance,
	),
	#[codec(index = 5)]
	receive_messages_proof(
		bp_crab::AccountId,
		bridge_runtime_common::messages::target::FromBridgedChainMessagesProof<bp_crab::Hash>,
		u32,
		Weight,
	),
	#[codec(index = 6)]
	receive_messages_delivery_proof(
		bridge_runtime_common::messages::source::FromBridgedChainMessagesDeliveryProof<
			bp_crab::Hash,
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
pub enum FeemarketCall {
	#[codec(index = 0)]
	enroll_and_lock_collateral(
		bp_crab_parachain::Balance,
		Option<bp_crab_parachain::Balance>,
	),
	#[codec(index = 1)]
	update_locked_collateral(bp_crab_parachain::Balance),
	#[codec(index = 2)]
	update_relay_fee(bp_crab_parachain::Balance),
	#[codec(index = 3)]
	cancel_enrollment(),
	#[codec(index = 4)]
	set_slash_protect(bp_crab_parachain::Balance),
	#[codec(index = 5)]
	set_assigned_relayers_number(u32),
}
