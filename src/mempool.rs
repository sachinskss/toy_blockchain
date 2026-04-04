use std::collections::HashMap;
use crate::transaction::Transaction;

pub struct Mempool {
    pub transactions: HashMap<String, Transaction>,
}

impl Mempool {
    pub fn new() -> Self {
        Mempool {
            transactions: HashMap::new(),
        }
    }

    pub fn add_transaction(&mut self, tx: Transaction) {
        self.transactions.insert(tx.id(), tx);
    }

    pub fn get_transactions_for_block(&self, count: usize) -> Vec<Transaction> {
        self.transactions.values().take(count).cloned().collect()
    }

    pub fn remove_transactions(&mut self, tx_ids: &[String]) {
        for tx_id in tx_ids {
            self.transactions.remove(tx_id);
        }
    }

    pub fn len(&self) -> usize {
        self.transactions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.transactions.is_empty()
    }
}
