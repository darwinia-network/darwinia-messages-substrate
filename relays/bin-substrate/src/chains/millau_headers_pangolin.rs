use crate::finality_pipeline::{SubstrateFinalitySyncPipeline, SubstrateFinalityToSubstrate};

use bp_header_chain::justification::GrandpaJustification;
use codec::Encode;
use relay_millau_client::{Millau, SyncHeader as MillauSyncHeader};
use pangolin_runtime::bridge::s2s::{PangolinRelayChain, SigningParams as PangolinSigningParams};
use relay_substrate_client::{Chain, TransactionSignScheme};
use sp_core::{Bytes, Pair};


/// Millau-to-Rialto finality sync pipeline.
pub(crate) type MillauFinalityToPangolin = SubstrateFinalityToSubstrate<
	Millau,
	PangolinRelayChain,
	PangolinSigningParams
>;


impl SubstrateFinalitySyncPipeline for MillauFinalityToPangolin {
	const BEST_FINALIZED_SOURCE_HEADER_ID_AT_TARGET: &'static str =
		bp_millau::BEST_FINALIZED_MILLAU_HEADER_METHOD;

	type TargetChain = PangolinRelayChain;

	fn transactions_author(&self) -> bp_rialto::AccountId {
		(*self.target_sign.public().as_array_ref()).into()
	}

	fn make_submit_finality_proof_transaction(
		&self,
		transaction_nonce: <PangolinRelayChain as Chain>::Index,
		header: MillauSyncHeader,
		proof: GrandpaJustification<bp_millau::Header>,
	) -> Bytes {
		let call = pangolin_runtime::BridgeGrandpaMillauCall::submit_finality_proof(header.into_inner(), proof).into();

		let genesis_hash = *self.target_client.genesis_hash();
		let transaction = PangolinRelayChain::sign_transaction(genesis_hash, &self.target_sign, transaction_nonce, call);

		Bytes(transaction.encode())
	}
}



