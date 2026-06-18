use std::collections::{HashMap, HashSet};

use anyhow::{Result, anyhow};
use base64::Engine;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::BlockchainError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct OutPoint {
    pub txid: String,
    pub index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxOut {
    pub value: u64,
    pub address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxIn {
    pub prev_out: OutPoint,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub inputs: Vec<TxIn>,
    pub outputs: Vec<TxOut>,
    pub timestamp: i64,
}

impl Transaction {
    pub fn id(&self) -> String {
        let data = bincode::serialize(self).unwrap_or_default();
        let mut hasher = Sha256::new();
        hasher.update(data);
        hex::encode(hasher.finalize())
    }

    pub fn is_coinbase(&self) -> bool {
        self.inputs.is_empty()
    }

    pub fn verify(&self, utxo_set: &HashMap<OutPoint, TxOut>) -> Result<()> {
        if self.outputs.is_empty() {
            return Err(anyhow!(BlockchainError::InvalidTransaction));
        }

        let txid = self.id();
        let mut seen_inputs = HashSet::new();
        let mut input_value = 0u64;

        for output in &self.outputs {
            if output.address.is_empty() {
                return Err(anyhow!(BlockchainError::InvalidTransaction));
            }
        }

        if self.is_coinbase() {
            return Ok(());
        }

        for input in &self.inputs {
            if !seen_inputs.insert(input.prev_out.clone()) {
                return Err(anyhow!(BlockchainError::DuplicateInput));
            }

            let prev_txout = utxo_set
                .get(&input.prev_out)
                .ok_or_else(|| anyhow!(BlockchainError::InputNotFound))?;

            input_value = input_value
                .checked_add(prev_txout.value)
                .ok_or_else(|| anyhow!(BlockchainError::InvalidTransaction))?;

            let sender_bytes = base64::engine::general_purpose::STANDARD
                .decode(&prev_txout.address)
                .map_err(|_| anyhow!(BlockchainError::InvalidPublicKey))?;
            let sender_pk_bytes: [u8; 32] = sender_bytes
                .try_into()
                .map_err(|_| anyhow!(BlockchainError::InvalidPublicKey))?;
            let sender_pk = VerifyingKey::from_bytes(&sender_pk_bytes)
                .map_err(|_| anyhow!(BlockchainError::InvalidPublicKey))?;

            let sig_bytes = hex::decode(&input.signature)
                .map_err(|_| anyhow!(BlockchainError::InvalidSignatureFormat))?;
            let sig_array: [u8; 64] = sig_bytes
                .try_into()
                .map_err(|_| anyhow!(BlockchainError::InvalidSignatureFormat))?;
            let signature = Signature::from_bytes(&sig_array);

            sender_pk
                .verify(txid.as_bytes(), &signature)
                .map_err(|_| anyhow!(BlockchainError::InvalidSignature))?;
        }

        let output_value = self.outputs.iter().try_fold(0u64, |sum, output| {
            sum.checked_add(output.value)
                .ok_or_else(|| anyhow!(BlockchainError::InvalidTransaction))
        })?;

        if input_value < output_value {
            return Err(anyhow!(BlockchainError::InsufficientBalance));
        }

        Ok(())
    }
}
