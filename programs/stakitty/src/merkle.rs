//! Sorted-pair SHA-256 Merkle tree with domain-separated leaves and nodes.
//! Leaf: sha256(0x00 || owner || range_start_le || range_end_le).
//! Node: sha256(0x01 || min(a, b) || max(a, b)).

use anchor_lang::prelude::Pubkey;
use solana_sha256_hasher::hashv;

pub fn leaf_hash(owner: &Pubkey, range_start: u128, range_end: u128) -> [u8; 32] {
    hashv(&[
        &[0u8],
        owner.as_ref(),
        &range_start.to_le_bytes(),
        &range_end.to_le_bytes(),
    ])
    .to_bytes()
}

pub fn node_hash(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    hashv(&[&[1u8], lo, hi]).to_bytes()
}

pub fn verify(proof: &[[u8; 32]], root: &[u8; 32], leaf: [u8; 32]) -> bool {
    proof
        .iter()
        .fold(leaf, |acc, sibling| node_hash(&acc, sibling))
        == *root
}
