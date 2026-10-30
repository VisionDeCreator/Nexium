//! Isolated account/resource Move boundary. Not yet dispatched by the node.
//!
//! A fresh VM per session avoids stale module caches across rollback. No native
//! functions, chain framework or privileged system modules are installed here.
use anyhow::{ensure, Result};
use move_binary_format::CompiledModule;
use move_core_types::{
    account_address::AccountAddress,
    language_storage::{ModuleId, StructTag},
    resolver::{ModuleResolver, ResourceResolver},
};
use move_vm_runtime::move_vm::MoveVM;
use std::collections::BTreeMap;

pub const MOVE_REVISION: &str = "cc98ac02caa73e64e6c8b781c61de356b45801c4";
pub const MAX_MODULE_BYTES: usize = 64 * 1024;

#[derive(Default)]
pub struct ResourceStore {
    modules: BTreeMap<ModuleId, Vec<u8>>,
    resources: BTreeMap<(AccountAddress, StructTag), Vec<u8>>,
}

impl ModuleResolver for ResourceStore {
    type Error = String;
    fn get_module(&self, id: &ModuleId) -> Result<Option<Vec<u8>>, String> {
        Ok(self.modules.get(id).cloned())
    }
}

impl ResourceResolver for ResourceStore {
    type Error = String;
    fn get_resource(
        &self,
        address: &AccountAddress,
        tag: &StructTag,
    ) -> Result<Option<Vec<u8>>, String> {
        Ok(self.resources.get(&(*address, tag.clone())).cloned())
    }
}

/// Pure verification only: publishing requires authenticated sender, gas and
/// framework policies, which belong to the next integration milestone.
pub fn verify_module(bytes: &[u8], publisher: AccountAddress) -> Result<ModuleId> {
    ensure!(bytes.len() <= MAX_MODULE_BYTES, "module size limit");
    let module =
        CompiledModule::deserialize(bytes).map_err(|error| anyhow::anyhow!("{error:?}"))?;
    move_bytecode_verifier::verify_module(&module).map_err(|error| anyhow::anyhow!("{error:?}"))?;
    let id = module.self_id();
    ensure!(id.address() == &publisher, "module publisher mismatch");
    Ok(id)
}

pub fn new_vm() -> Result<MoveVM> {
    MoveVM::new(vec![]).map_err(|error| anyhow::anyhow!("{error:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_session_has_no_effects_and_uses_32_byte_addresses() {
        assert_eq!(AccountAddress::LENGTH, 32);
        let store = ResourceStore::default();
        let vm = new_vm().unwrap();
        let (changes, events) = vm.new_session(&store).finish().unwrap();
        assert!(changes.accounts().is_empty());
        assert!(events.is_empty());
    }
    #[test]
    fn malformed_module_rejected() {
        assert!(verify_module(&[1, 2, 3], AccountAddress::ZERO).is_err());
        assert!(verify_module(&vec![0; MAX_MODULE_BYTES + 1], AccountAddress::ZERO).is_err());
    }
    #[test]
    fn verified_module_must_match_the_publisher() {
        let module = move_binary_format::file_format::empty_module();
        let mut bytes = vec![];
        module.serialize(&mut bytes).unwrap();
        assert_eq!(
            verify_module(&bytes, *module.self_id().address()).unwrap(),
            module.self_id()
        );
        assert!(verify_module(&bytes, AccountAddress::new([1; 32])).is_err());
    }
    #[test]
    fn pinned_compiler_bytecode_executes_in_pinned_vm() {
        use move_core_types::identifier::Identifier;
        use move_vm_types::gas::UnmeteredGasMeter;
        let directory = tempfile::TempDir::new().unwrap();
        let source = directory.path().join("Answer.move");
        std::fs::write(
            &source,
            "module 0x42::Answer { public fun answer(): u64 { 42 } }",
        )
        .unwrap();
        let (_, compiled) = move_compiler::Compiler::from_files(
            vec![source.to_string_lossy().to_string()],
            Vec::<String>::new(),
            BTreeMap::<String, move_compiler::shared::NumericalAddress>::new(),
        )
        .build()
        .unwrap();
        let (mut units, _) = compiled.expect("fixture must compile");
        let bytes = units.pop().unwrap().into_compiled_unit().serialize(None);
        let id = verify_module(&bytes, AccountAddress::from_hex_literal("0x42").unwrap()).unwrap();
        let mut store = ResourceStore::default();
        store.modules.insert(id.clone(), bytes);
        let vm = new_vm().unwrap();
        let mut session = vm.new_session(&store);
        // Trusted constant-return fixture only. Unmetered execution is never exposed by the node.
        let result = session
            .execute_function_bypass_visibility(
                &id,
                &Identifier::new("answer").unwrap(),
                vec![],
                Vec::<Vec<u8>>::new(),
                &mut UnmeteredGasMeter,
            )
            .unwrap();
        assert_eq!(
            bcs::from_bytes::<u64>(&result.return_values[0].0).unwrap(),
            42
        );
        let (changes, events) = session.finish().unwrap();
        assert!(changes.accounts().is_empty());
        assert!(events.is_empty());
    }
}
