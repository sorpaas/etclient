use sputnikvm::vm::{VMStatus, HeaderParams, ValidTransaction, VM, SeqTransactionVM, FRONTIER_PATCH};
use sputnikvm_stateful::{Stateful, MemoryStateful};
use bigint::{H256, H64, B256, Gas, U256, Address};
use block::{Header, Block, TotalHeader, PartialHeader, Transaction, TransactionAction};
use trie::{DatabaseGuard, DatabaseOwned, MemoryTrie};
use hexutil::*;
use rlp;
use bloom::LogsBloom;
use serde_json;
use ethash;
use sha3::{Digest, Keccak256};
use std::str::FromStr;
use std::collections::HashMap;

pub fn genesis_block(state_root: H256, full_size: usize, cache: &[u8]) -> Block {
    let extra_data = read_hex("11bbe8db4e347b4e8c937c1c8370e4b5ed33adb3db69cbdb7a38e1e50b1b82fa").unwrap();
    let ommers: Vec<Header> = Vec::new();

    let mut header = Header {
        parent_hash: H256::default(),
        ommers_hash: H256::from(Keccak256::digest(&rlp::encode_list(&ommers).to_vec()).as_slice()),
        beneficiary: Address::default(),
        state_root,
        transactions_root: MemoryTrie::empty(HashMap::new()).root(),
        receipts_root: MemoryTrie::empty(HashMap::new()).root(),
        logs_bloom: LogsBloom::default(),
        difficulty: U256::from(0x400000000usize),
        number: U256::zero(),
        gas_limit: Gas::from(0x1388usize),
        gas_used: Gas::zero(),
        timestamp: 0,
        extra_data: B256::new(&extra_data),
        nonce: H64::from_str("0x0000000000000042").unwrap(),
        mix_hash: H256::default(),
    };

    let partial = PartialHeader::from_full(header.clone());
    let (mix_hash, result) = ethash::hashimoto_light(&partial, header.nonce, full_size, cache);

    header.mix_hash = mix_hash;

    Block {
        header,
        transactions: Vec::new(),
        ommers: Vec::new()
    }
}

fn transit_genesis<G: DatabaseGuard, D: DatabaseOwned>(stateful: &mut Stateful<G, D>) {
    #[derive(Serialize, Deserialize, Debug)]
    struct JSONAccount {
        balance: String,
    }

    let genesis_accounts: HashMap<String, JSONAccount> =
        serde_json::from_str(include_str!("../../res/genesis.json")).unwrap();

    let mut accounts: Vec<(&String, &JSONAccount)> = genesis_accounts.iter().collect();
    for (key, value) in accounts {
        let address = Address::from_str(key).unwrap();
        let balance = U256::from_dec_str(&value.balance).unwrap();

        let vm: SeqTransactionVM = stateful.execute(ValidTransaction {
            caller: None,
            gas_price: Gas::zero(),
            gas_limit: Gas::from(1000000usize),
            action: TransactionAction::Call(address),
            value: balance,
            input: Vec::new(),
            nonce: U256::zero(),
        }, HeaderParams {
            beneficiary: Address::default(),
            timestamp: 0,
            number: U256::zero(),
            difficulty: U256::from(0x400000000usize),
            gas_limit: Gas::from(0x1388usize)
        }, &FRONTIER_PATCH, &[]);
        match vm.status() {
            VMStatus::ExitedOk => (),
            _ => panic!(),
        }
    }
}
