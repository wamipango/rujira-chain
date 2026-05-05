use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{
    to_json_binary, Addr, Coin, CosmosMsg, Decimal, QuerierWrapper, StdResult, Timestamp, Uint128,
    WasmMsg,
};

use crate::{
    ghost::{
        common::{self, BorrowerResponse, DebtPoolResponse, DelegateResponse, MarketMsg},
        interest::Interest,
    },
    CallbackData, TokenMetadata,
};

#[cw_serde]
pub struct InstantiateMsg {
    /// The denom minted
    pub denom: String,
    pub interest: Interest,
    pub receipt: TokenMetadata,
    pub fee: Decimal,
    pub fee_address: String,
    pub mint_cap: Uint128,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Privileged Msgs for whitelisted contracts
    Market(MarketMsg),
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(ConfigResponse)]
    Config {},

    #[returns(StatusResponse)]
    Status {},

    #[returns(common::BorrowerResponse)]
    Borrower { addr: String },

    #[returns(common::DelegateResponse)]
    Delegate { borrower: String, addr: String },

    #[returns(common::BorrowersResponse)]
    Borrowers {
        limit: Option<u8>,
        start_after: Option<String>,
    },
}

#[cw_serde]
pub enum SudoMsg {
    SetBorrower { contract: String, limit: Uint128 },
    SetInterest(Interest),
    SetFee((Decimal, String)),
    SetMintCap(Uint128),
}

#[cw_serde]
pub struct ConfigResponse {
    pub denom: String,
    pub interest: Interest,
    pub fee_address: String,
    pub fee: Decimal,
    pub mint_cap: Uint128,
}

#[cw_serde]
pub struct StatusResponse {
    pub last_updated: Timestamp,

    pub utilization_ratio: Decimal,

    pub debt_rate: Decimal,

    // Share pool that accounts for accrued debt interest
    pub debt_pool: DebtPoolResponse,
}

#[cw_serde]
pub struct Mint(Addr);

impl From<&Addr> for Mint {
    fn from(value: &Addr) -> Self {
        Self(value.clone())
    }
}

impl Mint {
    pub fn config(&self, q: QuerierWrapper) -> StdResult<ConfigResponse> {
        q.query_wasm_smart(self.0.to_string(), &QueryMsg::Config {})
    }

    pub fn borrower(&self, q: QuerierWrapper, addr: &Addr) -> StdResult<BorrowerResponse> {
        q.query_wasm_smart(
            self.0.to_string(),
            &QueryMsg::Borrower {
                addr: addr.to_string(),
            },
        )
    }
    pub fn delegate(
        &self,
        q: QuerierWrapper,
        borrower: &Addr,
        addr: &Addr,
    ) -> StdResult<DelegateResponse> {
        q.query_wasm_smart(
            self.0.to_string(),
            &QueryMsg::Delegate {
                addr: addr.to_string(),
                borrower: borrower.to_string(),
            },
        )
    }

    pub fn msg(&self, msg: MarketMsg, funds: Vec<Coin>) -> StdResult<CosmosMsg> {
        Ok(WasmMsg::Execute {
            contract_addr: self.0.to_string(),
            msg: to_json_binary(&ExecuteMsg::Market(msg))?,
            funds,
        }
        .into())
    }

    pub fn msg_repay(&self, delegate: Option<String>, amount: &Coin) -> StdResult<CosmosMsg> {
        self.msg(MarketMsg::Repay { delegate }, vec![amount.clone()])
    }

    pub fn msg_borrow(
        &self,
        delegate: Option<String>,
        callback: Option<CallbackData>,
        amount: &Coin,
    ) -> StdResult<CosmosMsg> {
        self.msg(
            MarketMsg::Borrow {
                amount: amount.amount,
                callback,
                delegate,
            },
            vec![],
        )
    }
}
