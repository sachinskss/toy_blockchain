use sha2::{Digest, Sha256};
use std::hash::Hasher;

use merkle_light::hash::Algorithm;
use merkle_light::merkle::MerkleTree;

#[derive(Clone, Default)]
pub struct Sha256Algorithm(Sha256);

impl Hasher for Sha256Algorithm {
    fn finish(&self) -> u64 {
        0
    }

    fn write(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }
}

impl Algorithm<[u8; 32]> for Sha256Algorithm {
    fn hash(&mut self) -> [u8; 32] {
        self.0.clone().finalize().into()
    }

    fn reset(&mut self) {
        self.0 = Sha256::new();
    }

    fn leaf(&mut self, data: [u8; 32]) -> [u8; 32] {
        data
    }

    fn node(&mut self, left: [u8; 32], right: [u8; 32]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(left);
        hasher.update(right);
        hasher.finalize().into()
    }
}

pub fn calculate_merkle_root(hashes: Vec<[u8; 32]>) -> String {
    if hashes.is_empty() {
        return hex::encode([0u8; 32]);
    }

    let tree: MerkleTree<[u8; 32], Sha256Algorithm> = MerkleTree::from_iter(hashes);
    hex::encode(tree.root())
}
