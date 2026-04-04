use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ed25519_dalek::{Signature, VerifyingKey, Verifier};
use crate::error::BlockchainError;
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use base64::Engine;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct OutPoint {
    pub txid: String,
    pub index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxOut {
    pub value: u64,
    pub address: String, // base64 encoded public key
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxIn {
    pub prev_out: OutPoint,
    pub signature: String, // hex encoded signature
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

    pub fn verify(&self, utxo_set: &HashMap<OutPoint, TxOut>) -> Result<()> {
        let mut input_value = 0;
        let txid = self.id();

        for input in &self.inputs {
            let prev_txout = utxo_set
                .get(&input.prev_out)
                .ok_or_else(|| anyhow!(BlockchainError::InputNotFound))?;
            
            input_value += prev_txout.value;

            // Verify signature
            let sender_bytes = base64::engine::general_purpose::STANDARD.decode(&prev_txout.address)?;
            let sender_pk_bytes: [u8; 32] = sender_bytes.try_into().map_err(|_| anyhow!(BlockchainError::InvalidPublicKey))?;
            let sender_pk = VerifyingKey::from_bytes(&sender_pk_bytes)?;
            let sig_bytes = hex::decode(&input.signature)?;
            let sig_array: [u8; 64] = sig_bytes.try_into().map_err(|_| anyhow!(BlockchainError::InvalidSignatureFormat))?;
            let signature = Signature::from_bytes(&sig_array);
            
            sender_pk.verify(txid.as_bytes(), &signature)
                .map_err(|_| BlockchainError::InvalidSignature)?;
        }

        let output_value: u64 = self.outputs.iter().map(|o| o.value).sum();
        if input_value < output_value && !self.inputs.is_empty() {
            return Err(BlockchainError::InsufficientBalance.into());
        }

        Ok(())
    }
}
