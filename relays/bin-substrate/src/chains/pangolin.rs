use crate::cli::{
	bridge,
	encode_call::{self, Call, CliEncodeCall},
	encode_message, send_message, CliChain,
};
use bp_message_dispatch::{CallOrigin, MessagePayload};
use codec::Decode;
use frame_support::weights::{GetDispatchInfo, Weight};
use relay_pangolin_client::PangolinRelayChain;
use sp_version::RuntimeVersion;

impl CliEncodeCall for PangolinRelayChain {
	fn max_extrinsic_size() -> u32 {
		drml_primitives::max_extrinsic_size()
	}

	fn encode_call(call: &Call) -> anyhow::Result<Self::Call> {
		Ok(match call {
			Call::Raw { data } => Decode::decode(&mut &*data.0)?,
			Call::Remark { remark_payload, .. } => pangolin_runtime::Call::System(
				pangolin_runtime::SystemCall::remark(remark_payload.as_ref().map(|x| x.0.clone()).unwrap_or_default()),
			),
			Call::Transfer { recipient, amount } => pangolin_runtime::Call::Balances(
				// todo: there need correct it
				pangolin_runtime::BalanceRingCall::transfer(
					sp_runtime::MultiAddress::Id(recipient.raw_id()),
					amount.cast() as u128,
				),
			),
			Call::BridgeSendMessage {
				lane,
				payload,
				fee,
				bridge_instance_index,
			} => match *bridge_instance_index {
				bridge::PANGOLIN_TO_MILLAU_INDEX => {
					let payload = Decode::decode(&mut &*payload.0)?;
					pangolin_runtime::Call::BridgeMillauMessages(
						pangolin_runtime::bridge::s2s::MessagesCall::send_message(lane.0, payload, fee.cast() as u128),
					)
				}
				_ => anyhow::bail!(
					"Unsupported target bridge pallet with instance index: {}",
					bridge_instance_index
				),
			},
		})
	}
}

impl CliChain for PangolinRelayChain {
	const RUNTIME_VERSION: RuntimeVersion = pangolin_runtime::VERSION;

	type KeyPair = sp_core::sr25519::Pair;
	type MessagePayload =
		MessagePayload<drml_primitives::AccountId, bp_millau::AccountSigner, bp_millau::Signature, Vec<u8>>;

	fn ss58_format() -> u16 {
		pangolin_runtime::SS58Prefix::get() as u16
	}

	fn max_extrinsic_weight() -> Weight {
		drml_primitives::max_extrinsic_weight()
	}

	// TODO [#854|#843] support multiple bridges?
	fn encode_message(message: encode_message::MessagePayload) -> Result<Self::MessagePayload, String> {
		match message {
			encode_message::MessagePayload::Raw { data } => MessagePayload::decode(&mut &*data.0)
				.map_err(|e| format!("Failed to decode Pangolin's MessagePayload: {:?}", e)),
			encode_message::MessagePayload::Call { mut call, mut sender } => {
				type Source = PangolinRelayChain;
				type Target = relay_millau_client::Millau;

				sender.enforce_chain::<Source>();
				let spec_version = Target::RUNTIME_VERSION.spec_version;
				let origin = CallOrigin::SourceAccount(sender.raw_id());
				encode_call::preprocess_call::<Source, Target>(&mut call, bridge::PANGOLIN_TO_MILLAU_INDEX);
				let call = Target::encode_call(&call).map_err(|e| e.to_string())?;
				let weight = call.get_dispatch_info().weight;

				Ok(send_message::message_payload(spec_version, weight, origin, &call))
			}
		}
	}
}
