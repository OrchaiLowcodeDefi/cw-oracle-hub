# CW Oracle Hub

The CW Oracle Hub contains the logic for oracle price feeding.

## CosmWasm Integration

To invoke the pricefeed contract at the end of each round, set the cw-oracle-hub address as the admin and add the following code to your contract:

```rust
// msg.rs
#[cw_serde]
pub enum SudoMsg {
    AppendPrice {
        key: String,
        price: Uint128,
        timestamp: u64,
     },
}

// contract.rs
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, _env: Env, msg: SudoMsg) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::AppendPrice {
            key,
            price,
            timestamp,
        } => append_price(deps, info, key, price, timestamp),
    }
}
```
