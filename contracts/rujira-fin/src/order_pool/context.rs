use cosmwasm_std::{ensure, Response, Storage};
use rujira_rs::exchange::{SwapContext, SwapError};

use super::Pool;

#[derive(Default, Clone, Debug)]
pub struct PoolContext {
    pub pools: Vec<Pool>,
    comitted: bool,
}

impl SwapContext for PoolContext {
    fn commit(
        &mut self,
        storage: &mut dyn Storage,
        response: Response,
    ) -> Result<Response, SwapError> {
        ensure!(!self.comitted, SwapError::AlreadyComitted {});
        self.comitted = true;

        for mut pool in self.pools.clone() {
            pool.commit(storage)?;
        }
        Ok(response)
    }
}
