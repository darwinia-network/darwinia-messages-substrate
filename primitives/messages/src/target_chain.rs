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

//! Primitives of messages module, that are used on the target chain.

use crate::{LaneId, Message, MessageData, MessageKey, OutboundLaneData, VerificationError};

use bp_runtime::{messages::MessageDispatchResult, Size};
use codec::{Decode, Encode, Error as CodecError};
use frame_support::{weights::Weight, Parameter, RuntimeDebug};
use scale_info::TypeInfo;
use sp_std::{collections::btree_map::BTreeMap, prelude::*};

/// Proved messages from the source chain.
pub type ProvedMessages<Message> = BTreeMap<LaneId, ProvedLaneMessages<Message>>;

/// Error message that is used in `ForbidOutboundMessages` implementation.
const ALL_INBOUND_MESSAGES_REJECTED: &str =
	"This chain is configured to reject all inbound messages";

/// Source chain API. Used by target chain, to verify source chain proofs.
///
/// All implementations of this trait should only work with finalized data that
/// can't change. Wrong implementation may lead to invalid lane states (i.e. lane
/// that's stuck) and/or processing messages without paying fees.
pub trait SourceHeaderChain<Fee> {
	/// Proof that messages are sent from source chain. This may also include proof
	/// of corresponding outbound lane states.
	type MessagesProof: Parameter + Size;

	/// Verify messages proof and return proved messages.
	///
	/// Returns error if either proof is incorrect, or the number of messages in the proof
	/// is not matching the `messages_count`.
	///
	/// Messages vector is required to be sorted by nonce within each lane. Out-of-order
	/// messages will be rejected.
	///
	/// The `messages_count` argument verification (sane limits) is supposed to be made
	/// outside this function. This function only verifies that the proof declares exactly
	/// `messages_count` messages.
	fn verify_messages_proof(
		proof: Self::MessagesProof,
		messages_count: u32,
	) -> Result<ProvedMessages<Message<Fee>>, VerificationError>;
}

/// Called when inbound message is received.
pub trait MessageDispatch<AccountId, Fee> {
	/// Decoded message payload type. Valid message may contain invalid payload. In this case
	/// message is delivered, but dispatch fails. Therefore, two separate types of payload
	/// (opaque `MessagePayload` used in delivery and this `DispatchPayload` used in dispatch).
	type DispatchPayload: Decode;

	/// Estimate dispatch weight.
	///
	/// This function must return correct upper bound of dispatch weight. The return value
	/// of this function is expected to match return value of the corresponding
	/// `From<Chain>InboundLaneApi::message_details().dispatch_weight` call.
	fn dispatch_weight(message: &mut DispatchMessage<Self::DispatchPayload, Fee>) -> Weight;

	/// Checking in message receiving step before dispatch
	///
	/// This will be called before the call enter dispatch phase. If failed, the message(call) will
	/// be not be processed by this relayer, latter relayers can still continue process it.
	fn pre_dispatch(
		relayer_account: &AccountId,
		message: &DispatchMessage<Self::DispatchPayload, Fee>,
	) -> Result<(), &'static str>;

	/// Called when inbound message is received.
	///
	/// It is up to the implementers of this trait to determine whether the message
	/// is invalid (i.e. improperly encoded, has too large weight, ...) or not.
	///
	/// If your configuration allows paying dispatch fee at the target chain, then
	/// it must be paid inside this method to the `relayer_account`.
	fn dispatch(
		relayer_account: &AccountId,
		message: DispatchMessage<Self::DispatchPayload, Fee>,
	) -> MessageDispatchResult;
}

/// Proved messages from single lane of the source chain.
#[derive(Clone, PartialEq, Eq, Encode, Decode, RuntimeDebug, TypeInfo)]
pub struct ProvedLaneMessages<Message> {
	/// Optional outbound lane state.
	pub lane_state: Option<OutboundLaneData>,
	/// Messages sent through this lane.
	pub messages: Vec<Message>,
}
impl<Message> Default for ProvedLaneMessages<Message> {
	fn default() -> Self {
		ProvedLaneMessages { lane_state: None, messages: Vec::new() }
	}
}

/// Message data with decoded dispatch payload.
#[derive(RuntimeDebug)]
pub struct DispatchMessageData<DispatchPayload, Fee> {
	/// Result of dispatch payload decoding.
	pub payload: Result<DispatchPayload, CodecError>,
	/// Message delivery and dispatch fee, paid by the submitter.
	pub fee: Fee,
}
impl<DispatchPayload: Decode, Fee> From<MessageData<Fee>>
	for DispatchMessageData<DispatchPayload, Fee>
{
	fn from(data: MessageData<Fee>) -> Self {
		DispatchMessageData {
			payload: DispatchPayload::decode(&mut &data.payload[..]),
			fee: data.fee,
		}
	}
}

/// Message with decoded dispatch payload.
#[derive(RuntimeDebug)]
pub struct DispatchMessage<DispatchPayload, Fee> {
	/// Message key.
	pub key: MessageKey,
	/// Message data with decoded dispatch payload.
	pub data: DispatchMessageData<DispatchPayload, Fee>,
}
impl<DispatchPayload: Decode, Fee> From<Message<Fee>> for DispatchMessage<DispatchPayload, Fee> {
	fn from(message: Message<Fee>) -> Self {
		DispatchMessage { key: message.key, data: message.data.into() }
	}
}

/// Structure that may be used in place of `SourceHeaderChain` and `MessageDispatch` on chains,
/// where inbound messages are forbidden.
pub struct ForbidInboundMessages;
impl<Fee> SourceHeaderChain<Fee> for ForbidInboundMessages {
	type MessagesProof = ();

	fn verify_messages_proof(
		_proof: Self::MessagesProof,
		_messages_count: u32,
	) -> Result<ProvedMessages<Message<Fee>>, VerificationError> {
		Err(VerificationError::Other(ALL_INBOUND_MESSAGES_REJECTED))
	}
}
impl<AccountId, Fee> MessageDispatch<AccountId, Fee> for ForbidInboundMessages {
	type DispatchPayload = ();

	fn dispatch_weight(_message: &mut DispatchMessage<Self::DispatchPayload, Fee>) -> Weight {
		Weight::MAX
	}

	fn pre_dispatch(
		_: &AccountId,
		_message: &DispatchMessage<Self::DispatchPayload, Fee>,
	) -> Result<(), &'static str> {
		Ok(())
	}

	fn dispatch(
		_: &AccountId,
		_: DispatchMessage<Self::DispatchPayload, Fee>,
	) -> MessageDispatchResult {
		MessageDispatchResult {
			dispatch_result: false,
			unspent_weight: Weight::zero(),
			dispatch_fee_paid_during_dispatch: false,
		}
	}
}
