use crate::msg::token_factory::{
    MsgBurnTokens, MsgCreateDenom, MsgMintTokens, MsgSetDenomAdmin, MsgSetMetadata,
};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    Addr, Coin, CosmosMsg, DenomMetadata, DenomUnit, Env, QuerierWrapper, StdResult, Uint128,
};

pub struct TokenFactory {
    id: String,
    env: Env,
}

impl TokenFactory {
    pub fn new(env: &Env, id: &str) -> Self {
        Self {
            env: env.clone(),
            id: id.to_string(),
        }
    }

    pub fn denom(&self) -> String {
        format!("x/{}", self.id)
    }

    pub fn supply(&self, q: QuerierWrapper) -> StdResult<Uint128> {
        Ok(q.query_supply(self.denom())?.amount)
    }

    pub fn create_msg(&self, metadata: TokenMetadata) -> CosmosMsg {
        MsgCreateDenom {
            sender: self.env.contract.address.clone(),
            id: self.id.clone(),
            metadata: metadata.to_sdk(self.denom()),
        }
        .into()
    }

    pub fn mint_msg(&self, amount: Uint128, recipient: Addr) -> CosmosMsg {
        MsgMintTokens {
            sender: self.env.contract.address.clone(),
            amount: Coin::new(amount, self.denom()),
            recipient,
        }
        .into()
    }

    pub fn burn_msg(&self, amount: Uint128) -> CosmosMsg {
        MsgBurnTokens {
            sender: self.env.contract.address.clone(),
            amount: Coin::new(amount, self.denom()),
        }
        .into()
    }

    pub fn set_admin_msg(&self, new_admin: Addr) -> CosmosMsg {
        MsgSetDenomAdmin {
            sender: self.env.contract.address.clone(),
            denom: self.denom(),
            new_admin,
        }
        .into()
    }

    pub fn set_metadata_msg(&self, new_metadata: TokenMetadata) -> CosmosMsg {
        MsgSetMetadata {
            sender: self.env.contract.address.clone(),
            denom: self.denom(),
            new_metadata: new_metadata.to_sdk(self.denom()),
        }
        .into()
    }
}

/// Metadata represents a struct that describes a basic token.
///
/// It follows the general structure of the x/bank Metadata, however `denom` is omitted, and injected with the correct string
#[cw_serde]
pub struct TokenMetadata {
    pub description: String,

    /// exponent represents power of 10 exponent that one must
    /// raise the base_denom to in order to equal the given DenomUnit's denom
    /// 1 denom = 1^exponent base_denom
    /// (e.g. with a base_denom of uruji, one can create a DenomUnit of 'ruji' with
    /// exponent = 6, thus: 1 ruji = 10^6 uruji).
    pub exponent: u8,

    /// display indicates the suggested denom that should be displayed in clients.
    pub display: String,

    /// name defines the name of the token (eg: ruji)
    pub name: String,

    /// symbol is the token symbol usually shown on exchanges (eg: RUJI). This can be the same as the display.
    pub symbol: String,

    /// URI to a document (on or off-chain) that contains additional information. Optional.
    pub uri: Option<String>,

    /// URIHash is a sha256 hash of a document pointed by URI. It's used to verify that the document didn't change. Optional.
    pub uri_hash: Option<String>,
}

impl TokenMetadata {
    fn to_sdk(&self, denom: String) -> DenomMetadata {
        let x = self.clone();
        DenomMetadata {
            description: x.description,
            denom_units: vec![DenomUnit {
                denom: denom.clone(),
                exponent: x.exponent.into(),
                aliases: vec![],
            }],
            base: denom,
            display: x.display,
            name: x.name,
            symbol: x.symbol,
            uri: x.uri.unwrap_or_default(),
            uri_hash: x.uri_hash.unwrap_or_default(),
        }
    }
}
