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

#[cfg(test)]
mod tests {
    use super::*;

    const LEAF_A: &str = "44f8f9b9ab53ca607c21a460616c4b0ab54c06658f5a5cde06a93c3af187deb0";
    const ROOT: &str = "01f395e5fe84cc61f884f544ba6b5c84114430d34aa555c04fd5e725e5e46f77";

    /// Shared with `scripts/lib/merkle.test.ts`: both implementations must produce these bytes.
    #[test]
    fn cross_language_vector() {
        let alice = Pubkey::new_from_array([1u8; 32]);
        let bob = Pubkey::new_from_array([2u8; 32]);
        let carol = Pubkey::new_from_array([3u8; 32]);
        let a = leaf_hash(&alice, 0, 3_000);
        let b = leaf_hash(&bob, 3_000, 4_000);
        let c = leaf_hash(&carol, 4_000, 4_500);
        let root = node_hash(&node_hash(&a, &b), &c);
        let hex = |h: [u8; 32]| h.iter().map(|x| format!("{x:02x}")).collect::<String>();
        assert_eq!(hex(a), LEAF_A);
        assert_eq!(hex(root), ROOT);
        assert!(verify(&[b, c], &root, a));
        assert!(verify(&[node_hash(&a, &b)], &root, c));
        assert!(!verify(&[b, c], &root, leaf_hash(&alice, 0, 3_001)));
    }
}
