use anybuf::Anybuf;
use cosmwasm_std::{coin, Addr, AnyMsg, Coin, CosmosMsg, Env, Uint128};

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

    pub fn create_msg(&self) -> CosmosMsg {
        CosmosMsg::Any(AnyMsg {
            type_url: format!("{}MsgCreateDenom", TYPE_URL_PREFX),
            value: Anybuf::new()
                .append_string(1, self.env.contract.address.to_string())
                .append_string(2, self.denom())
                .as_bytes()
                .into(),
        })
    }

    pub fn mint_msg(&self, amount: Uint128, recipient: Addr) -> CosmosMsg {
        CosmosMsg::Any(AnyMsg {
            type_url: format!("{}MsgMint", TYPE_URL_PREFX),
            value: Anybuf::new()
                .append_string(1, self.env.contract.address.to_string())
                .append_message(2, &coin_message(coin(amount.u128(), self.denom())))
                .append_string(3, recipient.to_string())
                .as_bytes()
                .into(),
        })
    }

    pub fn burn_msg(&self, amount: Uint128) -> CosmosMsg {
        CosmosMsg::Any(AnyMsg {
            type_url: format!("{}MsgBurn", TYPE_URL_PREFX),
            value: Anybuf::new()
                .append_string(1, self.env.contract.address.to_string())
                .append_message(2, &coin_message(coin(amount.u128(), self.denom())))
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
