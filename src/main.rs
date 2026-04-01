use chrono::Utc;
use ed25519_dalek::{Keypair, PublicKey, Signature, Signer, Verifier};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt::Write;

// ---------- Transaction ----------
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub sender: String,      // base64 encoded public key
    pub receiver: String,    // base64 encoded public key
    pub amount: u64,
    pub signature: String,   // hex encoded signature
}

impl Transaction {
    pub fn new(sender: PublicKey, receiver: PublicKey, amount: u64, keypair: &Keypair) -> Self {
        let tx_data = format!("{}{}{}", sender, receiver, amount);
        let signature = keypair.sign(tx_data.as_bytes());
        Transaction {
            sender: base64_encode(sender.as_bytes()),
            receiver: base64_encode(receiver.as_bytes()),
            amount,
            signature: hex::encode(signature.to_bytes()),
        }
    }

    pub fn verify(&self) -> bool {
        // Decode sender public key
        let sender_bytes = match base64_decode(&self.sender) {
            Ok(b) => b,
            Err(_) => return false,
        };
        let sender_pk = match PublicKey::from_bytes(&sender_bytes) {
            Ok(pk) => pk,
            Err(_) => return false,
        };
        let tx_data = format!("{}{}{}", self.sender, self.receiver, self.amount);
        let sig_bytes = match hex::decode(&self.signature) {
            Ok(s) => s,
            Err(_) => return false,
        };
        let signature = match Signature::from_bytes(&sig_bytes) {
            Ok(s) => s,
            Err(_) => return false,
        };
        sender_pk.verify(tx_data.as_bytes(), &signature).is_ok()
    }
}

// ---------- Block ----------
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub index: u64,
    pub timestamp: i64,
    pub transactions: Vec<Transaction>,
    pub previous_hash: String,
    pub nonce: u64,
    pub hash: String,
}

impl Block {
    pub fn new(index: u64, transactions: Vec<Transaction>, previous_hash: String) -> Self {
        let timestamp = Utc::now().timestamp();
        let mut block = Block {
            index,
            timestamp,
            transactions,
            previous_hash,
            nonce: 0,
            hash: String::new(),
        };
        block.mine_block();
        block
    }

    pub fn calculate_hash(&self) -> String {
        let data = format!(
            "{}{}{:?}{}{}",
            self.index, self.timestamp, self.transactions, self.previous_hash, self.nonce
        );
        let mut hasher = Sha256::new();
        hasher.update(data.as_bytes());
        let result = hasher.finalize();
        hex::encode(result)
    }

    pub fn mine_block(&mut self) {
        let target = "0000"; // 4 leading zeros difficulty
        while !self.calculate_hash().starts_with(target) {
            self.nonce += 1;
        }
        self.hash = self.calculate_hash();
        println!("Block mined: {}", self.hash);
    }
}

// ---------- Blockchain ----------
pub struct Blockchain {
    pub chain: Vec<Block>,
    pub pending_transactions: Vec<Transaction>,
    pub mining_reward: u64,
}

impl Blockchain {
    pub fn new() -> Self {
        let genesis_block = Block::new(0, vec![], String::new());
        Blockchain {
            chain: vec![genesis_block],
            pending_transactions: vec![],
            mining_reward: 100,
        }
    }

    pub fn add_transaction(&mut self, tx: Transaction) {
        if tx.verify() {
            self.pending_transactions.push(tx);
            println!("Transaction added.");
        } else {
            println!("Invalid transaction – signature verification failed.");
        }
    }

    pub fn mine_pending_transactions(&mut self, miner_address: &str) {
        // Add mining reward transaction
        let reward_tx = Transaction {
            sender: "system".to_string(),
            receiver: miner_address.to_string(),
            amount: self.mining_reward,
            signature: "".to_string(),
        };
        self.pending_transactions.push(reward_tx);

        let new_block = Block::new(
            self.chain.len() as u64,
            self.pending_transactions.clone(),
            self.chain.last().unwrap().hash.clone(),
        );
        self.chain.push(new_block);
        self.pending_transactions.clear();
    }

    pub fn is_chain_valid(&self) -> bool {
        for i in 1..self.chain.len() {
            let current = &self.chain[i];
            let previous = &self.chain[i - 1];

            if current.hash != current.calculate_hash() {
                return false;
            }
            if current.previous_hash != previous.hash {
                return false;
            }
            // Verify all transactions in block
            for tx in &current.transactions {
                if !tx.verify() {
                    return false;
                }
            }
        }
        true
    }

    pub fn get_balance(&self, address: &str) -> u64 {
        let mut balance = 0;
        for block in &self.chain {
            for tx in &block.transactions {
                if tx.sender == address {
                    balance -= tx.amount;
                }
                if tx.receiver == address {
                    balance += tx.amount;
                }
            }
        }
        balance
    }
}

// ---------- Helper functions ----------
fn base64_encode(data: &[u8]) -> String {
    base64::encode(data)
}

fn base64_decode(data: &str) -> Result<Vec<u8>, base64::DecodeError> {
    base64::decode(data)
}

// ---------- Main (CLI) ----------
fn main() {
    let mut blockchain = Blockchain::new();
    let mut rng = OsRng;

    // Create some keypairs for demonstration
    let alice = Keypair::generate(&mut rng);
    let bob = Keypair::generate(&mut rng);

    // Create and add a transaction
    let tx = Transaction::new(
        alice.public,
        bob.public,
        50,
        &alice,
    );
    blockchain.add_transaction(tx);

    // Mine a block
    let miner_pubkey = base64_encode(alice.public.as_bytes());
    blockchain.mine_pending_transactions(&miner_pubkey);

    // Show blockchain
    println!("{:#?}", blockchain.chain);

    // Check balances
    println!("Alice balance: {}", blockchain.get_balance(&base64_encode(alice.public.as_bytes())));
    println!("Bob balance: {}", blockchain.get_balance(&base64_encode(bob.public.as_bytes())));
}