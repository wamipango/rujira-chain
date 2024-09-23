use anybuf::Anybuf;
use cosmwasm_std::{Addr, AnyMsg, Coin, CosmosMsg, DenomMetadata, DenomUnit};

static TYPE_URL_PREFX: &str = "/types.";

pub(crate) struct MsgCreateDenom {
    pub sender: Addr,
    pub nonce: String,
    pub metadata: DenomMetadata,
}

pub(crate) struct MsgMint {
    pub sender: Addr,
    pub amount: Coin,
    pub recipient: Addr,
}

pub(crate) struct MsgBurn {
    pub sender: Addr,
    pub amount: Coin,
}

pub(crate) struct MsgSetAdmin {
    pub sender: Addr,
    pub denom: String,
    pub new_admin: Addr,
}

pub(crate) struct MsgSetMetadata {
    pub sender: Addr,
    pub denom: String,
    pub new_metadata: DenomMetadata,
}

impl From<MsgCreateDenom> for CosmosMsg {
    fn from(value: MsgCreateDenom) -> Self {
        CosmosMsg::Any(AnyMsg {
            type_url: format!("{}MsgCreateDenom", TYPE_URL_PREFX),
            value: Anybuf::new()
                .append_string(1, &value.sender)
                .append_string(2, &value.nonce)
                .append_message(3, &metadata_message(value.metadata))
                .as_bytes()
                .into(),
        })
    }
}

impl From<MsgMint> for CosmosMsg {
    fn from(value: MsgMint) -> Self {
        CosmosMsg::Any(AnyMsg {
            type_url: format!("{}MsgMint", TYPE_URL_PREFX),
            value: Anybuf::new()
                .append_string(1, &value.sender)
                .append_message(2, &coin_message(value.amount))
                .append_string(3, &value.recipient)
                .as_bytes()
                .into(),
        })
    }
}

impl From<MsgBurn> for CosmosMsg {
    fn from(value: MsgBurn) -> Self {
        CosmosMsg::Any(AnyMsg {
            type_url: format!("{}MsgBurn", TYPE_URL_PREFX),
            value: Anybuf::new()
                .append_string(1, &value.sender)
                .append_message(2, &coin_message(value.amount))
                .as_bytes()
                .into(),
        })
    }
}

impl From<MsgSetAdmin> for CosmosMsg {
    fn from(value: MsgSetAdmin) -> Self {
        CosmosMsg::Any(AnyMsg {
            type_url: format!("{}MsgSetAdmin", TYPE_URL_PREFX),
            value: Anybuf::new()
                .append_string(1, &value.sender)
                .append_string(2, &value.denom)
                .append_string(3, &value.new_admin)
                .as_bytes()
                .into(),
        })
    }
}

impl From<MsgSetMetadata> for CosmosMsg {
    fn from(value: MsgSetMetadata) -> Self {
        CosmosMsg::Any(AnyMsg {
            type_url: format!("{}MsgSetMetadata", TYPE_URL_PREFX),
            value: Anybuf::new()
                .append_string(1, &value.sender)
                .append_string(2, &value.denom)
                .append_message(3, &metadata_message(value.new_metadata))
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

fn metadata_message(value: DenomMetadata) -> Anybuf {
    let denom_units: Vec<Anybuf> = value.denom_units.iter().map(denom_unit_message).collect();

    Anybuf::new()
        .append_string(1, value.description)
        .append_repeated_message(2, &denom_units)
        .append_string(3, value.base)
        .append_string(4, value.display)
        .append_string(5, value.name)
        .append_string(6, value.symbol)
        .append_string(7, value.uri)
        .append_string(8, value.uri_hash)
}

fn denom_unit_message(value: &DenomUnit) -> Anybuf {
    Anybuf::new()
        .append_string(1, value.denom.clone())
        .append_uint32(2, value.exponent.clone())
        .append_repeated_string(3, &value.aliases)
}
