// Copyright 2021 Parity Technologies (UK) Ltd.
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

// darwinia-network
use crate::{
	messages::{
		source::FromBridgedChainMessagesDeliveryProof, target::FromBridgedChainMessagesProof,
	},
	BridgeRuntimeFilterCall,
};
use pallet_bridge_messages::{Config, Pallet};
// paritytech
use frame_support::{dispatch::CallableCallFor, log, traits::IsSubType};
use sp_runtime::transaction_validity::TransactionValidity;

/// Validate messages in order to avoid "mining" messages delivery and delivery confirmation
/// transactions, that are delivering outdated messages/confirmations. Without this validation,
/// even honest relayers may lose their funds if there are multiple relays running and submitting
/// the same messages/confirmations.
impl<
		BridgedHeaderHash,
		SourceHeaderChain: bp_messages::target_chain::SourceHeaderChain<
			MessagesProof = FromBridgedChainMessagesProof<BridgedHeaderHash>,
		>,
		TargetHeaderChain: bp_messages::source_chain::TargetHeaderChain<
			<T as Config<I>>::OutboundPayload,
			<T as frame_system::Config>::AccountId,
			MessagesDeliveryProof = FromBridgedChainMessagesDeliveryProof<BridgedHeaderHash>,
		>,
		Call: IsSubType<CallableCallFor<Pallet<T, I>, T>>,
		T: frame_system::Config<RuntimeCall = Call>
			+ Config<I, SourceHeaderChain = SourceHeaderChain, TargetHeaderChain = TargetHeaderChain>,
		I: 'static,
	> BridgeRuntimeFilterCall<Call> for Pallet<T, I>
{
	fn validate(call: &Call) -> TransactionValidity {
		match call.is_sub_type() {
			Some(pallet_bridge_messages::Call::<T, I>::receive_messages_proof {
				ref proof,
				..
			}) => {
				let inbound_lane_data =
					pallet_bridge_messages::InboundLanes::<T, I>::get(proof.lane);
				if proof.nonces_end <= inbound_lane_data.last_delivered_nonce() {
					log::trace!(
						target: pallet_bridge_messages::LOG_TARGET,
						"Rejecting obsolete messages delivery transaction: \
                            lane {:?}, bundled {:?}, best {:?}",
						proof.lane,
						proof.nonces_end,
						inbound_lane_data.last_delivered_nonce(),
					);

					return sp_runtime::transaction_validity::InvalidTransaction::Stale.into();
				}
			},
			Some(pallet_bridge_messages::Call::<T, I>::receive_messages_delivery_proof {
				ref proof,
				ref relayers_state,
				..
			}) => {
				let latest_delivered_nonce = relayers_state.last_delivered_nonce;

				let outbound_lane_data =
					pallet_bridge_messages::OutboundLanes::<T, I>::get(proof.lane);
				if latest_delivered_nonce <= outbound_lane_data.latest_received_nonce {
					log::trace!(
						target: pallet_bridge_messages::LOG_TARGET,
						"Rejecting obsolete messages confirmation transaction: \
                            lane {:?}, bundled {:?}, best {:?}",
						proof.lane,
						latest_delivered_nonce,
						outbound_lane_data.latest_received_nonce,
					);

					return sp_runtime::transaction_validity::InvalidTransaction::Stale.into();
				}
			},
			_ => {},
		}

		Ok(sp_runtime::transaction_validity::ValidTransaction::default())
	}
}
