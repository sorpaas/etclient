use sputnikvm;
use block;
use ethash;

pub trait Patch {
    type VM: sputnikvm::Patch;
    type Signature: block::SignaturePatch;
    type TransactionValidation: block::ValidationPatch;
    type Ethash: ethash::Patch;
}

pub struct FrontierPatch;
impl Patch for FrontierPatch {
    type VM = sputnikvm::FrontierPatch;
    type Signature = block::GlobalSignaturePatch;
    type TransactionValidation = block::FrontierValidationPatch;
    type Ethash = ethash::EthereumPatch;
}

pub struct HomesteadPatch;
impl Patch for HomesteadPatch {
    type VM = sputnikvm::HomesteadPatch;
    type Signature = block::GlobalSignaturePatch;
    type TransactionValidation = block::HomesteadValidationPatch;
    type Ethash = ethash::EthereumPatch;
}

pub struct EIP150Patch;
impl Patch for EIP150Patch {
    type VM = sputnikvm::EIP150Patch;
    type Signature = block::GlobalSignaturePatch;
    type TransactionValidation = block::HomesteadValidationPatch;
    type Ethash = ethash::EthereumPatch;
}

pub struct EIP160Patch;
impl Patch for EIP160Patch {
    type VM = sputnikvm::EIP160Patch;
    type Signature = block::ClassicSignaturePatch;
    type TransactionValidation = block::HomesteadValidationPatch;
    type Ethash = ethash::EthereumPatch;
}
