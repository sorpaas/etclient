use sputnikvm;
use block;

pub trait Patch {
    type VM: sputnikvm::Patch;
    type Signature: block::SignaturePatch;
    type TransactionValidation: block::ValidationPatch;
}

pub struct FrontierPatch;
impl Patch for FrontierPatch {
    type VM = sputnikvm::FrontierPatch;
    type Signature = block::GlobalSignaturePatch;
    type TransactionValidation = block::FrontierValidationPatch;
}

pub struct HomesteadPatch;
impl Patch for HomesteadPatch {
    type VM = sputnikvm::HomesteadPatch;
    type Signature = block::GlobalSignaturePatch;
    type TransactionValidation = block::HomesteadValidationPatch;
}

pub struct EIP150Patch;
impl Patch for EIP150Patch {
    type VM = sputnikvm::EIP150Patch;
    type Signature = block::GlobalSignaturePatch;
    type TransactionValidation = block::HomesteadValidationPatch;
}

pub struct EIP155Patch;
impl Patch for EIP155Patch {
    type VM = sputnikvm::EIP150Patch;
    type Signature = block::ClassicSignaturePatch;
    type TransactionValidation = block::HomesteadValidationPatch;
}

pub struct EIP160Patch;
impl Patch for EIP160Patch {
    type VM = sputnikvm::EIP160Patch;
    type Signature = block::ClassicSignaturePatch;
    type TransactionValidation = block::HomesteadValidationPatch;
}
