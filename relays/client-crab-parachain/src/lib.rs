use std::time::Duration;

use codec::Encode;
use frame_support::weights::{IdentityFee, Weight};
use sp_core::{storage::StorageKey, Pair};
use sp_runtime::{generic::SignedPayload, traits::IdentifyAccount};

use bp_messages::MessageNonce;
use relay_substrate_client::{
	Chain, ChainBase, ChainWithBalances, ChainWithMessages, SignParam, TransactionSignScheme,
	UnsignedTransaction,
};

pub mod runtime;

/// CrabParachain header id.
pub type HeaderId =
	relay_utils::HeaderId<bp_crab_parachain::Hash, bp_crab_parachain::BlockNumber>;

/// CrabParachain chain definition.
#[derive(Debug, Clone, Copy)]
pub struct CrabParachainChain;

impl ChainBase for CrabParachainChain {
	type BlockNumber = bp_crab_parachain::BlockNumber;
	type Hash = bp_crab_parachain::Hash;
	type Hasher = bp_crab_parachain::Hashing;
	type Header = bp_crab_parachain::Header;

	type AccountId = bp_crab_parachain::AccountId;
	type Balance = bp_crab_parachain::Balance;
	type Index = bp_crab_parachain::Nonce;
	type Signature = bp_crab_parachain::Signature;

	fn max_extrinsic_size() -> u32 {
		bp_crab_parachain::CrabParachain::max_extrinsic_size()
	}

	fn max_extrinsic_weight() -> Weight {
		bp_crab_parachain::CrabParachain::max_extrinsic_weight()
	}
}

impl Chain for CrabParachainChain {
	const NAME: &'static str = "CrabParachain";
	const TOKEN_ID: Option<&'static str> = Some("polkadot");
	const BEST_FINALIZED_HEADER_ID_METHOD: &'static str =
		bp_crab_parachain::BEST_FINALIZED_CRAB_PARACHAIN_HEADER_METHOD;
	const AVERAGE_BLOCK_INTERVAL: Duration =
		Duration::from_millis(bp_crab_parachain::MILLISECS_PER_BLOCK);
	const STORAGE_PROOF_OVERHEAD: u32 = bp_crab_parachain::EXTRA_STORAGE_PROOF_SIZE;
	const MAXIMAL_ENCODED_ACCOUNT_ID_SIZE: u32 =
		bp_crab_parachain::MAXIMAL_ENCODED_ACCOUNT_ID_SIZE;

	type SignedBlock = bp_crab_parachain::SignedBlock;
	type Call = crate::runtime::Call;
	type WeightToFee = IdentityFee<bp_crab_parachain::Balance>;
}

impl ChainWithMessages for CrabParachainChain {
	const WITH_CHAIN_MESSAGES_PALLET_NAME: &'static str =
		bp_crab_parachain::WITH_CRAB_PARACHAIN_MESSAGES_PALLET_NAME;
	const TO_CHAIN_MESSAGE_DETAILS_METHOD: &'static str =
		bp_crab_parachain::TO_CRAB_PARACHAIN_MESSAGE_DETAILS_METHOD;
	const TO_CHAIN_LATEST_GENERATED_NONCE_METHOD: &'static str =
		bp_crab_parachain::TO_CRAB_PARACHAIN_LATEST_GENERATED_NONCE_METHOD;
	const TO_CHAIN_LATEST_RECEIVED_NONCE_METHOD: &'static str =
		bp_crab_parachain::TO_CRAB_PARACHAIN_LATEST_RECEIVED_NONCE_METHOD;
	const FROM_CHAIN_LATEST_RECEIVED_NONCE_METHOD: &'static str =
		bp_crab_parachain::FROM_CRAB_PARACHAIN_LATEST_RECEIVED_NONCE_METHOD;
	const FROM_CHAIN_LATEST_CONFIRMED_NONCE_METHOD: &'static str =
		bp_crab_parachain::FROM_CRAB_PARACHAIN_LATEST_CONFIRMED_NONCE_METHOD;
	const FROM_CHAIN_UNREWARDED_RELAYERS_STATE: &'static str =
		bp_crab_parachain::FROM_CRAB_PARACHAIN_UNREWARDED_RELAYERS_STATE;
	const PAY_INBOUND_DISPATCH_FEE_WEIGHT_AT_CHAIN: Weight =
		bp_crab_parachain::PAY_INBOUND_DISPATCH_FEE_WEIGHT;
	const MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX: MessageNonce =
		bp_crab_parachain::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX;
	const MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX: MessageNonce =
		bp_crab_parachain::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX;
	type WeightInfo = ();
}

impl ChainWithBalances for CrabParachainChain {
	fn account_info_storage_key(account_id: &Self::AccountId) -> StorageKey {
		StorageKey(bp_crab_parachain::account_info_storage_key(account_id))
	}
}

impl TransactionSignScheme for CrabParachainChain {
	type Chain = CrabParachainChain;
	type AccountKeyPair = sp_core::sr25519::Pair;
	type SignedTransaction = crate::runtime::UncheckedExtrinsic;

	fn sign_transaction(param: SignParam<Self>) -> Self::SignedTransaction {
		let raw_payload = SignedPayload::new(
			param.unsigned.call.clone(),
			bp_crab_parachain::SignedExtensions::new(
				param.spec_version,
				param.transaction_version,
				param.era,
				param.genesis_hash,
				param.unsigned.nonce,
				param.unsigned.tip,
			),
		)
		.expect("SignedExtension never fails.");

		let signature = raw_payload.using_encoded(|payload| param.signer.sign(payload));
		let signer: sp_runtime::MultiSigner = param.signer.public().into();
		let (call, extra, _) = raw_payload.deconstruct();

		bp_crab_parachain::UncheckedExtrinsic::new_signed(
			call,
			sp_runtime::MultiAddress::Id(signer.into_account()),
			signature.into(),
			extra,
		)
	}

	fn is_signed(tx: &Self::SignedTransaction) -> bool {
		tx.signature.is_some()
	}

	fn is_signed_by(signer: &Self::AccountKeyPair, tx: &Self::SignedTransaction) -> bool {
		tx.signature
			.as_ref()
			.map(|(address, _, _)| {
				let account_id: bp_crab_parachain::AccountId =
					(*signer.public().as_array_ref()).into();
				*address == bp_crab_parachain::Address::from(account_id)
			})
			.unwrap_or(false)
	}

	fn parse_transaction(tx: Self::SignedTransaction) -> Option<UnsignedTransaction<Self::Chain>> {
		let extra = &tx.signature.as_ref()?.2;
		Some(UnsignedTransaction { call: tx.function, nonce: extra.nonce(), tip: extra.tip() })
	}
}
