use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use chrono::Utc;
use crate::transaction::{Transaction, OutPoint, TxOut};
use crate::merkle::calculate_merkle_root;
use crate::error::BlockchainError;
use anyhow::Result;
use std::collections::HashMap;
use crate::mempool::Mempool;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub index: u64,
    pub timestamp: i64,
    pub transactions: Vec<Transaction>,
    pub previous_hash: String,
    pub merkle_root: String,
    pub nonce: u64,
    pub hash: String,
}

impl Block {
    pub fn new(index: u64, transactions: Vec<Transaction>, previous_hash: String) -> Self {
        let timestamp = Utc::now().timestamp();
        let hashes: Vec<[u8; 32]> = transactions
            .iter()
            .map(|tx| {
                let mut h = [0u8; 32];
                let id = tx.id();
                let bytes = hex::decode(id).unwrap_or_default();
                h.copy_from_slice(&bytes);
                h
            })
            .collect();
        let merkle_root = calculate_merkle_root(hashes);
        let mut block = Block {
            index,
            timestamp,
            transactions,
            previous_hash,
            merkle_root,
            nonce: 0,
            hash: String::new(),
        };
        block.mine_block();
        block
    }

    pub fn calculate_hash(&self) -> String {
        let data = format!(
            "{}{}{}{}{}",
            self.index, self.timestamp, self.previous_hash, self.merkle_root, self.nonce
        );
        let mut hasher = Sha256::new();
        hasher.update(data.as_bytes());
        hex::encode(hasher.finalize())
    }

    pub fn mine_block(&mut self) {
        let target = "0000"; // Difficulty
        while !self.calculate_hash().starts_with(target) {
            self.nonce += 1;
        }
        self.hash = self.calculate_hash();
    }
}

pub struct Blockchain {
    pub chain: Vec<Block>,
    pub utxo_set: HashMap<OutPoint, TxOut>,
    pub mempool: Mempool,
    pub db: sled::Db,
}

impl Blockchain {
    pub fn open(db_path: &str) -> Result<Self> {
        let db = sled::open(db_path)?;
        let mut chain = Vec::new();
        let utxo_tree = db.open_tree("utxo_set")?;
        let mut utxo_set = HashMap::new();

        // Load UTXO set from DB
        for item in utxo_tree.iter() {
            let (key, value) = item?;
            let outpoint: OutPoint = bincode::deserialize(&key)?;
            let txout: TxOut = bincode::deserialize(&value)?;
            utxo_set.insert(outpoint, txout);
        }


        let mut i = 0;
        while let Some(bytes) = db.get(format!("block_{}", i))? {
            let block: Block = bincode::deserialize(&bytes)?;
            Self::update_utxo_set(&mut utxo_set, &block.transactions);
            chain.push(block);
            i += 1;
        }

        if chain.is_empty() {
            let genesis = Block::new(0, vec![], "0".repeat(64));
            let bytes = bincode::serialize(&genesis)?;
            db.insert("block_0", bytes)?;
            Self::update_utxo_set(&mut utxo_set, &genesis.transactions);
            chain.push(genesis);
            // Save initial UTXO set
            for (outpoint, txout) in &utxo_set {
                utxo_tree.insert(bincode::serialize(outpoint)?, bincode::serialize(txout)?)?;
            }
        }

        Ok(Blockchain {
            chain,
            utxo_set,
            mempool: Mempool::new(),
            db,
        })
    }

    fn update_utxo_set(utxo_set: &mut HashMap<OutPoint, TxOut>, transactions: &[Transaction]) {
        for tx in transactions {
            let txid = tx.id();
            for input in &tx.inputs {
                utxo_set.remove(&input.prev_out);
            }
            for (i, output) in tx.outputs.iter().enumerate() {
                utxo_set.insert(
                    OutPoint {
                        txid: txid.clone(),
                        index: i as u32,
                    },
                    output.clone(),
                );
            }
        }
    }

    pub fn add_to_mempool(&mut self, tx: Transaction) -> Result<()> {
        // Verify transaction before adding to mempool
        tx.verify(&self.utxo_set)?;
        self.mempool.add_transaction(tx);
        Ok(())
    }

    pub fn replace_chain(&mut self, new_chain: Vec<Block>) -> Result<()> {
        // Simple longest chain rule: replace if new chain is longer and valid
        if new_chain.len() > self.chain.len() {
            // Re-initialize UTXO set and apply blocks from new chain
            self.utxo_set.clear();
            let utxo_tree = self.db.open_tree("utxo_set")?;
            utxo_tree.clear()?;

            // Clear existing blocks in DB
            for i in 0..self.chain.len() {
                self.db.remove(format!("block_{}", i))?;
            }

            self.chain.clear();
            for block in new_chain {
                self._apply_block(block)?;
            }
            Ok(())
        } else {
            Err(BlockchainError::InvalidPreviousHash.into()) // Placeholder error, refine later
        }
    }

    pub fn add_block(&mut self, block: Block) -> Result<()> {
        if block.previous_hash != self.chain.last().unwrap().hash {
            return Err(BlockchainError::InvalidPreviousHash.into());
        }
        if !block.hash.starts_with("0000") {
            return Err(BlockchainError::PoWNotMet.into());
        }
        self._apply_block(block)
    }

    // Internal function to apply a block to the chain and update UTXO set
    pub fn _apply_block(&mut self, block: Block) -> Result<()> {


        Self::update_utxo_set(&mut self.utxo_set, &block.transactions);

        // Update UTXO set in DB
        let utxo_tree = self.db.open_tree("utxo_set")?;
        utxo_tree.clear()?;
        for (outpoint, txout) in &self.utxo_set {
            utxo_tree.insert(bincode::serialize(outpoint)?, bincode::serialize(txout)?)?;
        }

        let bytes = bincode::serialize(&block)?;
        self.db.insert(format!("block_{}", block.index), bytes)?;
        self.chain.push(block);
        Ok(())
    }

    pub fn get_balance(&self, address: &str) -> u64 {
        self.utxo_set
            .values()
            .filter(|out| out.address == address)
            .map(|out| out.value)
            .sum()
    }

    pub fn mine_block_from_mempool(&mut self, miner_address: String) -> Result<Block> {
        let transactions: Vec<Transaction> = self.mempool.get_transactions_for_block(10).into_iter().collect(); // Take up to 10 transactions
        let previous_hash = self.chain.last().unwrap().hash.clone();
        let new_block = Block::new(self.chain.len() as u64, transactions.clone(), previous_hash);
        self.add_block(new_block.clone())?;

        let tx_ids: Vec<String> = transactions.iter().map(|tx| tx.id()).collect();
        self.mempool.remove_transactions(&tx_ids);

        Ok(new_block)
    }
}
