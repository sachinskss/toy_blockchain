mod error;
mod merkle;
mod transaction;
mod blockchain;
mod mempool;
mod network;

use anyhow::Result;
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use serde::{Deserialize, Serialize};

use crate::blockchain::{Blockchain, Block};

use crate::transaction::Transaction;
use crate::network::handle_p2p;


// ---------- API Handlers ----------
async fn get_blocks(State(state): State<Arc<RwLock<Blockchain>>>) -> Json<Vec<Block>> {
    let bc = state.read().unwrap();
    Json(bc.chain.clone())
}

async fn get_balance(
    State(state): State<Arc<RwLock<Blockchain>>>,
    Path(address): Path<String>,
) -> Json<u64> {
    let bc = state.read().unwrap();
    Json(bc.get_balance(&address))
}

async fn submit_tx(
    State(state): State<Arc<RwLock<Blockchain>>>,
    Json(tx): Json<Transaction>,
) -> Result<Json<String>, String> {
    let mut bc = state.write().unwrap();
    bc.add_to_mempool(tx)
        .map(|_| Json("Transaction added to mempool".to_string()))
        .map_err(|e| e.to_string())
}

async fn mine_block_api(
    State(state): State<Arc<RwLock<Blockchain>>>,
    Path(miner_address): Path<String>,
) -> Result<Json<Block>, String> {
    let mut bc = state.write().unwrap();
    bc.mine_block_from_mempool(miner_address)
        .map(Json)
        .map_err(|e| e.to_string())
}

async fn get_mempool(State(state): State<Arc<RwLock<Blockchain>>>) -> Json<Vec<Transaction>> {
    let bc = state.read().unwrap();
    Json(bc.mempool.transactions.values().cloned().collect())
}

// ---------- Main ----------
#[tokio::main]
async fn main() -> Result<()> {
    let blockchain = Blockchain::open("blockchain_db")?;
    let shared_state = Arc::new(RwLock::new(blockchain));

    let p2p_state = shared_state.clone();
    tokio::spawn(async move {
        let listener = TcpListener::bind("127.0.0.1:9000").await.unwrap();
        println!("P2P Server listening on 127.0.0.1:9000");
        loop {
            if let Ok((stream, _)) = listener.accept().await {
                let state = p2p_state.clone();
                tokio::spawn(handle_p2p(stream, state));
            }
        }
    });

    let app = Router::new()
        .route("/blocks", get(get_blocks))
        .route("/balance/:address", get(get_balance))
        .route("/transactions", post(submit_tx))
        .route("/mine/:miner_address", post(mine_block_api))
        .route("/mempool", get(get_mempool))
        .with_state(shared_state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("REST API listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
