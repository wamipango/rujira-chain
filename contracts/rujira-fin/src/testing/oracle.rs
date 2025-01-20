use cosmwasm_std::Decimal;
use rujira_rs::fin::OraclePrice;

pub struct TestOracle(pub Decimal);

impl OraclePrice for TestOracle {
    fn adjust(&self, bps: &i16) -> Decimal {
        match bps {
            i16::MIN..=-1 => self.0 * Decimal::from_ratio(10000u16 - bps.unsigned_abs(), 10000u16),
            0 => self.0,
            1..=i16::MAX => self.0 * Decimal::from_ratio(bps.unsigned_abs() + 10000u16, 10000u16),
        }
    }
}
