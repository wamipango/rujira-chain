use anyhow::{Context, Result as AnyResult};
use cosmwasm_std::{Addr, Binary, Storage};
use serde::Deserialize;

use crate::mock::RujiraApp;

#[derive(Debug, Deserialize)]
pub struct ExportedContractState {
    #[serde(default)]
    pub models: Vec<ExportedStateModel>,
    #[serde(default, rename = "data")]
    pub data: Vec<ExportedStateModel>,
}

impl ExportedContractState {
    pub fn iter(&self) -> impl Iterator<Item = &ExportedStateModel> {
        self.models.iter().chain(self.data.iter())
    }
}

#[derive(Debug, Deserialize)]
pub struct ExportedStateModel {
    pub key: String,
    pub value: String,
}

pub fn load_contract_state_all(
    app: &mut RujiraApp,
    contract_addr: &Addr,
    export: &ExportedContractState,
) -> AnyResult<usize> {
    let mut storage = app.contract_storage_mut(contract_addr);
    let mut loaded = 0usize;

    for model in export.iter() {
        let key = Binary::from_base64(&model.key)
            .with_context(|| format!("invalid base64 key: {}", model.key))?;
        let value = Binary::from_base64(&model.value)
            .with_context(|| format!("invalid base64 value for key {}", model.key))?;
        storage.set(key.as_slice(), value.as_slice());
        loaded += 1;
    }

    Ok(loaded)
}

pub fn load_contract_state_all_from_slice(
    app: &mut RujiraApp,
    contract_addr: &Addr,
    data: &[u8],
) -> AnyResult<usize> {
    let export: ExportedContractState =
        serde_json::from_slice(data).context("failed to parse contract-state JSON")?;
    load_contract_state_all(app, contract_addr, &export)
}

pub fn load_contract_state_all_from_path(
    app: &mut RujiraApp,
    contract_addr: &Addr,
    path: impl AsRef<std::path::Path>,
) -> AnyResult<usize> {
    let data = std::fs::read(path.as_ref()).with_context(|| {
        format!(
            "failed to read contract-state JSON from {}",
            path.as_ref().display()
        )
    })?;
    load_contract_state_all_from_slice(app, contract_addr, &data)
}
