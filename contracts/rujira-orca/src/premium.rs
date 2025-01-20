use cosmwasm_std::Decimal;
use rujira_rs::fin::OraclePrice;

pub trait Premium {
    fn to_rate(&self, oracle: &impl OraclePrice) -> Decimal;
}

impl Premium for u8 {
    fn to_rate(&self, oracle: &impl OraclePrice) -> Decimal {
        oracle.adjust(&((*self as i16) * -100))
    }
}
