use cosmwasm_std::Env;

pub struct TokenFactory {
    env: Env,
}

impl TokenFactory {
    pub fn new(env: &Env) -> Self {
        Self { env: env.clone() }
    }

    pub fn denom(&self, nonce: &str) -> String {
        format!("factory/{}/{}", self.env.contract.address, nonce)
    }
}
