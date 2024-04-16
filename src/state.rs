use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, StdResult, Storage};
use cw3::{DepositInfo, Proposal};
use cw4::Cw4Contract;
use cw_storage_plus::{Item, Map};
use cw_utils::{Duration, Threshold};

use crate::msg::VoteData;

#[cw_serde]
pub struct Config {
    pub owner: Addr,
    pub threshold: Threshold,
    pub max_submitting_period: Duration,
    // Total weight and voters are queried from this contract
    pub group_addr: Cw4Contract,

    /// The price, if any, of creating a new proposal.
    pub proposal_deposit: Option<DepositInfo>,

    pub price_keys: Vec<String>,
    /// The contracts to be executed after by calling ExecuteMsg::AppendPrice { key, price, timestamp }
    pub hook_contracts: Vec<Addr>,
}

impl Config {
    pub fn verify_data(&self, data: &VoteData) -> bool {
        // different size
        if data.keys().len() != self.price_keys.len() {
            return false;
        }

        // not provide given key
        for key in &self.price_keys {
            if !data.contains_key(key) {
                return false;
            }
        }
        true
    }
}

#[cw_serde]
pub struct Data {
    pub weight: u64,
    pub data: VoteData,
}

pub const PROPOSAL_COUNT: Item<u64> = Item::new("proposal_count");

pub fn next_id(store: &mut dyn Storage) -> StdResult<u64> {
    let id: u64 = PROPOSAL_COUNT.may_load(store)?.unwrap_or_default() + 1;
    PROPOSAL_COUNT.save(store, &id)?;
    Ok(id)
}

pub fn last_id(store: &dyn Storage) -> StdResult<u64> {
    let id: u64 = PROPOSAL_COUNT.may_load(store)?.unwrap_or_default();

    Ok(id)
}

// unique items
pub const CONFIG: Item<Config> = Item::new("config");
pub const BALLOTS: Map<(u64, &Addr), Data> = Map::new("votes_v2");
pub const PROPOSALS: Map<u64, Proposal> = Map::new("proposals_v2");
