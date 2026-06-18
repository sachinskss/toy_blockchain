use std::collections::HashMap;

use crate::transaction::Transaction;

#[derive(Debug, Default)]
pub struct Mempool {
    pub transactions: HashMap<String, Transaction>,
}

impl Mempool {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_transaction(&mut self, tx: Transaction) -> String {
        let txid = tx.id();
        self.transactions.insert(txid.clone(), tx);
        txid
    }

    pub fn contains(&self, txid: &str) -> bool {
        self.transactions.contains_key(txid)
    }

    pub fn all(&self) -> Vec<Transaction> {
        self.transactions.values().cloned().collect()
    }

    pub fn get_transactions_for_block(&self, count: usize) -> Vec<Transaction> {
        self.transactions.values().take(count).cloned().collect()
    }

    pub fn remove_transactions(&mut self, tx_ids: &[String]) {
        for tx_id in tx_ids {
            self.transactions.remove(tx_id);
        }
    }
}
