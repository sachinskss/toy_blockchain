use thiserror::Error;

#[derive(Error, Debug)]
pub enum BlockchainError {
    #[error("invalid transaction signature")]
    InvalidSignature,
    #[error("insufficient balance")]
    InsufficientBalance,
    #[error("invalid block hash")]
    InvalidBlockHash,
    #[error("invalid previous hash")]
    InvalidPreviousHash,
    #[error("proof of work not met")]
    PoWNotMet,
    #[error("input UTXO not found")]
    InputNotFound,
    #[error("invalid public key")]
    InvalidPublicKey,
    #[error("invalid signature format")]
    InvalidSignatureFormat,
    #[error("invalid merkle root")]
    InvalidMerkleRoot,
    #[error("block contains invalid transaction")]
    InvalidTransaction,
    #[error("duplicate transaction input detected")]
    DuplicateInput,
    #[error("chain replacement rejected")]
    ChainReplacementRejected,
    #[error("genesis block mismatch")]
    GenesisMismatch,
    #[error("database entry missing: {0}")]
    MissingData(&'static str),
    #[error("serialization error: {0}")]
    SerializationError(String),
    #[error("storage error: {0}")]
    StorageError(String),
}
