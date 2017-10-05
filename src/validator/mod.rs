mod genesis;

use trie::{MemoryDatabase, FixedMemoryTrie};
use bigint::{U256, H256, H64, Gas};
use block::{Header, Receipt, TotalHeader, Transaction, Block, Log, TransactionAction, ommers_hash, transactions_root, receipts_root};
use bloom::LogsBloom;
use sha3::{Digest, Keccak256};
use rlp;
use ethash;
use blockchain::chain::{HeaderHash, Chain};
use sputnikvm::{HeaderParams, VM, SeqTransactionVM, ValidTransaction};
use sputnikvm_stateful::MemoryStateful;
use patch::*;

use std::collections::HashMap;
use std::marker::PhantomData;
use std::cmp::{min, max};

pub fn validate_gas_limit(last_gas_limit: Gas, this_gas_limit: Gas) -> bool {
    let lower_bound = last_gas_limit - last_gas_limit / Gas::from(1024u64);
    let upper_bound = last_gas_limit + last_gas_limit / Gas::from(1024u64);

    this_gas_limit < upper_bound && this_gas_limit > lower_bound &&
        this_gas_limit >= Gas::from(5000u64)
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

pub struct LightDAG {
    cache: Vec<u8>,
    cache_start: U256,
    cache_size: usize,
    full_size: usize,
}

impl LightDAG {
    pub fn new(number: U256) -> Self {
        let cache_start = number - number % U256::from(ethash::EPOCH_LENGTH);
        let cache_size = ethash::get_cache_size(cache_start);
        let full_size = ethash::get_full_size(cache_start);
        let seed = ethash::get_seedhash(cache_start);

        let mut cache: Vec<u8> = Vec::with_capacity(cache_size);
        cache.resize(cache_size, 0);
        ethash::make_cache(&mut cache, seed);

        Self {
            cache, cache_start, cache_size, full_size
        }
    }

    pub fn hashimoto(&self, hash: H256, nonce: H64) -> (H256, H256) {
        ethash::hashimoto_light(hash, nonce, self.full_size, &self.cache)
    }

    pub fn is_valid_for(&self, number: U256) -> bool {
        number - self.cache_start < U256::from(ethash::EPOCH_LENGTH)
    }
}

pub struct EthereumProcessor {
    database: MemoryDatabase,
    chain: Chain<H256, TotalHeader, HashMap<H256, TotalHeader>>,
    dag: LightDAG,
}

impl EthereumProcessor {
    pub fn new() -> Self {
        let database = MemoryDatabase::default();

        let genesis = {
            let mut stateful = MemoryStateful::empty(&database);
            genesis::transit_genesis(&mut stateful);
            genesis::genesis_header(stateful.root())
        };

        Self {
            database,
            chain: Chain::new(TotalHeader::from_genesis(genesis)),
            dag: LightDAG::new(U256::zero()),
        }
    }

    pub fn put(&mut self, block: Block) -> bool {
        let parent = match self.chain.fetch(block.header.parent_hash) {
            Some(val) => val.clone(),
            None => return false,
        };
        let most_recent_block_hashes = self.chain.last_hashes(256);
        if !self.dag.is_valid_for(block.header.number) {
            self.dag = LightDAG::new(block.header.number);
        }

        {
            let mut validator: Box<Validator> = if block.header.number < U256::from(1150000) {
                Box::new(EthereumValidator::<FrontierPatch>::new(
                    &block, &parent.0, &self.database, &self.dag, &most_recent_block_hashes))
            } else if block.header.number < U256::from(2500000) {
                Box::new(EthereumValidator::<HomesteadPatch>::new(
                    &block, &parent.0, &self.database, &self.dag, &most_recent_block_hashes))
            } else if block.header.number < U256::from(3000000) {
                Box::new(EthereumValidator::<EIP150Patch>::new(
                    &block, &parent.0, &self.database, &self.dag, &most_recent_block_hashes))
            } else {
                Box::new(EthereumValidator::<EIP160Patch>::new(
                    &block, &parent.0, &self.database, &self.dag, &most_recent_block_hashes))
            };

            if !validator.validate() {
                return false;
            }
        }

        self.chain.put(TotalHeader::from_parent(block.header, &parent))
    }
}

pub trait Validator {
    fn validate(&mut self) -> bool;
}

pub struct EthereumValidator<'a, P: Patch> {
    database: &'a MemoryDatabase,
    dag: &'a LightDAG,
    current_block: &'a Block,
    parent_header: &'a Header,
    most_recent_block_hashes: &'a [H256],
    _marker: PhantomData<P>,
}

