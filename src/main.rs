mod blockchain;
mod error;
mod mempool;
mod merkle;
mod network;
mod transaction;

use std::env;
use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};

use crate::blockchain::{Block, Blockchain};
use crate::network::{Message, NodeState, broadcast, run_p2p_server, sync_with_peer};
use crate::transaction::Transaction;

#[derive(Debug, Deserialize)]
struct ConnectParams {
    peer: String,
}

#[derive(Debug, Serialize)]
struct StatusResponse {
    height: usize,
    tip_hash: String,
    difficulty_prefix: String,
    mempool_size: usize,
    peers: Vec<String>,
}

async fn get_blocks(State(state): State<Arc<NodeState>>) -> Json<Vec<Block>> {
    let blockchain = state.blockchain.read().await;
    Json(blockchain.chain.clone())
}

async fn get_balance(
    State(state): State<Arc<NodeState>>,
    Path(address): Path<String>,
) -> Json<u64> {
    let blockchain = state.blockchain.read().await;
    Json(blockchain.get_balance(&address))
}

async fn submit_tx(
    State(state): State<Arc<NodeState>>,
    Json(tx): Json<Transaction>,
) -> Result<Json<String>, String> {
    let txid = {
        let mut blockchain = state.blockchain.write().await;
        blockchain.add_to_mempool(tx).map_err(|e| e.to_string())?
    };

    let blockchain = state.blockchain.read().await;
    let maybe_tx = blockchain.mempool.transactions.get(&txid).cloned();
    drop(blockchain);

    if let Some(tx) = maybe_tx {
        broadcast(&state, &Message::NewTransaction(tx)).await;
    }

    Ok(Json(txid))
}

async fn mine_block_api(
    State(state): State<Arc<NodeState>>,
    Path(miner_address): Path<String>,
) -> Result<Json<Block>, String> {
    let block = {
        let mut blockchain = state.blockchain.write().await;
        blockchain
            .mine_block_from_mempool(miner_address)
            .map_err(|e| e.to_string())?
    };

    broadcast(&state, &Message::NewBlock(block.clone())).await;
    Ok(Json(block))
}

async fn get_mempool(State(state): State<Arc<NodeState>>) -> Json<Vec<Transaction>> {
    let blockchain = state.blockchain.read().await;
    Json(blockchain.mempool.all())
}

async fn connect_peer(
    State(state): State<Arc<NodeState>>,
    Query(params): Query<ConnectParams>,
) -> Result<Json<String>, String> {
    state.peers.write().await.insert(params.peer.clone());
    sync_with_peer(&state, &params.peer)
        .await
        .map_err(|e| e.to_string())?;
    Ok(Json(format!("connected to {}", params.peer)))
}

async fn get_status(State(state): State<Arc<NodeState>>) -> Result<Json<StatusResponse>, String> {
    let blockchain = state.blockchain.read().await;
    let peers = state.peers.read().await.iter().cloned().collect::<Vec<_>>();
    let tip_hash = blockchain.tip_hash().map_err(|e| e.to_string())?;
    Ok(Json(StatusResponse {
        height: blockchain.chain.len(),
        tip_hash,
        difficulty_prefix: blockchain.difficulty_prefix().to_string(),
        mempool_size: blockchain.mempool.transactions.len(),
        peers,
    }))
}

#[tokio::main]
async fn main() -> Result<()> {
    let rest_addr = env::var("REST_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".to_string());
    let p2p_addr = env::var("P2P_ADDR").unwrap_or_else(|_| "127.0.0.1:9000".to_string());
    let db_path = env::var("BLOCKCHAIN_DB").unwrap_or_else(|_| "blockchain_db".to_string());
    let seed_peers = env::var("PEERS")
        .unwrap_or_default()
        .split(',')
        .filter(|peer| !peer.trim().is_empty())
        .map(|peer| peer.trim().to_string())
        .collect::<Vec<_>>();

    let blockchain = Blockchain::open(&db_path)?;
    let state = Arc::new(NodeState::new(blockchain));

    {
        let mut peers = state.peers.write().await;
        for peer in &seed_peers {
            peers.insert(peer.clone());
        }
    }

    for peer in &seed_peers {
        let _ = sync_with_peer(&state, peer).await;
    }

    let p2p_state = state.clone();
    tokio::spawn(async move {
        if let Err(error) = run_p2p_server(p2p_addr, p2p_state).await {
            eprintln!("P2P server failed: {error}");
        }
    });

    let app = Router::new()
        .route("/blocks", get(get_blocks))
        .route("/balance/:address", get(get_balance))
        .route("/transactions", post(submit_tx))
        .route("/mine/:miner_address", post(mine_block_api))
        .route("/mempool", get(get_mempool))
        .route("/status", get(get_status))
        .route("/peers/connect", post(connect_peer))
        .with_state(state);

    let addr: SocketAddr = rest_addr.parse()?;
    println!("REST API listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
