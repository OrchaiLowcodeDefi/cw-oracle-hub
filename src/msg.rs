use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Uint128};
use cw3::UncheckedDepositInfo;
use cw4::MemberChangedHookMsg;
use cw_utils::{Duration, Expiration, Threshold};

use crate::state::{Data, Executor};

#[cw_serde]
pub struct InstantiateMsg {
    // this is the group contract that contains the member list
    pub group_addr: String,
    pub threshold: Threshold,
    pub max_submitting_period: Duration,
    // who is able to execute aggregated result
    // None means that anyone can execute
    pub executor: Option<Executor>,
    /// The cost of creating a proposal (if any).
    pub proposal_deposit: Option<UncheckedDepositInfo>,

    pub price_key: String,
    pub hook_contracts: Vec<Addr>,
}

// TODO: add some T variants? Maybe good enough as fixed Empty for now
#[cw_serde]
pub enum ExecuteMsg {
    Propose {
        price: Uint128,
        // note: we ignore API-spec'd earliest if passed, always opens immediately
        latest: Option<Expiration>,
    },
    Vote {
        proposal_id: u64,
        price: Uint128,
    },
    Close {
        proposal_id: u64,
    },
    /// Handles update hook messages from the group contract
    MemberChangedHook(MemberChangedHookMsg),
}

#[cw_serde]
pub struct MigrateMsg {}

// We can also add this as a cw3 extension
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(cw_utils::ThresholdResponse)]
    Threshold {},
    #[returns(cw3::ProposalResponse)]
    Proposal { proposal_id: u64 },
    #[returns(cw3::ProposalListResponse)]
    ListProposals {
        start_after: Option<u64>,
        limit: Option<u32>,
    },
    #[returns(cw3::ProposalListResponse)]
    ReverseProposals {
        start_before: Option<u64>,
        limit: Option<u32>,
    },
    #[returns(VoteResponse)]
    Vote { proposal_id: u64, voter: String },
    #[returns(cw3::VoteListResponse)]
    ListVotes {
        proposal_id: u64,
        start_after: Option<String>,
        limit: Option<u32>,
    },
    #[returns(cw3::VoterResponse)]
    Voter { address: String },
    #[returns(cw3::VoterListResponse)]
    ListVoters {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Gets the current configuration.
    #[returns(crate::state::Config)]
    Config {},
}

#[cw_serde]
pub struct VoteInfo {
    pub proposal_id: u64,
    pub voter: String,
    pub data: Data,
}

#[cw_serde]
pub struct VoteResponse {
    pub vote: Option<VoteInfo>,
}

#[cw_serde]
pub struct VoteListResponse {
    pub votes: Vec<VoteInfo>,
}
