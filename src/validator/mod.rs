mod genesis;

use trie::FixedMemoryTrie;
use bigint::{U256, H256};
use block::{Header, TotalHeader, Transaction, Block};
use sha3::{Digest, Keccak256};
use rlp;
use std::collections::HashMap;
use blockchain::chain::{HeaderHash, Chain};
use sputnikvm_stateful::MemoryStateful;

pub fn transactions_root(transactions: &[Transaction]) -> H256 {
    let mut trie = FixedMemoryTrie::empty(HashMap::new());
    for (i, transaction) in transactions.iter().enumerate() {
        trie.insert(U256::from(i), transaction.clone());
    }
    trie.root()
}

pub fn ommers_hash(ommers: &[Header]) -> H256 {
    let encoded = rlp::encode_list(ommers).to_vec();
    let hash = H256::from(Keccak256::digest(&encoded).as_slice());
    hash
}

pub struct Validator {
    stateful: MemoryStateful,
    chain: Chain<H256, TotalHeader, HashMap<H256, TotalHeader>>,
}
