use std::collections::HashMap;

use anyhow::{Result, anyhow};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::BlockchainError;
use crate::mempool::Mempool;
use crate::merkle::calculate_merkle_root;
use crate::transaction::{OutPoint, Transaction, TxOut};

const BLOCKS_TREE: &str = "blocks";
const UTXO_TREE: &str = "utxo_set";
const DEFAULT_DIFFICULTY_PREFIX: &str = "0000";
const BLOCK_REWARD: u64 = 50;

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
    pub fn new(
        index: u64,
        transactions: Vec<Transaction>,
        previous_hash: String,
        difficulty_prefix: &str,
    ) -> Result<Self> {
        let timestamp = Utc::now().timestamp();
        let merkle_root = Self::merkle_root_for_transactions(&transactions)?;
        let mut block = Block {
            index,
            timestamp,
            transactions,
            previous_hash,
            merkle_root,
            nonce: 0,
            hash: String::new(),
        };
        block.mine_block(difficulty_prefix);
        Ok(block)
    }

    pub fn merkle_root_for_transactions(transactions: &[Transaction]) -> Result<String> {
        let mut hashes = Vec::with_capacity(transactions.len());
        for tx in transactions {
            let bytes = hex::decode(tx.id())
                .map_err(|e| anyhow!(BlockchainError::SerializationError(e.to_string())))?;
            let digest: [u8; 32] = bytes
                .try_into()
                .map_err(|_| anyhow!(BlockchainError::SerializationError("invalid txid length".into())))?;
            hashes.push(digest);
        }
        Ok(calculate_merkle_root(hashes))
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

    pub fn mine_block(&mut self, difficulty_prefix: &str) {
        loop {
            let hash = self.calculate_hash();
            if hash.starts_with(difficulty_prefix) {
                self.hash = hash;
                break;
            }
            self.nonce += 1;
        }
    }

    pub fn validate_pow(&self, difficulty_prefix: &str) -> Result<()> {
        let expected_hash = self.calculate_hash();
        if self.hash != expected_hash {
            return Err(anyhow!(BlockchainError::InvalidBlockHash));
        }
        if !self.hash.starts_with(difficulty_prefix) {
            return Err(anyhow!(BlockchainError::PoWNotMet));
        }
        Ok(())
    }

    pub fn validate_merkle_root(&self) -> Result<()> {
        let expected = Self::merkle_root_for_transactions(&self.transactions)?;
        if expected != self.merkle_root {
            return Err(anyhow!(BlockchainError::InvalidMerkleRoot));
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct Blockchain {
    pub chain: Vec<Block>,
    pub utxo_set: HashMap<OutPoint, TxOut>,
    pub mempool: Mempool,
    pub db: sled::Db,
    pub difficulty_prefix: String,
}

impl Blockchain {
    pub fn open(db_path: &str) -> Result<Self> {
        let db = sled::open(db_path)
            .map_err(|e| anyhow!(BlockchainError::StorageError(e.to_string())))?;
        let blocks_tree = db
            .open_tree(BLOCKS_TREE)
            .map_err(|e| anyhow!(BlockchainError::StorageError(e.to_string())))?;

        let mut chain = Vec::new();
        for item in blocks_tree.iter() {
            let (_, value) = item.map_err(|e| anyhow!(BlockchainError::StorageError(e.to_string())))?;
            let block: Block = bincode::deserialize(&value)
                .map_err(|e| anyhow!(BlockchainError::SerializationError(e.to_string())))?;
            chain.push(block);
        }

        let difficulty_prefix = DEFAULT_DIFFICULTY_PREFIX.to_string();

        if chain.is_empty() {
            let genesis = Block::new(0, Vec::new(), "0".repeat(64), &difficulty_prefix)?;
            let bytes = bincode::serialize(&genesis)
                .map_err(|e| anyhow!(BlockchainError::SerializationError(e.to_string())))?;
            blocks_tree
                .insert(genesis.index.to_be_bytes(), bytes)
                .map_err(|e| anyhow!(BlockchainError::StorageError(e.to_string())))?;
            chain.push(genesis);
            db.flush()
                .map_err(|e| anyhow!(BlockchainError::StorageError(e.to_string())))?;
        }

        let utxo_set = Self::rebuild_utxo_set(&chain)?;
        let blockchain = Blockchain {
            chain,
            utxo_set,
            mempool: Mempool::new(),
            db,
            difficulty_prefix,
        };
        blockchain.persist_utxo_set()?;
        Ok(blockchain)
    }

    pub fn difficulty_prefix(&self) -> &str {
        &self.difficulty_prefix
    }

    pub fn tip_hash(&self) -> Result<String> {
        self.chain
            .last()
            .map(|block| block.hash.clone())
            .ok_or_else(|| anyhow!(BlockchainError::MissingData("chain tip")))
    }

    pub fn add_to_mempool(&mut self, tx: Transaction) -> Result<String> {
        self.validate_transaction_against_state(&tx, &self.utxo_set)?;
        let txid = tx.id();
        if self.mempool.contains(&txid) {
            return Ok(txid);
        }
        Ok(self.mempool.add_transaction(tx))
    }

    pub fn add_block(&mut self, block: Block) -> Result<()> {
        let (mut candidate_utxo, tx_ids) = self.validate_candidate_block(&block)?;
        self.persist_block(&block)?;
        self.chain.push(block);
        self.utxo_set = std::mem::take(&mut candidate_utxo);
        self.persist_utxo_set()?;
        self.mempool.remove_transactions(&tx_ids);
        Ok(())
    }

    pub fn replace_chain(&mut self, new_chain: Vec<Block>) -> Result<()> {
        if new_chain.len() <= self.chain.len() {
            return Err(anyhow!(BlockchainError::ChainReplacementRejected));
        }

        let new_utxo = self.validate_chain(&new_chain)?;
        self.overwrite_chain(&new_chain, &new_utxo)?;
        self.chain = new_chain;
        self.utxo_set = new_utxo;
        self.retain_valid_mempool()?;
        Ok(())
    }

    pub fn mine_block_from_mempool(&mut self, miner_address: String) -> Result<Block> {
        let mut transactions = Vec::new();
        transactions.push(Transaction {
            inputs: Vec::new(),
            outputs: vec![TxOut {
                value: BLOCK_REWARD,
                address: miner_address,
            }],
            timestamp: Utc::now().timestamp(),
        });

        let mut staged_utxo = self.utxo_set.clone();
        for tx in self.mempool.get_transactions_for_block(100) {
            if self.validate_transaction_against_state(&tx, &staged_utxo).is_ok() {
                Self::apply_transactions_to_utxo(&mut staged_utxo, std::slice::from_ref(&tx));
                transactions.push(tx);
            }
        }

        let previous_hash = self.tip_hash()?;
        let block = Block::new(
            self.chain.len() as u64,
            transactions,
            previous_hash,
            &self.difficulty_prefix,
        )?;
        self.add_block(block.clone())?;
        Ok(block)
    }

    pub fn get_balance(&self, address: &str) -> u64 {
        self.utxo_set
            .values()
            .filter(|out| out.address == address)
            .map(|out| out.value)
            .sum()
    }

    pub fn select_utxos_for_amount(
        &self,
        address: &str,
        amount: u64,
    ) -> Option<Vec<(OutPoint, TxOut)>> {
        let mut selected = self
            .utxo_set
            .iter()
            .filter(|(_, output)| output.address == address)
            .map(|(outpoint, output)| (outpoint.clone(), output.clone()))
            .collect::<Vec<_>>();

        selected.sort_by(|a, b| {
            a.0.txid
                .cmp(&b.0.txid)
                .then_with(|| a.0.index.cmp(&b.0.index))
        });

        let mut total = 0u64;
        let mut chosen = Vec::new();
        for (outpoint, output) in selected {
            total = total.checked_add(output.value)?;
            chosen.push((outpoint, output));
            if total >= amount {
                return Some(chosen);
            }
        }

        None
    }

    pub fn validate_chain(&self, chain: &[Block]) -> Result<HashMap<OutPoint, TxOut>> {
        if chain.is_empty() {
            return Err(anyhow!(BlockchainError::MissingData("chain")));
        }

        if chain[0].previous_hash != "0".repeat(64) {
            return Err(anyhow!(BlockchainError::GenesisMismatch));
        }

        let mut utxo = HashMap::new();
        let mut expected_index = 0u64;
        let mut previous_hash = String::new();

        for (position, block) in chain.iter().enumerate() {
            if block.index != expected_index {
                return Err(anyhow!(BlockchainError::InvalidBlockHash));
            }
            if position > 0 && block.previous_hash != previous_hash {
                return Err(anyhow!(BlockchainError::InvalidPreviousHash));
            }
            block.validate_pow(&self.difficulty_prefix)?;
            block.validate_merkle_root()?;
            Self::validate_transactions_for_block(&block.transactions, &mut utxo)?;
            previous_hash = block.hash.clone();
            expected_index += 1;
        }

        Ok(utxo)
    }

    fn validate_candidate_block(
        &self,
        block: &Block,
    ) -> Result<(HashMap<OutPoint, TxOut>, Vec<String>)> {
        let expected_previous_hash = self.tip_hash()?;
        if block.previous_hash != expected_previous_hash {
            return Err(anyhow!(BlockchainError::InvalidPreviousHash));
        }
        if block.index != self.chain.len() as u64 {
            return Err(anyhow!(BlockchainError::InvalidBlockHash));
        }
        block.validate_pow(&self.difficulty_prefix)?;
        block.validate_merkle_root()?;

        let mut candidate_utxo = self.utxo_set.clone();
        Self::validate_transactions_for_block(&block.transactions, &mut candidate_utxo)?;
        let tx_ids = block.transactions.iter().map(Transaction::id).collect();
        Ok((candidate_utxo, tx_ids))
    }

    fn validate_transactions_for_block(
        transactions: &[Transaction],
        utxo_set: &mut HashMap<OutPoint, TxOut>,
    ) -> Result<()> {
        let mut coinbase_seen = false;
        for tx in transactions {
            if tx.is_coinbase() {
                if coinbase_seen {
                    return Err(anyhow!(BlockchainError::InvalidTransaction));
                }
                coinbase_seen = true;
            } else {
                tx.verify(utxo_set)?;
            }
            Self::apply_transactions_to_utxo(utxo_set, std::slice::from_ref(tx));
        }
        Ok(())
    }

    fn validate_transaction_against_state(
        &self,
        tx: &Transaction,
        utxo_state: &HashMap<OutPoint, TxOut>,
    ) -> Result<()> {
        tx.verify(utxo_state)?;
        Ok(())
    }

    fn apply_transactions_to_utxo(
        utxo_set: &mut HashMap<OutPoint, TxOut>,
        transactions: &[Transaction],
    ) {
        for tx in transactions {
            let txid = tx.id();
            for input in &tx.inputs {
                utxo_set.remove(&input.prev_out);
            }
            for (index, output) in tx.outputs.iter().enumerate() {
                utxo_set.insert(
                    OutPoint {
                        txid: txid.clone(),
                        index: index as u32,
                    },
                    output.clone(),
                );
            }
        }
    }

    fn rebuild_utxo_set(chain: &[Block]) -> Result<HashMap<OutPoint, TxOut>> {
        let mut utxo = HashMap::new();
        for block in chain {
            Self::validate_transactions_for_block(&block.transactions, &mut utxo)?;
        }
        Ok(utxo)
    }

    fn persist_block(&self, block: &Block) -> Result<()> {
        let tree = self
            .db
            .open_tree(BLOCKS_TREE)
            .map_err(|e| anyhow!(BlockchainError::StorageError(e.to_string())))?;
        let bytes = bincode::serialize(block)
            .map_err(|e| anyhow!(BlockchainError::SerializationError(e.to_string())))?;
        tree.insert(block.index.to_be_bytes(), bytes)
            .map_err(|e| anyhow!(BlockchainError::StorageError(e.to_string())))?;
        self.db
            .flush()
            .map_err(|e| anyhow!(BlockchainError::StorageError(e.to_string())))?;
        Ok(())
    }

    fn persist_utxo_set(&self) -> Result<()> {
        let tree = self
            .db
            .open_tree(UTXO_TREE)
            .map_err(|e| anyhow!(BlockchainError::StorageError(e.to_string())))?;
        tree.clear()
            .map_err(|e| anyhow!(BlockchainError::StorageError(e.to_string())))?;
        for (outpoint, txout) in &self.utxo_set {
            let key = bincode::serialize(outpoint)
                .map_err(|e| anyhow!(BlockchainError::SerializationError(e.to_string())))?;
            let value = bincode::serialize(txout)
                .map_err(|e| anyhow!(BlockchainError::SerializationError(e.to_string())))?;
            tree.insert(key, value)
                .map_err(|e| anyhow!(BlockchainError::StorageError(e.to_string())))?;
        }
        self.db
            .flush()
            .map_err(|e| anyhow!(BlockchainError::StorageError(e.to_string())))?;
        Ok(())
    }

    fn overwrite_chain(
        &self,
        new_chain: &[Block],
        new_utxo: &HashMap<OutPoint, TxOut>,
    ) -> Result<()> {
        let blocks_tree = self
            .db
            .open_tree(BLOCKS_TREE)
            .map_err(|e| anyhow!(BlockchainError::StorageError(e.to_string())))?;
        blocks_tree
            .clear()
            .map_err(|e| anyhow!(BlockchainError::StorageError(e.to_string())))?;
        for block in new_chain {
            let bytes = bincode::serialize(block)
                .map_err(|e| anyhow!(BlockchainError::SerializationError(e.to_string())))?;
            blocks_tree
                .insert(block.index.to_be_bytes(), bytes)
                .map_err(|e| anyhow!(BlockchainError::StorageError(e.to_string())))?;
        }

        let utxo_tree = self
            .db
            .open_tree(UTXO_TREE)
            .map_err(|e| anyhow!(BlockchainError::StorageError(e.to_string())))?;
        utxo_tree
            .clear()
            .map_err(|e| anyhow!(BlockchainError::StorageError(e.to_string())))?;
        for (outpoint, txout) in new_utxo {
            let key = bincode::serialize(outpoint)
                .map_err(|e| anyhow!(BlockchainError::SerializationError(e.to_string())))?;
            let value = bincode::serialize(txout)
                .map_err(|e| anyhow!(BlockchainError::SerializationError(e.to_string())))?;
            utxo_tree
                .insert(key, value)
                .map_err(|e| anyhow!(BlockchainError::StorageError(e.to_string())))?;
        }

        self.db
            .flush()
            .map_err(|e| anyhow!(BlockchainError::StorageError(e.to_string())))?;
        Ok(())
    }

    fn retain_valid_mempool(&mut self) -> Result<()> {
        let current = self.mempool.all();
        self.mempool = Mempool::new();
        for tx in current {
            if self.validate_transaction_against_state(&tx, &self.utxo_set).is_ok() {
                self.mempool.add_transaction(tx);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaction::{OutPoint, Transaction, TxIn, TxOut};

    fn build_test_blockchain(utxo_set: HashMap<OutPoint, TxOut>) -> Blockchain {
        let unique = format!(
            "toy_blockchain_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);
        let _ = std::fs::remove_dir_all(&path);
        let db = sled::open(&path).unwrap();
        Blockchain {
            chain: Vec::new(),
            utxo_set,
            mempool: Mempool::new(),
            db,
            difficulty_prefix: DEFAULT_DIFFICULTY_PREFIX.to_string(),
        }
    }

    #[test]
    fn merkle_root_for_empty_transactions_is_zero_hash() {
        let root = Block::merkle_root_for_transactions(&[]).unwrap();
        assert_eq!(root, hex::encode([0u8; 32]));
    }

    #[test]
    fn mempool_add_contains_and_remove_work() {
        let mut mempool = Mempool::new();
        let tx = Transaction {
            inputs: vec![],
            outputs: vec![TxOut {
                value: 10,
                address: "miner".to_string(),
            }],
            timestamp: 1,
        };
        let txid = mempool.add_transaction(tx.clone());
        assert!(mempool.contains(&txid));
        assert_eq!(mempool.all().len(), 1);
        mempool.remove_transactions(&[txid.clone()]);
        assert!(!mempool.contains(&txid));
        assert!(mempool.all().is_empty());
    }

    #[test]
    fn coinbase_transaction_is_valid_without_inputs() {
        let tx = Transaction {
            inputs: vec![],
            outputs: vec![TxOut {
                value: 50,
                address: "miner".to_string(),
            }],
            timestamp: 1,
        };

        assert!(tx.verify(&HashMap::new()).is_ok());
    }

    #[test]
    fn transaction_verify_rejects_malformed_signature_format() {
        let tx = Transaction {
            inputs: vec![TxIn {
                prev_out: OutPoint {
                    txid: "a".repeat(64),
                    index: 0,
                },
                signature: "not-hex".to_string(),
            }],
            outputs: vec![TxOut {
                value: 5,
                address: "miner".to_string(),
            }],
            timestamp: 1,
        };

        let mut utxo = HashMap::new();
        utxo.insert(
            tx.inputs[0].prev_out.clone(),
            TxOut {
                value: 5,
                address: "miner".to_string(),
            },
        );

        assert!(tx.verify(&utxo).is_err());
    }

    #[test]
    fn transaction_verify_rejects_unknown_input() {
        let tx = Transaction {
            inputs: vec![TxIn {
                prev_out: OutPoint {
                    txid: "b".repeat(64),
                    index: 0,
                },
                signature: "00".repeat(32),
            }],
            outputs: vec![TxOut {
                value: 5,
                address: "miner".to_string(),
            }],
            timestamp: 1,
        };

        assert!(tx.verify(&HashMap::new()).is_err());
    }

    #[test]
    fn select_utxos_for_amount_returns_none_when_insufficient_funds() {
        let mut utxo = HashMap::new();
        utxo.insert(
            OutPoint {
                txid: "tx1".to_string(),
                index: 0,
            },
            TxOut {
                value: 10,
                address: "alice".to_string(),
            },
        );

        let blockchain = build_test_blockchain(utxo);
        assert!(blockchain.select_utxos_for_amount("alice", 50).is_none());
    }

    #[test]
    fn select_utxos_for_amount_can_select_multiple_utxos() {
        let mut utxo = HashMap::new();
        utxo.insert(
            OutPoint {
                txid: "tx1".to_string(),
                index: 0,
            },
            TxOut {
                value: 20,
                address: "alice".to_string(),
            },
        );
        utxo.insert(
            OutPoint {
                txid: "tx2".to_string(),
                index: 0,
            },
            TxOut {
                value: 15,
                address: "alice".to_string(),
            },
        );

        let blockchain = build_test_blockchain(utxo);
        let selected = blockchain.select_utxos_for_amount("alice", 25).unwrap();
        assert_eq!(selected.len(), 2);
        assert_eq!(selected.iter().map(|(_, out)| out.value).sum::<u64>(), 35);
    }

    #[test]
    fn example_seed_file_is_present_and_readable() {
        let path = std::path::Path::new("blockchain_db/example_seed.json");
        assert!(path.exists());
        let contents = std::fs::read_to_string(path).unwrap();
        assert!(contents.contains("Example persisted blockchain state"));
        assert!(contents.contains("sample_utxos"));
    }

    #[test]
    fn empty_blockchain_open_creates_genesis_block() {
        let unique = format!(
            "toy_blockchain_genesis_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);
        let _ = std::fs::remove_dir_all(&path);

        let blockchain = Blockchain::open(path.to_str().unwrap()).unwrap();
        assert_eq!(blockchain.chain.len(), 1);
        assert_eq!(blockchain.chain[0].index, 0);
        assert_eq!(blockchain.chain[0].transactions.len(), 0);
    }
}
