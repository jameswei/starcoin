// Copyright (c) The Libra Core Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod message;

use crate::message::{Event, Notification, ThinBlock};
use actix::{ActorContext, ActorFuture, AsyncContext, ContextFutureSpawner, WrapFuture};
use anyhow::{format_err, Result};
use starcoin_bus::{Broadcast, Bus, BusActor, Subscription};
use starcoin_crypto::hash::PlainCryptoHash;
use starcoin_logger::prelude::*;
use starcoin_storage::Store;
use starcoin_types::block::Block;
use starcoin_types::system_events::{NewHeadBlock, SyncBegin, SyncDone};
use std::sync::Arc;

/// ChainNotify watch `NewHeadBlock` message from bus,
/// and then reproduce `Notification<ThinBlock>` and `Notification<Arc<Vec<Event>>>` message to bus.
/// User can subscribe the two notification to watch onchain events.
pub struct ChainNotifyHandlerActor {
    bus: actix::Addr<BusActor>,
    store: Arc<dyn Store>,
    broadcast_txn: bool,
}

impl ChainNotifyHandlerActor {
    pub fn new(bus: actix::Addr<BusActor>, store: Arc<dyn Store>) -> Self {
        Self {
            bus,
            store,
            broadcast_txn: true,
        }
    }
}

impl actix::Actor for ChainNotifyHandlerActor {
    type Context = actix::Context<Self>;
    fn started(&mut self, ctx: &mut Self::Context) {
        let sync_begin_recipient = ctx.address().recipient::<SyncBegin>();
        self.bus
            .send(Subscription {
                recipient: sync_begin_recipient,
            })
            .into_actor(self)
            .then(|_res, act, _ctx| async {}.into_actor(act))
            .wait(ctx);

        let sync_done_recipient = ctx.address().recipient::<SyncDone>();
        self.bus
            .send(Subscription {
                recipient: sync_done_recipient,
            })
            .into_actor(self)
            .then(|_res, act, _ctx| async {}.into_actor(act))
            .wait(ctx);

        self.bus
            .clone()
            .channel::<NewHeadBlock>()
            .into_actor(self)
            .then(|res, act, ctx| {
                match res {
                    Err(e) => {
                        error!(target: "chain-notify", "fail to start event subscription actor, err: {}", &e);
                        ctx.terminate();
                    }
                    Ok(r) => {
                        ctx.add_stream(r);
                    }
                };
                async {}.into_actor(act)
            })
            .wait(ctx);
    }
}

impl actix::Handler<SyncBegin> for ChainNotifyHandlerActor {
    type Result = ();

    fn handle(&mut self, _begin: SyncBegin, _ctx: &mut Self::Context) -> Self::Result {
        self.broadcast_txn = false;
    }
}

impl actix::Handler<SyncDone> for ChainNotifyHandlerActor {
    type Result = ();
    fn handle(&mut self, _done: SyncDone, _ctx: &mut Self::Context) -> Self::Result {
        self.broadcast_txn = true;
    }
}

impl actix::StreamHandler<NewHeadBlock> for ChainNotifyHandlerActor {
    fn handle(&mut self, item: NewHeadBlock, _ctx: &mut Self::Context) {
        if self.broadcast_txn {
            let NewHeadBlock(block_detail) = item;
            let block = block_detail.get_block();
            // notify header.
            self.notify_new_block(block);

            // notify events
            if let Err(e) = self.notify_events(block, self.store.clone()) {
                error!(target: "pubsub", "fail to notify events to client, err: {}", &e);
            }
        }
    }
}

impl ChainNotifyHandlerActor {
    pub fn notify_new_block(&self, block: &Block) {
        let thin_block = ThinBlock::new(
            block.header().clone(),
            block
                .transactions()
                .iter()
                .map(|t| t.crypto_hash())
                .collect(),
        );
        self.bus.do_send(Broadcast {
            msg: Notification(thin_block),
        });
    }

    pub fn notify_events(&self, block: &Block, store: Arc<dyn Store>) -> Result<()> {
        let block_number = block.header().number();
        let block_id = block.id();
        let txn_info_ids = store.get_block_txn_info_ids(block_id)?;
        let mut all_events: Vec<Event> = vec![];
        for (_i, txn_info_id) in txn_info_ids.into_iter().enumerate().rev() {
            let txn_hash = store
                .get_transaction_info(txn_info_id)?
                .map(|info| info.transaction_hash())
                .ok_or_else(|| format_err!("cannot find txn info by it's id {}", &txn_info_id))?;
            // get events directly by txn_info_id
            let events = store.get_contract_events(txn_info_id)?.unwrap_or_default();
            all_events.extend(
                events
                    .into_iter()
                    .map(|evt| Event::new(block_id, block_number, txn_hash, None, evt)),
            );
        }
        let events = Arc::new(all_events);
        self.bus.do_send(Broadcast {
            msg: Notification(events),
        });
        Ok(())
    }
}