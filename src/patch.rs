use sputnikvm;
use block;
use ethash;

use bigint::U256;
use std::ops::Shr;
use std::cmp::min;
use std::marker::PhantomData;

pub trait BaseRewardPatch {
    fn base_reward() -> U256;
}

pub struct EthereumBaseRewardPatch;
impl BaseRewardPatch for EthereumBaseRewardPatch {
    fn base_reward() -> U256 { U256::from(5000000000000000000usize) }
}

pub trait RewardPatch {
    fn block_reward(number: U256, uncles: usize) -> U256;
    fn uncle_reward(distance: U256) -> U256;
}

pub struct FrontierRewardPatch<P: BaseRewardPatch>(PhantomData<P>);
impl<P: BaseRewardPatch> RewardPatch for FrontierRewardPatch<P> {
    fn block_reward(number: U256, uncles: usize) -> U256 { P::base_reward() + P::base_reward().shr(5) * U256::from(uncles) }
    fn uncle_reward(distance: U256) -> U256 {
        (P::base_reward() * (U256::from(8) - distance)).shr(3)
    }
}

fn era(block_number: U256, era_rounds: U256) -> usize {
    if block_number != U256::zero() && block_number % era_rounds == U256::zero() {
		(block_number / era_rounds - U256::one()).as_usize()
	} else {
		(block_number / era_rounds).as_usize()
    }
}

pub trait EraPatch {
    fn era_rounds() -> U256;
}

pub struct ClassicEraPatch;
impl EraPatch for ClassicEraPatch {
    fn era_rounds() -> U256 { U256::from(5000000) }
}

pub struct EraReducedRewardPatch<B: BaseRewardPatch, E: EraPatch>(PhantomData<(B, E)>);
impl<B: BaseRewardPatch, E: EraPatch> RewardPatch for EraReducedRewardPatch<B, E> {
    fn block_reward(number: U256, uncles: usize) -> U256 {
        let mut reward = B::base_reward();
        for _ in 0..era(number, E::era_rounds()) {
            reward = reward / U256::from(5) * U256::from(4);
        }
        reward
    }

    fn uncle_reward(distance: U256) -> U256 {
        B::base_reward().shr(5)
    }
}

pub trait BaseTargetDifficultyPatch {
    fn base_target_difficulty(
        last_difficulty: U256, last_timestamp: u64, this_timestamp: u64
    ) -> U256;
}

pub struct FrontierBaseTargetDifficultyPatch;
impl BaseTargetDifficultyPatch for FrontierBaseTargetDifficultyPatch {
    fn base_target_difficulty(
        last_difficulty: U256, last_timestamp: u64, this_timestamp: u64
    ) -> U256 {
        let difficulty_bound_divisor = U256::from(0x0800);
        let duration_limit = 0x0d;

        if this_timestamp >= last_timestamp + duration_limit {
            last_difficulty - (last_difficulty / difficulty_bound_divisor)
        } else {
            last_difficulty + (last_difficulty / difficulty_bound_divisor)
        }
    }
}

pub struct HomesteadBaseTargetDifficultyPatch;
impl BaseTargetDifficultyPatch for HomesteadBaseTargetDifficultyPatch {
    fn base_target_difficulty(
        last_difficulty: U256, last_timestamp: u64, this_timestamp: u64
    ) -> U256 {
        let difficulty_bound_divisor = U256::from(0x0800);

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
    }
}

pub trait DifficultyBombPatch {
    fn difficulty_bomb(this_number: U256) -> U256;
}

pub struct FrontierDifficultyBombPatch;
impl DifficultyBombPatch for FrontierDifficultyBombPatch {
    fn difficulty_bomb(this_number: U256) -> U256 {
        let exp_diff_period = U256::from(100000);

        let period = (this_number / exp_diff_period).as_usize();
        if period > 1 {
            U256::from(1) << (period - 2)
        } else {
            U256::zero()
        }
    }
}

pub trait DelayedPatch {
    fn pause_block_number() -> U256;
    fn continue_block_number() -> U256;
}

pub struct ClassicDelayedPatch;
impl DelayedPatch for ClassicDelayedPatch {
    fn pause_block_number() -> U256 { U256::from(3000000) }
    fn continue_block_number() -> U256 { U256::from(5000000) }
}

