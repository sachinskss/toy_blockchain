use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tokio::time::timeout;

use crate::blockchain::{Block, Blockchain};
use crate::transaction::Transaction;

#[derive(Debug)]
pub struct NodeState {
    pub blockchain: RwLock<Blockchain>,
    pub peers: RwLock<HashSet<String>>,
}

impl NodeState {
    pub fn new(blockchain: Blockchain) -> Self {
        Self {
            blockchain: RwLock::new(blockchain),
            peers: RwLock::new(HashSet::new()),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Message {
    NewTransaction(Transaction),
    NewBlock(Block),
    GetChain,
    Chain(Vec<Block>),
}

const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const MESSAGE_TIMEOUT: Duration = Duration::from_secs(5);

fn normalize_peer(peer: &str) -> String {
    peer.trim().trim_end_matches('/').to_string()
}

fn should_replace_chain(local_len: usize, remote_len: usize) -> bool {
    remote_len > local_len
}

pub async fn run_p2p_server(bind_addr: String, state: Arc<NodeState>) -> Result<()> {
    let listener = TcpListener::bind(&bind_addr).await?;
    println!("P2P server listening on {}", bind_addr);

    loop {
        let (stream, peer_addr) = listener.accept().await?;
        let peer = peer_addr.to_string();
        state.peers.write().await.insert(peer);
        let state = state.clone();
        tokio::spawn(async move {
            if let Err(error) = handle_p2p(stream, state).await {
                eprintln!("P2P handler error: {error}");
            }
        });
    }
}

pub async fn handle_p2p(mut stream: TcpStream, state: Arc<NodeState>) -> Result<()> {
    let msg = read_message(&mut stream).await?;
    match msg {
        Message::NewTransaction(tx) => {
            let mut blockchain = state.blockchain.write().await;
            if let Err(error) = blockchain.add_to_mempool(tx) {
                eprintln!("Error adding transaction to mempool: {error}");
            }
        }
        Message::NewBlock(block) => {
            let mut blockchain = state.blockchain.write().await;
            if let Err(error) = blockchain.add_block(block) {
                eprintln!("Error adding block: {error}");
            }
        }
        Message::GetChain => {
            let response = {
                let blockchain = state.blockchain.read().await;
                Message::Chain(blockchain.chain.clone())
            };
            write_message(&mut stream, &response).await?;
        }
        Message::Chain(new_chain) => {
            let mut blockchain = state.blockchain.write().await;
            if let Err(error) = blockchain.replace_chain(new_chain) {
                eprintln!("Error replacing chain: {error}");
            }
        }
    }
    Ok(())
}

pub async fn broadcast(state: &Arc<NodeState>, message: &Message) {
    let peers = state.peers.read().await.iter().cloned().collect::<Vec<_>>();
    for peer in peers {
        let peer = normalize_peer(&peer);
        if let Err(error) = send_message(&peer, message).await {
            eprintln!("Broadcast to {peer} failed: {error}");
            state.peers.write().await.remove(&peer);
        }
    }
}

pub async fn sync_with_peer(state: &Arc<NodeState>, peer: &str) -> Result<()> {
    let peer = normalize_peer(peer);
    let response = request_chain(&peer).await?;
    let local_len = state.blockchain.read().await.chain.len();
    if should_replace_chain(local_len, response.len()) {
        let mut blockchain = state.blockchain.write().await;
        if let Err(error) = blockchain.replace_chain(response) {
            return Err(error);
        }
    }
    Ok(())
}

async fn request_chain(peer: &str) -> Result<Vec<Block>> {
    let mut stream = timeout(CONNECT_TIMEOUT, TcpStream::connect(peer)).await??;
    write_message(&mut stream, &Message::GetChain).await?;
    match read_message(&mut stream).await? {
        Message::Chain(chain) => Ok(chain),
        _ => Ok(Vec::new()),
    }
}

pub async fn send_message(peer: &str, message: &Message) -> Result<()> {
    let mut stream = timeout(CONNECT_TIMEOUT, TcpStream::connect(peer)).await??;
    write_message(&mut stream, message).await
}

async fn write_message(stream: &mut TcpStream, message: &Message) -> Result<()> {
    let payload = bincode::serialize(message)?;
    timeout(MESSAGE_TIMEOUT, stream.write_all(&payload.len().to_le_bytes())).await??;
    timeout(MESSAGE_TIMEOUT, stream.write_all(&payload)).await??;
    Ok(())
}

async fn read_message(stream: &mut TcpStream) -> Result<Message> {
    let mut len_bytes = [0u8; 4];
    timeout(MESSAGE_TIMEOUT, stream.read_exact(&mut len_bytes)).await??;
    let len = u32::from_le_bytes(len_bytes) as usize;
    let mut buf = vec![0u8; len];
    timeout(MESSAGE_TIMEOUT, stream.read_exact(&mut buf)).await??;
    Ok(bincode::deserialize(&buf)?)
}
#[cfg(test)]
mod tests {
    use super::should_replace_chain;

    #[test]
    fn should_replace_chain_prefers_longer_remote_chain() {
        assert!(should_replace_chain(3, 5));
    }

    #[test]
    fn should_replace_chain_rejects_shorter_remote_chain() {
        assert!(!should_replace_chain(5, 3));
    }

    #[test]
    fn should_replace_chain_rejects_equal_length_chain() {
        assert!(!should_replace_chain(4, 4));
    }
}