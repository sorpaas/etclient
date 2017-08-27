mod genesis;

use trie::FixedMemoryTrie;
use bigint::{U256, H256, Gas};
use block::{Header, Receipt, TotalHeader, PartialHeader, Transaction, Block, Log};
use bloom::LogsBloom;
use sha3::{Digest, Keccak256};
use rlp;
use ethash;
use blockchain::chain::{HeaderHash, Chain};
use sputnikvm::vm::{self, Patch, HeaderParams, VM, SeqTransactionVM};
use sputnikvm_stateful::MemoryStateful;

use std::collections::HashMap;
use std::cmp::{min, max};

pub fn patch(number: U256) -> &'static Patch {
    if number < U256::from(1150000) {
        &vm::FRONTIER_PATCH
    } else if number < U256::from(2500000) {
        &vm::HOMESTEAD_PATCH
    } else if number < U256::from(3000000) {
        &vm::EIP150_PATCH
    } else {
        &vm::EIP160_PATCH
    }
}

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

pub fn receipts_root(receipts: &[Receipt]) -> H256 {
    let mut trie = FixedMemoryTrie::empty(HashMap::new());
    for (i, receipt) in receipts.iter().enumerate() {
        trie.insert(U256::from(i), receipt.clone());
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

        validator.regenerate_dag(U256::zero());
        validator
    }

    pub fn regenerate_dag(&mut self, number: U256) {
        self.cache_start = number - number % U256::from(ethash::EPOCH_LENGTH);
        let cache_size = ethash::get_cache_size(self.cache_start);
        let full_size = ethash::get_full_size(self.cache_start);
        let seed = ethash::get_seedhash(self.cache_start);

        let mut cache: Vec<u8> = Vec::with_capacity(cache_size);
        cache.resize(cache_size, 0);
        ethash::make_cache(&mut cache, seed);

        self.cache = cache;
        self.full_size = full_size;
    }

    pub fn append_block(&mut self, block: Block) {
        println!("current best: {:?}", self.chain.best());
        assert!(self.validate(&block));

        let total = TotalHeader::from_parent(block.header, self.chain.best());
        self.chain.put(total);
    }

    pub fn validate(&mut self, block: &Block) -> bool {
        if block.header.number - self.cache_start >= U256::from(ethash::EPOCH_LENGTH) {
            self.regenerate_dag(block.header.number);
        }

        self.validate_basic(block) &&
            self.validate_timestamp_and_difficulty(&block.header) &&
            self.validate_consensus(&block.header) &&
            self.validate_gas_limit(&block.header) &&
            self.validate_state(block)
    }

    pub fn validate_consensus(&self, header: &Header) -> bool {
        assert!(header.number - self.cache_start < U256::from(ethash::EPOCH_LENGTH));
        let partial = PartialHeader::from_full(header.clone());

        let (mix_hash, result) = ethash::hashimoto_light(&partial, header.nonce,
                                                         self.full_size, &self.cache);

        // TODO: nonce <= 2^256 / difficulty
        mix_hash == header.mix_hash
    }

    pub fn validate_basic(&self, block: &Block) -> bool {
        block.header.parent_hash() == self.chain.best().0.header_hash() &&
            block.header.number == self.chain.best().0.number + U256::one() &&
            block.header.transactions_root == transactions_root(&block.transactions) &&
            block.header.ommers_hash == ommers_hash(&block.ommers)
    }

    pub fn validate_timestamp_and_difficulty(&self, header: &Header) -> bool {
        header.timestamp > self.chain.best().0.timestamp &&
            header.difficulty == calculate_difficulty(self.chain.best().0.difficulty,
                                                      self.chain.best().0.timestamp,
                                                      header.number, header.timestamp)
    }

    pub fn validate_gas_limit(&self, header: &Header) -> bool {
        let parent_gas_limit = self.chain.best().0.gas_limit;
        let lower_bound = parent_gas_limit - parent_gas_limit / Gas::from(1024u64);
        let upper_bound = parent_gas_limit + parent_gas_limit / Gas::from(1024u64);

        header.gas_limit < upper_bound && header.gas_limit > lower_bound &&
            header.gas_limit >= Gas::from(125000u64)
    }

    pub fn validate_state(&mut self, block: &Block) -> bool {
        let patch = patch(block.header.number);
        let mut most_recent_block_hashes = Vec::new();
        let mut next_hash = self.chain.best().header_hash();
        for _ in 0..256 {
            most_recent_block_hashes.push(next_hash);
            let this_block = self.chain.fetch(next_hash).unwrap();
            next_hash = this_block.parent_hash();
        }

        let mut receipts = Vec::new();
        let mut block_logs_bloom = LogsBloom::new();
        let mut block_used_gas = Gas::zero();

        for transaction in &block.transactions {
            let valid = match self.stateful.to_valid(transaction.clone(), patch) {
                Ok(val) => val,
                Err(_) => return false,
            };
            let vm: SeqTransactionVM = self.stateful.execute(
                valid, HeaderParams::from(&block.header), patch, &most_recent_block_hashes);

            let logs: Vec<Log> = vm.logs().into();
            let used_gas = vm.real_used_gas();
            let mut logs_bloom = LogsBloom::new();
            for log in logs.clone() {
                logs_bloom.set(&log.address);
                for topic in log.topics {
                    logs_bloom.set(&topic)
                }
            }

            let receipt = Receipt {
                used_gas: used_gas.clone(),
                logs,
                logs_bloom: logs_bloom.clone(),
                state_root: self.stateful.root(),
            };

            block_logs_bloom = block_logs_bloom | logs_bloom;
            block_used_gas = block_used_gas + used_gas;
            receipts.push(receipt);
        }

        block.header.state_root == self.stateful.root() &&
            block.header.receipts_root == receipts_root(&receipts) &&
            block.header.logs_bloom == block_logs_bloom &&
            block.header.gas_used == block_used_gas
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
