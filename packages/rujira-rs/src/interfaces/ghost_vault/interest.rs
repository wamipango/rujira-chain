use cosmwasm_schema::cw_serde;
use cosmwasm_std::{ensure, Decimal, StdError, StdResult};
use std::ops::{Add, Mul, Sub};

#[cw_serde]
pub struct Interest {
    pub target_utilization: Decimal,
    // The base rate charged when utilization = 0
    pub base_rate: Decimal,
    // The rate at which the interest rate increases when utilization < target_utilization
    pub slope1: Decimal,
    // The rate at which the interest rate increases when utilization > target_utilization
    pub slope2: Decimal,
}

impl Interest {
    pub fn validate(&self) -> StdResult<()> {
        ensure!(
            self.base_rate.lt(&Decimal::one()),
            StdError::generic_err("base_rate must be < 1".to_string())
        );
        ensure!(
            self.slope1.lt(&Decimal::one()),
            StdError::generic_err("slope1 must be < 1".to_string())
        );
        ensure!(
            self.slope2.lt(&Decimal::one()),
            StdError::generic_err("slope2 must be < 1".to_string())
        );
        ensure!(
            self.target_utilization.lt(&Decimal::one()),
            StdError::generic_err("target_utilization must be < 1".to_string())
        );
        Ok(())
    }

    pub fn rate(&self, utilization: Decimal) -> StdResult<Decimal> {
        ensure!(
            utilization.le(&Decimal::one()),
            StdError::generic_err("utilization must be <= 1".to_string())
        );

        if utilization.le(&self.target_utilization) {
            Ok(self.base_rate.add(self.slope1.mul(utilization)))
        } else {
            Ok(self
                .base_rate
                .add(self.slope1.mul(self.target_utilization))
                .add(self.slope2.mul(utilization.sub(self.target_utilization))))
        }
    }
}
