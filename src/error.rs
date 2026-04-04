use thiserror::Error;

#[derive(Error, Debug)]
pub enum BlockchainError {
    #[error("Invalid transaction signature")]
    InvalidSignature,
    #[error("Insufficient balance")]
    InsufficientBalance,
    #[error("Invalid block hash")]
    InvalidBlockHash,
    #[error("Invalid previous hash")]
    InvalidPreviousHash,
    #[error("Proof of work not met")]
    PoWNotMet,
    #[error("Serialization error: {0}")]
    SerializationError(String),
    #[error("Storage error: {0}")]
    StorageError(String),
    #[error("Input UTXO not found")]
    InputNotFound,
    #[error("Invalid public key")]
    InvalidPublicKey,
    #[error("Invalid signature format")]
    InvalidSignatureFormat,
}
