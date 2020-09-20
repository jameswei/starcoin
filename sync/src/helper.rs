use crate::verified_rpc_client::{RpcEntryVerify, RpcVerify};
use anyhow::{format_err, Result};
use crypto::hash::HashValue;
use crypto::hash::PlainCryptoHash;
use logger::prelude::*;
use network_api::NetworkService;
use rand::prelude::IteratorRandom;
use starcoin_accumulator::node::AccumulatorStoreType;
use starcoin_accumulator::AccumulatorNode;
use starcoin_network_rpc_api::{
    gen_client::NetworkRpcClient, BlockBody, GetAccumulatorNodeByNodeHash, GetBlockHeaders,
    GetBlockHeadersByNumber, GetTxns, TransactionsData,
};
use starcoin_state_tree::StateNode;
use starcoin_types::{
    block::{BlockHeader, BlockInfo, BlockNumber},
    peer_info::PeerId,
    transaction::TransactionInfo,
};

const HEAD_CT: usize = 10;
const STABLELIZE_BLCOK_NUM: usize = 7;

pub async fn get_txns(
    client: &NetworkRpcClient,
    peer_id: PeerId,
    req: GetTxns,
) -> Result<TransactionsData> {
    let data = client.get_txns(peer_id, req.clone()).await?;
    if req.ids.is_some() {
        let mut verify_condition: RpcEntryVerify<HashValue> = (&req).into();
        let verified_txns = verify_condition
            .filter((*data.get_txns()).to_vec(), |txn| -> HashValue {
                txn.crypto_hash()
            });
        Ok(TransactionsData {
            txns: verified_txns,
        })
    } else {
        Ok(data)
    }
}

pub async fn get_txn_infos(
    client: &NetworkRpcClient,
    peer_id: PeerId,
    block_id: HashValue,
) -> Result<Option<Vec<TransactionInfo>>> {
    client.get_txn_infos(peer_id, block_id).await
}

pub async fn get_headers_by_number(
    client: &NetworkRpcClient,
    peer_id: PeerId,
    req: GetBlockHeadersByNumber,
) -> Result<Vec<BlockHeader>> {
    let mut verify_condition: RpcEntryVerify<BlockNumber> = (&req).into();
    let data = client.get_headers_by_number(peer_id, req).await?;
    let verified_headers =
        verify_condition.filter(data, |header| -> BlockNumber { header.number() });
    Ok(verified_headers)
}

pub async fn get_headers_with_peer(
    client: &NetworkRpcClient,
    peer_id: PeerId,
    req: GetBlockHeaders,
    number: BlockNumber,
) -> Result<Vec<BlockHeader>> {
    let mut verify_condition: RpcEntryVerify<BlockNumber> =
        (&req.clone().into_numbers(number)).into();
    let data = client.get_headers_with_peer(peer_id, req).await?;
    let verified_headers =
        verify_condition.filter(data, |header| -> BlockNumber { header.number() });
    Ok(verified_headers)
}

pub async fn get_headers<N>(
    network: &N,
    client: &NetworkRpcClient,
    req: GetBlockHeaders,
    number: BlockNumber,
) -> Result<(Vec<BlockHeader>, PeerId)>
where
    N: NetworkService + 'static,
{
    let peers = network.best_peer_set().await?;

    // random select a peer has enough blocks to get from.
    let random_peer = peers
        .iter()
        .filter(|peer| {
            peer.latest_header.number >= number + req.max_size as u64 + STABLELIZE_BLCOK_NUM as u64
        })
        .choose(&mut rand::thread_rng())
        .map(|p| p.peer_id.clone());
    // or else, backward to use best peer.
    let selected_peer = random_peer.or_else(|| peers.first().map(|p| p.peer_id.clone()));

    if let Some(peer_id) = selected_peer {
        debug!("rpc select peer {}", &peer_id);
        get_headers_with_peer(client, peer_id.clone(), req, number)
            .await
            .map(|headers| (headers, peer_id))
    } else {
        Err(format_err!("Can not get peer when sync block header."))
    }
}

