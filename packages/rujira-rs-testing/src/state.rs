use anyhow::{Context, Result as AnyResult};
use cosmwasm_std::{from_base64, from_hex, Addr, Coin};
use cw_multi_test::Stargate;
use serde::Deserialize;

use crate::mock::RujiraApp;

#[derive(Debug, Deserialize)]
struct ExportedContractState {
    #[serde(default)]
    models: Vec<ExportedStateModel>,
}

#[derive(Debug, Deserialize)]
struct ExportedStateModel {
    key: String,
    value: String,
}

#[derive(Debug, Deserialize)]
struct ExportedBankState {
    #[serde(default)]
    balances: Vec<Coin>,
}

pub fn load_contract_state<T: Stargate>(
    app: &mut RujiraApp<T>,
    contract_addr: &Addr,
    label: &str,
    height: u64,
) -> AnyResult<()> {
    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/mock/state")
        .join(height.to_string());
    let wasm_path = base.join("wasm").join(format!("{label}.json"));
    let bank_path = base.join("bank").join(format!("{label}.json"));

    let wasm_data = std::fs::read(&wasm_path)
        .with_context(|| format!("failed to read wasm state from {}", wasm_path.display()))?;

    load_wasm_state_from_slice(app, contract_addr, &wasm_data).with_context(|| {
        format!(
            "failed to load wasm state for label '{}' at height {}",
            label, height
        )
    })?;

    let bank_data = std::fs::read(&bank_path)
        .with_context(|| format!("failed to read bank balances from {}", bank_path.display()))?;
    load_bank_balances_from_slice(app, contract_addr, &bank_data)?;

    Ok(())
}

fn load_wasm_state_from_slice<T: Stargate>(
    app: &mut RujiraApp<T>,
    contract_addr: &Addr,
    data: &[u8],
) -> AnyResult<usize> {
    let export: ExportedContractState =
        serde_json::from_slice(data).context("failed to parse contract-state JSON")?;

    let mut storage = app.contract_storage_mut(contract_addr);
    let mut loaded = 0usize;

    for model in export.models.iter() {
        let key =
            from_hex(&model.key).with_context(|| format!("invalid base64 key: {}", model.key))?;
        let value = from_base64(&model.value)
            .with_context(|| format!("invalid base64 value for key {}", model.key))?;
        storage.set(key.as_slice(), value.as_slice());
        loaded += 1;
    }

    Ok(loaded)
}

fn load_bank_balances_from_slice<T: Stargate>(
    app: &mut RujiraApp<T>,
    contract_addr: &Addr,
    data: &[u8],
) -> AnyResult<()> {
    let export: ExportedBankState =
        serde_json::from_slice(data).context("failed to parse bank balances JSON")?;

    app.init_modules(|router, _, storage| {
        router
            .bank
            .init_balance(storage, contract_addr, export.balances)
    })?;
    Ok(())
}
