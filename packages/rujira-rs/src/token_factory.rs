use anybuf::Anybuf;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, AnyMsg, Coin, CosmosMsg, Env, Uint128};

pub struct TokenFactory {
    nonce: String,
    env: Env,
}

static TYPE_URL_PREFX: &str = "/types.";

impl TokenFactory {
    pub fn new(env: &Env, nonce: &str) -> Self {
        Self {
            env: env.clone(),
            nonce: nonce.to_string(),
        }
    }

    pub fn denom(&self) -> String {
        format!("factory/{}/{}", self.env.contract.address, self.nonce)
    }

    pub fn create_msg(&self, metadata: TokenMetadata) -> CosmosMsg {
        CosmosMsg::Any(AnyMsg {
            type_url: format!("{}MsgCreateDenom", TYPE_URL_PREFX),
            value: Anybuf::new()
                .append_string(1, &self.env.contract.address)
                .append_string(2, &self.nonce)
                .append_message(3, &metadata.to_message(self.denom()))
                .as_bytes()
                .into(),
        })
    }

    pub fn mint_msg(&self, amount: Uint128, recipient: Addr) -> CosmosMsg {
        CosmosMsg::Any(AnyMsg {
            type_url: format!("{}MsgMint", TYPE_URL_PREFX),
            value: Anybuf::new()
                .append_string(1, &self.env.contract.address)
                .append_message(2, &coin_message(Coin::new(amount, self.denom())))
                .append_string(3, &recipient)
                .as_bytes()
                .into(),
        })
    }

    pub fn burn_msg(&self, amount: Uint128) -> CosmosMsg {
        CosmosMsg::Any(AnyMsg {
            type_url: format!("{}MsgBurn", TYPE_URL_PREFX),
            value: Anybuf::new()
                .append_string(1, &self.env.contract.address)
                .append_message(2, &coin_message(Coin::new(amount, self.denom())))
                .as_bytes()
                .into(),
        })
    }
}

fn coin_message(coin: Coin) -> Anybuf {
    Anybuf::new()
        .append_string(1, coin.denom)
        .append_string(2, coin.amount.to_string())
}

/// Metadata represents a struct that describes a basic token.
///
/// It follows the general structure of the x/bank Metadata, however `denom` is omitted, and injected with the correct string
#[cw_serde]
pub struct TokenMetadata {
    pub description: String,

    /// denom_units represents the of DenomUnit for a given coin
    pub exponent: u8,

    /// exponent represents power of 10 exponent that one must
    /// raise the base_denom to in order to equal the given DenomUnit's denom
    /// 1 denom = 1^exponent base_denom
    /// (e.g. with a base_denom of uruji, one can create a DenomUnit of 'ruji' with
    /// exponent = 6, thus: 1 ruji = 10^6 uruji).
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
    fn to_message(&self, denom: String) -> Anybuf {
        let x = self.clone();
        let denom_unit = Anybuf::new()
            .append_string(1, denom.clone())
            .append_uint32(2, self.exponent.into())
            .append_repeated_string(3, &[self.name.clone()]);

        Anybuf::new()
            .append_string(1, x.description)
            .append_repeated_message(2, &[denom_unit])
            .append_string(3, denom)
            .append_string(4, x.display)
            .append_string(5, x.name)
            .append_string(6, x.symbol)
            .append_string(7, x.uri.unwrap_or_default())
            .append_string(8, x.uri_hash.unwrap_or_default())
    }
}