pub async fn _get_header_by_hash<N>(
    network: &N,
    client: &NetworkRpcClient,
    hashes: Vec<HashValue>,
) -> Result<Vec<BlockHeader>>
where
    N: NetworkService + 'static,
{
    if let Some(peer_info) = network.best_peer().await? {
        let mut verify_condition: RpcEntryVerify<HashValue> = (&hashes).into();
        let data = client
            .get_header_by_hash(peer_info.get_peer_id(), hashes)
            .await?;
        let verified_headers = verify_condition.filter(data, |header| -> HashValue { header.id() });
        Ok(verified_headers)
    } else {
        Err(format_err!("Can not get peer when sync block header."))
    }
}

pub async fn get_body_by_hash<N>(
    client: &NetworkRpcClient,
    network: &N,
    hashes: Vec<HashValue>,
    max_height: BlockNumber,
) -> Result<(Vec<BlockBody>, PeerId)>
where
    N: NetworkService + 'static,
{
    let peers = network.best_peer_set().await?;
    // random select a peer who has enough block
    let random_peer = peers
        .iter()
        .filter(|peer| peer.latest_header.number >= max_height + STABLELIZE_BLCOK_NUM as u64)
        .choose(&mut rand::thread_rng());
    // or else fall back to use best peer
    let selected_peer = random_peer.or_else(|| peers.first());

    if let Some(peer_info) = selected_peer {
        let peer_id = peer_info.get_peer_id();
        debug!("rpc select peer {}", &peer_id);
        let mut verify_condition: RpcEntryVerify<HashValue> = (&hashes).into();
        let data = client.get_body_by_hash(peer_id.clone(), hashes).await?;
        let verified_bodies = verify_condition.filter(data, |body| -> HashValue { body.id() });
        Ok((verified_bodies, peer_id))
    } else {
        Err(format_err!("Can not get peer when sync block body."))
    }
}

pub async fn get_info_by_hash(
    client: &NetworkRpcClient,
    peer_id: PeerId,
    hashes: Vec<HashValue>,
) -> Result<Vec<BlockInfo>> {
    let mut verify_condition: RpcEntryVerify<HashValue> = (&hashes).into();
    let data = client.get_info_by_hash(peer_id, hashes).await?;
    let verified_infos = verify_condition.filter(data, |info| -> HashValue { *info.block_id() });
    Ok(verified_infos)
}

pub async fn get_state_node_by_node_hash(
    client: &NetworkRpcClient,
    peer_id: PeerId,
    node_key: HashValue,
) -> Result<StateNode> {
    if let Some(state_node) = client
        .get_state_node_by_node_hash(peer_id, node_key)
        .await?
    {
        let state_node_id = state_node.inner().hash();
        if node_key == state_node_id {
            Ok(state_node)
        } else {
            Err(format_err!(
                "State node hash {:?} and node key {:?} mismatch.",
                state_node_id,
                node_key
            ))
        }
    } else {
        Err(format_err!(
            "State node is none by node key {:?}.",
            node_key
        ))
    }
}

pub async fn get_accumulator_node_by_node_hash(
    client: &NetworkRpcClient,
    peer_id: PeerId,
    node_key: HashValue,
    accumulator_type: AccumulatorStoreType,
) -> Result<AccumulatorNode> {
    if let Some(accumulator_node) = client
        .get_accumulator_node_by_node_hash(
            peer_id,
            GetAccumulatorNodeByNodeHash {
                node_hash: node_key,
                accumulator_storage_type: accumulator_type,
            },
        )
        .await?
    {
        let accumulator_node_id = accumulator_node.hash();
        if node_key == accumulator_node_id {
            Ok(accumulator_node)
        } else {
            Err(format_err!(
                "Accumulator node hash {:?} and node key {:?} mismatch.",
                accumulator_node_id,
                node_key
            ))
        }
    } else {
        Err(format_err!(
            "Accumulator node is none by node key {:?}.",
            node_key
        ))
    }
}

/// for common
pub fn get_headers_msg_for_common(block_id: HashValue) -> GetBlockHeaders {
    GetBlockHeaders::new(block_id, 1, false, HEAD_CT)
}

pub fn get_headers_msg_for_ancestor(
    block_number: BlockNumber,
    step: usize,
) -> GetBlockHeadersByNumber {
    //todo：binary search
    GetBlockHeadersByNumber::new(block_number, step, HEAD_CT)
}
