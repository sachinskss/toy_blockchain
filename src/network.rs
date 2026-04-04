use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use crate::blockchain::{Blockchain, Block};
use crate::transaction::Transaction;

#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    NewTransaction(Transaction),
    NewBlock(Block),
    GetChain,
    Chain(Vec<Block>),
}

pub async fn handle_p2p(mut stream: TcpStream, state: Arc<RwLock<Blockchain>>) {
    let mut buf = vec![0u8; 4096];
    if let Ok(n) = stream.read(&mut buf).await {
        if let Ok(msg) = bincode::deserialize::<Message>(&buf[..n]) {
            match msg {
                Message::NewTransaction(tx) => {
                    let mut bc = state.write().unwrap();
                    let _ = bc.add_to_mempool(tx).map_err(|e| println!("Error adding transaction to mempool: {}", e));
                }
                Message::NewBlock(block) => {
                    let mut bc = state.write().unwrap();
                    let _ = bc.add_block(block).map_err(|e| println!("Error adding block: {}", e));
                }
                Message::GetChain => {
                    let bytes = {
                        let bc = state.read().unwrap();
                        let response = Message::Chain(bc.chain.clone());
                        bincode::serialize(&response).unwrap()
                    };
                    let _ = stream.write_all(&bytes).await;
                }
                Message::Chain(new_chain) => {
                    let mut bc = state.write().unwrap();
                    let _ = bc.replace_chain(new_chain).map_err(|e| println!("Error replacing chain: {}", e));
                }
            }
        }
    }
}
