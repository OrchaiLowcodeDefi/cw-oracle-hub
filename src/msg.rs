use cosmwasm_schema::{cw_serde, schemars::Map, QueryResponses};
use cosmwasm_std::{Addr, Uint128};
use cw3::{DepositInfo, Status, UncheckedDepositInfo};
use cw4::MemberChangedHookMsg;
use cw_utils::{Duration, Expiration, Threshold, ThresholdResponse};

use crate::state::Data;

pub type VoteData = Map<String, Uint128>; // key: price

#[cw_serde]
pub struct InstantiateMsg {
    pub owner: String,
    // this is the group contract that contains the member list
    pub group_addr: String,
    pub threshold: Threshold,
    pub max_submitting_period: Duration,

    /// The cost of creating a proposal (if any).
    pub proposal_deposit: Option<UncheckedDepositInfo>,

    pub price_keys: Vec<String>,
    pub hook_contracts: Vec<Addr>,
}

// TODO: add some T variants? Maybe good enough as fixed Empty for now
#[cw_serde]
pub enum ExecuteMsg {
    Propose {
        data: VoteData,
        // note: we ignore API-spec'd earliest if passed, always opens immediately
        latest: Option<Expiration>,
    },
    Vote {
        proposal_id: u64,
        data: VoteData,
    },
    Close {
        proposal_id: u64,
    },
    /// Handles update hook messages from the group contract
    MemberChangedHook(MemberChangedHookMsg),
    UpdateConfig {
        owner: Option<String>,
        threshold: Option<Threshold>,
        max_submitting_period: Option<Duration>,
        price_keys: Option<Vec<String>>,
        hook_contracts: Option<Vec<Addr>>,
    },
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
    #[returns(Option<cw3::ProposalResponse>)]
    LastProposal {},
}

#[cw_serde]
pub struct VoteInfo {
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

#[cw_serde]
pub struct ProposalResponse {
    pub id: u64,
    pub title: String,
    pub updated_at: u64,
    pub description: String,
    pub votes: Vec<VoteInfo>,
    pub status: Status,
    pub expires: Expiration,
    /// This is the threshold that is applied to this proposal. Both
    /// the rules of the voting contract, as well as the total_weight
    /// of the voting group may have changed since this time. That
    /// means that the generic `Threshold{}` query does not provide
    /// valid information for existing proposals.
    pub threshold: ThresholdResponse,
    pub proposer: Addr,
    pub deposit: Option<DepositInfo>,
}

#[cw_serde]
pub struct ProposalListResponse {
    pub proposals: Vec<ProposalResponse>,
}
