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
    body::Body,
    extract::{Path, Query, State},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};

use crate::blockchain::{Block, Blockchain};
use crate::network::{Message, NodeState, broadcast, run_p2p_server, sync_with_peer};
use crate::transaction::{Transaction, TxOut};

const TEST_DIFFICULTY_PREFIX: &str = "0000";

#[derive(Debug, Deserialize)]
struct ConnectParams {
    peer: String,
}

#[derive(Debug, Deserialize)]
struct WalletPreviewRequest {
    from_address: String,
    to_address: String,
    amount: u64,
    fee: Option<u64>,
}

#[derive(Debug, Serialize)]
struct StatusResponse {
    height: usize,
    tip_hash: String,
    difficulty_prefix: String,
    mempool_size: usize,
    peers: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct UtxoSummary {
    txid: String,
    index: u32,
    value: u64,
    address: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct WalletPreviewResponse {
    valid: bool,
    amount: u64,
    fee: u64,
    total_selected: u64,
    change: u64,
    selected_utxos: Vec<UtxoSummary>,
    outputs: Vec<TxOut>,
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

async fn get_utxos(
    State(state): State<Arc<NodeState>>,
    Path(address): Path<String>,
) -> Json<Vec<UtxoSummary>> {
    let blockchain = state.blockchain.read().await;
    let utxos = blockchain
        .utxo_set
        .iter()
        .filter(|(_, output)| output.address == address)
        .map(|(outpoint, output)| UtxoSummary {
            txid: outpoint.txid.clone(),
            index: outpoint.index,
            value: output.value,
            address: output.address.clone(),
        })
        .collect::<Vec<_>>();
    Json(utxos)
}

async fn preview_wallet_tx(
    State(state): State<Arc<NodeState>>,
    Json(req): Json<WalletPreviewRequest>,
) -> Result<Json<WalletPreviewResponse>, String> {
    let blockchain = state.blockchain.read().await;
    let fee = req.fee.unwrap_or(0);
    let required = req.amount.saturating_add(fee);
    let selected = blockchain
        .select_utxos_for_amount(&req.from_address, required)
        .unwrap_or_default();

    let total_selected = selected.iter().map(|(_, output)| output.value).sum::<u64>();
    let change = total_selected.saturating_sub(required);
    let outputs = std::iter::once(TxOut {
        value: req.amount,
        address: req.to_address.clone(),
    })
    .chain((change > 0).then_some(TxOut {
        value: change,
        address: req.from_address.clone(),
    }))
    .collect::<Vec<_>>();

    let response = WalletPreviewResponse {
        valid: total_selected >= required,
        amount: req.amount,
        fee,
        total_selected,
        change,
        selected_utxos: selected
            .iter()
            .map(|(outpoint, output)| UtxoSummary {
                txid: outpoint.txid.clone(),
                index: outpoint.index,
                value: output.value,
                address: output.address.clone(),
            })
            .collect(),
        outputs,
    };

    Ok(Json(response))
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

fn build_app(state: Arc<NodeState>) -> Router {
    Router::new()
        .route("/blocks", get(get_blocks))
        .route("/balance/:address", get(get_balance))
        .route("/utxos/:address", get(get_utxos))
        .route("/wallet/preview", post(preview_wallet_tx))
        .route("/transactions", post(submit_tx))
        .route("/mine/:miner_address", post(mine_block_api))
        .route("/mempool", get(get_mempool))
        .route("/status", get(get_status))
        .route("/peers/connect", post(connect_peer))
        .with_state(state)
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

    let app = build_app(state);

    let addr: SocketAddr = rest_addr.parse()?;
    println!("REST API listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::to_bytes,
        http::{Request, StatusCode},
    };
    use std::collections::HashMap;
    use tower::ServiceExt;

    use crate::transaction::{OutPoint, TxOut};

    fn build_test_state_with_utxos(utxo_set: HashMap<OutPoint, TxOut>) -> Arc<NodeState> {
        let unique = format!(
            "toy_blockchain_api_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);
        let _ = std::fs::remove_dir_all(&path);
        let db = sled::open(&path).unwrap();
        let blockchain = Blockchain {
            chain: Vec::new(),
            utxo_set,
            mempool: crate::mempool::Mempool::new(),
            db,
            difficulty_prefix: TEST_DIFFICULTY_PREFIX.to_string(),
        };
        Arc::new(NodeState::new(blockchain))
    }

    #[tokio::test]
    async fn balance_endpoint_returns_sum_for_address() {
        let mut utxo_set = HashMap::new();
        utxo_set.insert(
            OutPoint {
                txid: "tx1".to_string(),
                index: 0,
            },
            TxOut {
                value: 40,
                address: "alice".to_string(),
            },
        );
        utxo_set.insert(
            OutPoint {
                txid: "tx2".to_string(),
                index: 1,
            },
            TxOut {
                value: 10,
                address: "alice".to_string(),
            },
        );
        let state = build_test_state_with_utxos(utxo_set);
        let app = build_app(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/balance/alice")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(body.as_ref(), b"50");
    }

    #[tokio::test]
    async fn wallet_preview_endpoint_returns_selected_utxos_and_change() {
        let mut utxo_set = HashMap::new();
        utxo_set.insert(
            OutPoint {
                txid: "tx1".to_string(),
                index: 0,
            },
            TxOut {
                value: 20,
                address: "alice".to_string(),
            },
        );
        utxo_set.insert(
            OutPoint {
                txid: "tx2".to_string(),
                index: 1,
            },
            TxOut {
                value: 15,
                address: "alice".to_string(),
            },
        );
        let state = build_test_state_with_utxos(utxo_set);
        let app = build_app(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/wallet/preview")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "from_address": "alice",
                            "to_address": "bob",
                            "amount": 25,
                            "fee": 5
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let preview: WalletPreviewResponse = serde_json::from_slice(&body).unwrap();
        assert!(preview.valid);
        assert_eq!(preview.amount, 25);
        assert_eq!(preview.fee, 5);
        assert_eq!(preview.total_selected, 35);
        assert_eq!(preview.change, 5);
        assert_eq!(preview.outputs.len(), 2);
        assert_eq!(preview.outputs[0].value, 25);
        assert_eq!(preview.outputs[1].value, 5);
    }
}
