// This file is part of Darwinia.
//
// Copyright (C) 2018-2022 Darwinia Network
// SPDX-License-Identifier: GPL-3.0
//
// Darwinia is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Darwinia is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Darwinia. If not, see <https://www.gnu.org/licenses/>.

// darwinia-network
use crate::{types::Order, *};
use bp_messages::{
	source_chain::{OnDeliveryConfirmed, OnMessageAccepted},
	DeliveredMessages, LaneId, MessageNonce,
};

pub struct FeeMarketMessageAcceptedHandler<T, I>(PhantomData<(T, I)>);
impl<T: Config<I>, I: 'static> OnMessageAccepted for FeeMarketMessageAcceptedHandler<T, I> {
	// Called when the message is accepted by message pallet
	fn on_messages_accepted(lane: &LaneId, message: &MessageNonce) -> Weight {
		// Create a new order based on the latest block, assign relayers which have priority to
		// relaying
		let now = frame_system::Pallet::<T>::block_number();
		if let Some(assigned_relayers) = <Pallet<T, I>>::assigned_relayers() {
			let order = Order::new(
				*lane,
				*message,
				now,
				T::CollateralPerOrder::get(),
				assigned_relayers.clone(),
				T::Slot::get(),
			);

			// Store the create order
			<Orders<T, I>>::insert((order.lane, order.message), order.clone());
			// Once order is created, the assigned relayers's order capacity should reduce by one.
			// Thus, the whole market needs to re-sort to generate new assigned relayers set.
			let _ = Pallet::<T, I>::update_market(|| Ok(()), None);

			let ids: Vec<T::AccountId> = assigned_relayers.iter().map(|r| r.id.clone()).collect();
			Pallet::<T, I>::deposit_event(Event::OrderCreated(
				order.lane,
				order.message,
				order.fee(),
				ids,
				order.range_end(),
			));
		}

		// Storage: FeeMarket AssignedRelayers (r:1 w:0)
		// Storage: FeeMarket Orders (r:0 w:1)
		// Storage: System Events (r:0 w:1)
		<T as frame_system::Config>::DbWeight::get().reads_writes(1, 1)
	}
}

pub struct FeeMarketMessageConfirmedHandler<T, I>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> OnDeliveryConfirmed for FeeMarketMessageConfirmedHandler<T, I> {
	fn on_messages_delivered(lane: &LaneId, delivered_messages: &DeliveredMessages) -> Weight {
		let now = frame_system::Pallet::<T>::block_number();
		for message_nonce in delivered_messages.begin..=delivered_messages.end {
			if let Some(order) = <Orders<T, I>>::get((lane, message_nonce)) {
				if !order.is_confirmed() {
					<Orders<T, I>>::mutate((lane, message_nonce), |order| match order {
						Some(order) => order.set_confirm_time(Some(now)),
						None => {},
					});

					// Once order is confirmed, the assigned relayers's order capacity should
					// increase by one. Thus, the whole market needs to re-sort to generate new
					// assigned relayers set.
					let _ = Pallet::<T, I>::update_market(|| Ok(()), None);
				}
			}
		}

		// Storage: FeeMarket Orders (r:1 w:1)
		<T as frame_system::Config>::DbWeight::get().reads_writes(1, 1)
	}
}