impl<'a, P: Patch> Validator for EthereumValidator<'a, P> {
    fn validate(&mut self) -> bool {
        let basic = self.validate_basic();
        let timestamp_and_difficulty = self.validate_timestamp_and_difficulty();
        let consensus = self.validate_consensus();
        let gas_limit = self.validate_gas_limit();
        let state = self.validate_state();

        basic && timestamp_and_difficulty && consensus && gas_limit && state
    }
}

impl<'a, P: Patch> EthereumValidator<'a, P> {
    pub fn new(current_block: &'a Block, parent_header: &'a Header,
               database: &'a MemoryDatabase, dag: &'a LightDAG,
               most_recent_block_hashes: &'a [H256]) -> Self {
        assert!(dag.is_valid_for(current_block.header.number));
        assert!(U256::from(most_recent_block_hashes.len()) >=
                min(current_block.header.number, U256::from(256)));

        Self {
            database, dag, current_block, parent_header, most_recent_block_hashes,
            _marker: PhantomData,
        }
    }

    pub fn validate_consensus(&self) -> bool {
        let (mix_hash, result) = self.dag.hashimoto(self.current_block.header.partial_hash(),
                                                    self.current_block.header.nonce);
        let nonce_value: u64 = self.current_block.header.nonce.into();

        mix_hash == self.current_block.header.mix_hash &&
            U256::from(nonce_value) <= ethash::cross_boundary(self.current_block.header.difficulty)
    }

    pub fn validate_basic(&self) -> bool {
        if self.current_block.header.parent_hash().is_none() {
            return false;
        }

        let transactions_valid = {
            let mut transactions_valid = true;

            for transaction in &self.current_block.transactions {
                transactions_valid = transactions_valid && transaction.is_basic_valid::<P::Signature, P::TransactionValidation>();
            }

            transactions_valid
        };

        self.current_block.is_basic_valid() &&
            transactions_valid &&
            self.current_block.header.parent_hash().unwrap() == self.parent_header.header_hash() &&
            self.current_block.header.number == self.parent_header.number + U256::one()
    }

    pub fn validate_timestamp_and_difficulty(&self) -> bool {
        self.current_block.header.timestamp > self.parent_header.timestamp &&
            self.current_block.header.difficulty == calculate_difficulty(
                self.parent_header.difficulty, self.parent_header.timestamp,
                self.current_block.header.number, self.current_block.header.timestamp)
    }

    pub fn validate_gas_limit(&self) -> bool {
        validate_gas_limit(self.parent_header.gas_limit, self.current_block.header.gas_limit)
    }

    pub fn validate_state(&mut self) -> bool {
        let mut receipts = Vec::new();
        let mut block_logs_bloom = LogsBloom::new();
        let mut block_used_gas = Gas::zero();

        let mut stateful = MemoryStateful::new(self.database, self.parent_header.state_root);

        for transaction in &self.current_block.transactions {
            let valid = match stateful.to_valid::<P::VM>(transaction.clone()) {
                Ok(val) => val,
                Err(_) => return false,
            };
            let vm: SeqTransactionVM<P::VM> = stateful.execute(
                valid, HeaderParams::from(&self.current_block.header), &self.most_recent_block_hashes);

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
                state_root: stateful.root(),
            };

            block_logs_bloom = block_logs_bloom | logs_bloom;
            block_used_gas = block_used_gas + used_gas;
            receipts.push(receipt);
        }

        // Apply block rewards
        let basic_rewards = U256::from(5000000000000000000usize);
        let main_rewards = basic_rewards + basic_rewards * U256::from(self.current_block.ommers.len()) / U256::from(32usize);
        let vm: SeqTransactionVM<P::VM> = stateful.execute(
            ValidTransaction {
                caller: None,
                gas_price: Gas::zero(),
                gas_limit: Gas::from(1000000usize),
                action: TransactionAction::Call(self.current_block.header.beneficiary),
                value: main_rewards,
                input: Vec::new(),
                nonce: U256::zero(),
            }, HeaderParams::from(&self.current_block.header), &self.most_recent_block_hashes);

        for uncle in &self.current_block.ommers {
            let sub_rewards = basic_rewards - basic_rewards * (self.current_block.header.number - uncle.number) / U256::from(8usize);
            let vm: SeqTransactionVM<P::VM> = stateful.execute(
                ValidTransaction {
                    caller: None,
                    gas_price: Gas::zero(),
                    gas_limit: Gas::from(1000000usize),
                    action: TransactionAction::Call(uncle.beneficiary),
                    value: sub_rewards,
                    input: Vec::new(),
                    nonce: U256::zero(),
                }, HeaderParams::from(&self.current_block.header), &self.most_recent_block_hashes);
        }

        self.current_block.header.state_root == stateful.root() &&
            self.current_block.header.receipts_root == receipts_root(&receipts) &&
            self.current_block.header.logs_bloom == block_logs_bloom &&
            self.current_block.header.gas_used == block_used_gas
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
