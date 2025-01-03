use cosmwasm_std::Event;

/// Utility function to create consistent contract event naming
pub fn new_event(name: &str) -> Event {
    let pkg = env!("CARGO_PKG_NAME");
    Event::new(format!("{pkg}/{name}"))
}
