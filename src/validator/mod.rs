mod genesis;

use trie::FixedMemoryTrie;
use bigint::{U256, H256};
use block::{Header, TotalHeader, PartialHeader, Transaction, Block};
use sha3::{Digest, Keccak256};
use rlp;
use ethash;
use blockchain::chain::{HeaderHash, Chain};
use sputnikvm_stateful::MemoryStateful;

use std::collections::HashMap;
use std::cmp::{min, max};

pub fn calculate_difficulty(
    last_difficulty: U256, last_timestamp: u64, this_number: U256, this_timestamp: u64
) -> U256 {
    let exp_diff_period = U256::from(100000);

    let min_difficulty = U256::from(125000);
    let difficulty_bound_divisor = U256::from(0x0800);

    let duration_limit = 0x0d;
    let frontier_limit = U256::from(1150000);

    let mut target = if this_number < frontier_limit {
        if this_timestamp >= last_timestamp + duration_limit {
            last_difficulty - (last_difficulty / difficulty_bound_divisor)
        } else {
            last_difficulty + (last_difficulty / difficulty_bound_divisor)
        }
    } else {
        let increment_divisor = 10;
        let threshold = 1;

        let diff_inc = (this_timestamp - last_timestamp) / increment_divisor;
        if diff_inc <= threshold {
            last_difficulty +
                last_difficulty / difficulty_bound_divisor * (threshold - diff_inc).into()
        } else {
            let multiplier = min(diff_inc - threshold, 99).into();
            last_difficulty.saturating_sub(
                last_difficulty / difficulty_bound_divisor * multiplier
            )
        }
    };
    target = max(min_difficulty, target);

    let ecip1010_pause_transition = U256::from(3000000);
    let ecip1010_continue_transition = U256::from(5000000);

    if this_number < ecip1010_pause_transition {
        let period = (this_number / exp_diff_period).as_usize();
        if period > 1 {
            target = max(min_difficulty, target + (U256::from(1) << (period - 2)));
        }
    } else if this_number < ecip1010_continue_transition {
        let fixed_difficulty = ((ecip1010_pause_transition / exp_diff_period) - U256::from(2)).as_usize();
        target = max(min_difficulty, target + (U256::from(1) << fixed_difficulty));
    } else {
        let period = (this_number / exp_diff_period).as_usize();
        let delay = ((ecip1010_continue_transition - ecip1010_pause_transition) / exp_diff_period).as_usize();
        target = max(min_difficulty, target + (U256::from(1) << (period - delay - 2)));
    }
    target
}

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
    cache_start: U256,
    cache: Vec<u8>,
    full_size: usize,
}

impl Validator {
    pub fn new() -> Validator {
        let mut stateful = MemoryStateful::default();
        genesis::transit_genesis(&mut stateful);

        let genesis = genesis::genesis_block(stateful.root());
        let chain = Chain::new(TotalHeader::from_genesis(genesis.header));

        let mut validator = Validator {
            stateful, chain, cache_start: U256::zero(),
            cache: Vec::new(), full_size: 0,
        };

        validator.regenerate_dag();
        validator
    }

    pub fn regenerate_dag(&mut self) {
        self.cache_start = self.chain.best().0.number -
            self.chain.best().0.number % U256::from(ethash::EPOCH_LENGTH);
        let cache_size = ethash::get_cache_size(self.cache_start);
        let full_size = ethash::get_full_size(self.cache_start);
        let seed = ethash::get_seedhash(self.cache_start);

        let mut cache: Vec<u8> = Vec::with_capacity(cache_size);
        cache.resize(cache_size, 0);
        ethash::make_cache(&mut cache, seed);

        self.cache = cache;
        self.full_size = full_size;
    }

    pub fn validate_consensus(&self, header: &Header) -> bool {
        assert!(header.number - self.cache_start < U256::from(ethash::EPOCH_LENGTH));
        let partial = PartialHeader::from_full(header.clone());

        let (mix_hash, result) = ethash::hashimoto_light(&partial, header.nonce,
                                                        self.full_size, &self.cache);
        mix_hash == header.mix_hash
    }

    pub fn validate_basic(&self, block: &Block) -> bool {
        block.header.parent_hash() == self.chain.best().0.header_hash() &&
            block.header.number == self.chain.best().0.number + U256::one() &&
            block.header.transactions_root == transactions_root(&block.transactions) &&
            block.header.ommers_hash == ommers_hash(&block.ommers)
    }

    pub fn validate_difficulty(&self, header: &Header) -> bool {
        unimplemented!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_difficulty() {
        assert_eq!(calculate_difficulty(U256::from(17179869184usize), 0,
                                        U256::from(1), 1438269988),
                   U256::from(17171480576usize));
        assert_eq!(calculate_difficulty(U256::from(17171480576usize), 1438269988,
                                        U256::from(2), 1438270017),
                   U256::from(17163096064usize));
    }
}
