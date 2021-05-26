use crate::{*, mock::*};

#[test]
fn initialize_storage_should_works() {
	run_test(|ctx| {
		initialize_storage::<TestRuntime>(&ctx.genesis);
	})
}

// #[test]
// fn verify_and_update_authority_set_unsigned_should_not_work() {
// 	let df = BSCHeader::Default();
// 	run_test(|_|{
// 		ctx
// 	})
// }