pub struct DelayedDifficultyBombPatch<P: DelayedPatch>(PhantomData<P>);
impl<P: DelayedPatch> DifficultyBombPatch for DelayedDifficultyBombPatch<P> {
    fn difficulty_bomb(this_number: U256) -> U256 {
        let exp_diff_period = U256::from(100000);

        if this_number < P::pause_block_number() {
            let period = (this_number / exp_diff_period).as_usize();
            if period > 1 {
                U256::from(1) << (period - 2)
            } else {
                U256::zero()
            }
        } else if this_number < P::continue_block_number() {
            let fixed_difficulty = ((P::pause_block_number() / exp_diff_period) - U256::from(2)).as_usize();
            U256::from(1) << fixed_difficulty
        } else {
            let period = (this_number / exp_diff_period).as_usize();
            let delay = ((P::continue_block_number() - P::pause_block_number()) / exp_diff_period).as_usize();
            U256::from(1) << (period - delay - 2)
        }
    }
}

pub trait Patch {
    type VM: sputnikvm::Patch;
    type Signature: block::SignaturePatch;
    type TransactionValidation: block::ValidationPatch;
    type Ethash: ethash::Patch;
    type BaseTargetDifficulty: BaseTargetDifficultyPatch;
    type DifficultyBomb: DifficultyBombPatch;
    type Reward: RewardPatch;
}

pub struct FrontierPatch;
impl Patch for FrontierPatch {
    type VM = sputnikvm::FrontierPatch;
    type Signature = block::GlobalSignaturePatch;
    type TransactionValidation = block::FrontierValidationPatch;
    type Ethash = ethash::EthereumPatch;
    type BaseTargetDifficulty = FrontierBaseTargetDifficultyPatch;
    type DifficultyBomb = FrontierDifficultyBombPatch;
    type Reward = FrontierRewardPatch<EthereumBaseRewardPatch>;
}

pub struct HomesteadPatch;
impl Patch for HomesteadPatch {
    type VM = sputnikvm::HomesteadPatch;
    type Signature = block::GlobalSignaturePatch;
    type TransactionValidation = block::HomesteadValidationPatch;
    type Ethash = ethash::EthereumPatch;
    type BaseTargetDifficulty = HomesteadBaseTargetDifficultyPatch;
    type DifficultyBomb = FrontierDifficultyBombPatch;
    type Reward = FrontierRewardPatch<EthereumBaseRewardPatch>;
}

pub struct EIP150Patch;
impl Patch for EIP150Patch {
    type VM = sputnikvm::EIP150Patch;
    type Signature = block::GlobalSignaturePatch;
    type TransactionValidation = block::HomesteadValidationPatch;
    type Ethash = ethash::EthereumPatch;
    type BaseTargetDifficulty = HomesteadBaseTargetDifficultyPatch;
    type DifficultyBomb = FrontierDifficultyBombPatch;
    type Reward = FrontierRewardPatch<EthereumBaseRewardPatch>;
}

pub struct EIP160Patch;
impl Patch for EIP160Patch {
    type VM = sputnikvm::EIP160Patch;
    type Signature = block::ClassicSignaturePatch;
    type TransactionValidation = block::HomesteadValidationPatch;
    type Ethash = ethash::EthereumPatch;
    type BaseTargetDifficulty = HomesteadBaseTargetDifficultyPatch;
    type DifficultyBomb = DelayedDifficultyBombPatch<ClassicDelayedPatch>;
    type Reward = FrontierRewardPatch<EthereumBaseRewardPatch>;
}

pub struct ECIP1017Patch;
impl Patch for ECIP1017Patch {
    type VM = sputnikvm::EIP160Patch;
    type Signature = block::ClassicSignaturePatch;
    type TransactionValidation = block::HomesteadValidationPatch;
    type Ethash = ethash::EthereumPatch;
    type BaseTargetDifficulty = HomesteadBaseTargetDifficultyPatch;
    type DifficultyBomb = DelayedDifficultyBombPatch<ClassicDelayedPatch>;
    type Reward = EraReducedRewardPatch<EthereumBaseRewardPatch, ClassicEraPatch>;
}
