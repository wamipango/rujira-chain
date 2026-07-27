use crate::{fin, ghost, CallbackData};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{
    to_json_binary, Addr, Coin, CosmosMsg, Decimal, QuerierWrapper, StdResult, Uint128, WasmMsg,
};

pub const DEFAULT_SPREAD_BPS: u32 = 10;

pub fn default_spread_bps() -> u32 {
    DEFAULT_SPREAD_BPS
}

#[cw_serde]
pub struct InstantiateMsg {
    /// The maximum number of blocks that a settlement can be chunked into over a streaming swap
    pub max_stream_length: u32,
    /// The amount by which the `size` of the quote changes for each N of max_stream_length
    /// Allows the time-based price risk to be managed for quotes that settle over streaming swaps
    pub stream_step_ratio: Decimal,
    /// The spread of each quote, in basis points of pool depth
    #[serde(default = "default_spread_bps")]
    pub spread_bps: u32,
    /// The premium charged on quotes to generate a local surplus
    pub reserve_fee: Decimal,
    /// The maximum percentage of a pool's depth that can be borrowed
    pub max_borrow_ratio: Decimal,
    /// The minimum amount borrowed in order to execute a swap. Required to avoid a ZeroShare issue from a ghost-vault for tiny swaps
    pub min_borrow_amount: Uint128,
    /// AMM fee rate and destination address
    pub fee: (Decimal, String),
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Executes a market trade based on current order book.
    Swap {
        min_return: Coin,
        to: Option<String>,
        callback: Option<CallbackData>,
    },

    /// Only executes the loan repayment function
    Repay {},
}

#[cw_serde]
pub enum SudoMsg {
    SetConfig(ConfigUpdate),
    SetMarket { addr: String, enabled: bool },
    SetVault { denom: String, vault: Option<Vault> },
    Execute(CosmosMsg),
}

#[cw_serde]
pub struct ConfigUpdate {
    pub max_stream_length: Option<u32>,
    pub stream_step_ratio: Option<Decimal>,
    pub spread_bps: Option<u32>,
    pub max_borrow_ratio: Option<Decimal>,
    pub reserve_fee: Option<Decimal>,
    pub min_borrow_amount: Option<Uint128>,
    pub fee: Option<(Decimal, String)>,
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(ConfigResponse)]
    Config {},

    #[returns(fin::market_maker::QuoteResponse)]
    Quote(fin::market_maker::QuoteRequest),

    #[returns(MarketsResponse)]
    Markets {},

    #[returns(VaultsResponse)]
    Vaults {},
}

#[cw_serde]
pub struct ConfigResponse {
    pub max_stream_length: u32,
    pub stream_step_ratio: Decimal,
    pub spread_bps: u32,
    pub max_borrow_ratio: Decimal,
    pub min_borrow_amount: Uint128,
    pub reserve_fee: Decimal,
    pub fee: (Decimal, Addr),
}

#[cw_serde]
pub struct VaultsResponse {
    pub vaults: Vec<VaultResponse>,
}

#[cw_serde]
pub struct MarketsResponse {
    pub markets: Vec<Addr>,
}

#[cw_serde]
pub struct VaultResponse {
    pub denom: String,
    pub vault: Vault,
}

#[cw_serde]
pub struct Vault(Addr);

impl From<Addr> for Vault {
    fn from(value: Addr) -> Self {
        Self(value)
    }
}

impl Vault {
    pub fn borrow_msg(&self, amount: Uint128) -> StdResult<WasmMsg> {
        Ok(WasmMsg::Execute {
            contract_addr: self.0.to_string(),
            msg: to_json_binary(&ghost::vault::ExecuteMsg::Market(
                ghost::common::MarketMsg::Borrow {
                    amount,
                    callback: None,
                    delegate: None,
                },
            ))?,
            funds: vec![],
        })
    }

    pub fn repay_msg(&self, amount: Coin) -> StdResult<WasmMsg> {
        Ok(WasmMsg::Execute {
            contract_addr: self.0.to_string(),
            msg: to_json_binary(&ghost::vault::ExecuteMsg::Market(
                ghost::common::MarketMsg::Repay { delegate: None },
            ))?,
            funds: vec![amount],
        })
    }

    pub fn debt(&self, q: QuerierWrapper, owner: &Addr) -> StdResult<Uint128> {
        let res: ghost::common::BorrowerResponse = q.query_wasm_smart(
            self.0.clone(),
            &ghost::vault::QueryMsg::Borrower {
                addr: owner.to_string(),
            },
        )?;
        Ok(res.current)
    }

    pub fn available(&self, q: QuerierWrapper, owner: &Addr) -> StdResult<Uint128> {
        let res: ghost::common::BorrowerResponse = q.query_wasm_smart(
            self.0.clone(),
            &ghost::vault::QueryMsg::Borrower {
                addr: owner.to_string(),
            },
        )?;
        Ok(res.available)
    }

    pub fn addr(&self) -> Addr {
        self.0.clone()
    }
}
