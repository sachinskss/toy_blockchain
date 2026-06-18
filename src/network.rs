use std::collections::HashSet;
use std::sync::Arc;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;

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
        if let Err(error) = send_message(&peer, message).await {
            eprintln!("Broadcast to {peer} failed: {error}");
        }
    }
}

pub async fn sync_with_peer(state: &Arc<NodeState>, peer: &str) -> Result<()> {
    let response = request_chain(peer).await?;
    let mut blockchain = state.blockchain.write().await;
    let _ = blockchain.replace_chain(response);
    Ok(())
}

async fn request_chain(peer: &str) -> Result<Vec<Block>> {
    let mut stream = TcpStream::connect(peer).await?;
    write_message(&mut stream, &Message::GetChain).await?;
    match read_message(&mut stream).await? {
        Message::Chain(chain) => Ok(chain),
        _ => Ok(Vec::new()),
    }
}

pub async fn send_message(peer: &str, message: &Message) -> Result<()> {
    let mut stream = TcpStream::connect(peer).await?;
    write_message(&mut stream, message).await
}

async fn write_message(stream: &mut TcpStream, message: &Message) -> Result<()> {
    let payload = bincode::serialize(message)?;
    stream.write_u32(payload.len() as u32).await?;
    stream.write_all(&payload).await?;
    Ok(())
}

async fn read_message(stream: &mut TcpStream) -> Result<Message> {
    let len = stream.read_u32().await?;
    let mut buf = vec![0u8; len as usize];
    stream.read_exact(&mut buf).await?;
    Ok(bincode::deserialize(&buf)?)
}
