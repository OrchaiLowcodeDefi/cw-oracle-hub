use std::cmp::Ordering;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Addr, Api, Binary, BlockInfo, CosmosMsg, Deps, DepsMut, Empty, Env, MessageInfo,
    Order, QuerierWrapper, Response, StdResult, Storage, Uint128, WasmMsg,
};

use cw2::set_contract_version;

use cw3::{
    Proposal, ProposalListResponse, ProposalResponse, Status, Vote, VoterDetail, VoterListResponse,
    VoterResponse, Votes,
};

use cw4::{Cw4Contract, MemberChangedHookMsg, MemberDiff, MEMBERS_KEY};
use cw_storage_plus::{Bound, Map};
use cw_utils::{maybe_addr, Expiration, ThresholdResponse};

use crate::error::ContractError;
use crate::msg::{
    ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg, VoteInfo, VoteListResponse, VoteResponse,
};
use crate::state::{next_id, Config, Data, BALLOTS, CONFIG, PROPOSALS};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:cw-oracle-hub";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let group_addr = Cw4Contract(deps.api.addr_validate(&msg.group_addr).map_err(|_| {
        ContractError::InvalidGroup {
            addr: msg.group_addr.clone(),
        }
    })?);
    let total_weight = group_addr.total_weight(&deps.querier)?;
    msg.threshold.validate(total_weight)?;

    let proposal_deposit = msg
        .proposal_deposit
        .map(|deposit| deposit.into_checked(deps.as_ref()))
        .transpose()?;

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let cfg = Config {
        threshold: msg.threshold,
        max_submitting_period: msg.max_submitting_period,
        group_addr,
        executor: msg.executor,
        proposal_deposit,
        hook_contracts: msg.hook_contracts,
        price_key: msg.price_key,
    };
    CONFIG.save(deps.storage, &cfg)?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response<Empty>, ContractError> {
    match msg {
        ExecuteMsg::Propose { price, latest } => execute_propose(deps, env, info, price, latest),
        ExecuteMsg::Vote { proposal_id, price } => {
            execute_vote(deps, env, info, proposal_id, price)
        }
        ExecuteMsg::Execute { proposal_id } => execute_execute(deps, env, info, proposal_id),
        ExecuteMsg::Close { proposal_id } => execute_close(deps, env, info, proposal_id),
        ExecuteMsg::MemberChangedHook(MemberChangedHookMsg { diffs }) => {
            execute_membership_hook(deps, env, info, diffs)
        }
    }
}

pub fn execute_propose(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    price: Uint128,
    // we ignore earliest
    latest: Option<Expiration>,
) -> Result<Response<Empty>, ContractError> {
    // only members of the multisig can create a proposal
    let cfg = CONFIG.load(deps.storage)?;

    // Check that the native deposit was paid (as needed).
    if let Some(deposit) = cfg.proposal_deposit.as_ref() {
        deposit.check_native_deposit_paid(&info)?;
    }

    // Only members of the multisig can create a proposal
    // Non-voting members are special - they are allowed to create a proposal and
    // therefore "vote", but they aren't allowed to vote otherwise.
    // Such vote is also special, because despite having 0 weight it still counts when
    // counting threshold passing
    let vote_power = is_member(deps.storage, &deps.querier, deps.api, &info.sender, None)?
        .ok_or(ContractError::Unauthorized {})?;

    // max expires also used as default
    let max_expires = cfg.max_submitting_period.after(&env.block);
    let mut expires = latest.unwrap_or(max_expires);
    let comp = expires.partial_cmp(&max_expires);
    if let Some(Ordering::Greater) = comp {
        expires = max_expires;
    } else if comp.is_none() {
        return Err(ContractError::WrongExpiration {});
    }

    // Take the cw20 token deposit, if required. We do this before
    // creating the proposal struct below so that we can avoid a clone
    // and move the loaded deposit info into it.
    let take_deposit_msg = if let Some(deposit_info) = cfg.proposal_deposit.as_ref() {
        deposit_info.get_take_deposit_messages(&info.sender, &env.contract.address)?
    } else {
        vec![]
    };

    // create a proposal
    let mut prop = Proposal {
        title: "".to_string(),
        description: "".to_string(),
        start_height: env.block.height,
        msgs: vec![],
        expires,
        status: Status::Open,
        votes: Votes::yes(vote_power), // always vote yes
        threshold: cfg.threshold,
        total_weight: cfg.group_addr.total_weight(&deps.querier)?,
        proposer: info.sender.clone(),
        deposit: cfg.proposal_deposit,
    };
    prop.update_status(&env.block);
    let id = next_id(deps.storage)?;
    PROPOSALS.save(deps.storage, id, &prop)?;

    // add the first yes vote from voter
    let data = Data {
        weight: vote_power,
        price,
    };
    BALLOTS.save(deps.storage, (id, &info.sender), &data)?;

    Ok(Response::new()
        .add_messages(take_deposit_msg)
        .add_attribute("action", "propose")
        .add_attribute("sender", info.sender)
        .add_attribute("proposal_id", id.to_string())
        .add_attribute("status", format!("{:?}", prop.status)))
}

pub fn execute_vote(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
    price: Uint128,
) -> Result<Response<Empty>, ContractError> {
    // only members of the multisig can vote
    let cfg = CONFIG.load(deps.storage)?;

    // ensure proposal exists and can be voted on
    let mut prop = PROPOSALS.load(deps.storage, proposal_id)?;
    // Allow voting on Passed and Rejected proposals too,
    if ![Status::Open, Status::Passed, Status::Rejected].contains(&prop.status) {
        return Err(ContractError::NotOpen {});
    }
    // if they are not expired
    if prop.expires.is_expired(&env.block) {
        return Err(ContractError::Expired {});
    }

    // Only members of the multisig can submit
    // Additional check if weight >= 1
    // use a snapshot of "start of proposal"
    let vote_power = cfg
        .group_addr
        .is_voting_member(&deps.querier, &info.sender, prop.start_height)?
        .ok_or(ContractError::Unauthorized {})?;

    // cast vote if no vote previously cast
    BALLOTS.update(deps.storage, (proposal_id, &info.sender), |bal| match bal {
        Some(_) => Err(ContractError::AlreadyVoted {}),
        None => Ok(Data {
            weight: vote_power,
            price,
        }),
    })?;

    // update vote tally
    prop.votes.add_vote(Vote::Yes, vote_power);
    prop.update_status(&env.block);
    PROPOSALS.save(deps.storage, proposal_id, &prop)?;

    Ok(Response::new()
        .add_attribute("action", "vote")
        .add_attribute("sender", info.sender)
        .add_attribute("proposal_id", proposal_id.to_string())
        .add_attribute("status", format!("{:?}", prop.status)))
}

pub fn execute_execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
) -> Result<Response, ContractError> {
    let mut prop = PROPOSALS.load(deps.storage, proposal_id)?;
    // we allow execution even after the proposal "expiration" as long as all vote come in before
    // that point. If it was approved on time, it can be executed any time.
    prop.update_status(&env.block);
    if prop.status != Status::Passed {
        return Err(ContractError::WrongExecuteStatus {});
    }

    let cfg = CONFIG.load(deps.storage)?;
    cfg.authorize(&deps.querier, &info.sender)?;

    // set it to executed
    prop.status = Status::Executed;

    // get price by using median
    let prices = BALLOTS
        .prefix(proposal_id)
        .range(deps.storage, None, None, Order::Ascending)
        .map(|item| Ok(item?.1.price))
        .collect::<StdResult<Vec<_>>>()?;

    let mid = prices.len() / 2;
    let median_price = prices[mid];

    // now create message for props.msgs and update it
    prop.msgs = cfg
        .hook_contracts
        .iter()
        .map(|addr| {
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: addr.to_string(),
                funds: vec![],
                msg: Binary::from(
                    format!(
                        r#"{{"append_price":{{"key":"{}","price":"{}","timestamp":"{}"}}}}"#,
                        cfg.price_key,
                        median_price,
                        env.block.time.seconds()
                    )
                    .as_bytes(),
                ),
            })
        })
        .collect();

    PROPOSALS.save(deps.storage, proposal_id, &prop)?;

    // Unconditionally refund here.
    let response = match prop.deposit {
        Some(deposit) => {
            Response::new().add_message(deposit.get_return_deposit_message(&prop.proposer)?)
        }
        None => Response::new(),
    };

    // dispatch all proposed messages
    Ok(response
        .add_messages(prop.msgs)
        .add_attribute("action", "execute")
        .add_attribute("sender", info.sender)
        .add_attribute("proposal_id", proposal_id.to_string()))
}

pub fn execute_close(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
) -> Result<Response<Empty>, ContractError> {
    // anyone can trigger this if the vote passed

    let mut prop = PROPOSALS.load(deps.storage, proposal_id)?;
    if [Status::Executed, Status::Rejected, Status::Passed].contains(&prop.status) {
        return Err(ContractError::WrongCloseStatus {});
    }
    // Avoid closing of Passed due to expiration proposals
    if prop.current_status(&env.block) == Status::Passed {
        return Err(ContractError::WrongCloseStatus {});
    }
    if !prop.expires.is_expired(&env.block) {
        return Err(ContractError::NotExpired {});
    }

    // set it to failed
    prop.status = Status::Rejected;
    PROPOSALS.save(deps.storage, proposal_id, &prop)?;

    // Refund the deposit if we have been configured to do so.
    let mut response = Response::new();
    if let Some(deposit) = prop.deposit {
        if deposit.refund_failed_proposals {
            response = response.add_message(deposit.get_return_deposit_message(&prop.proposer)?)
        }
    }

    Ok(response
        .add_attribute("action", "close")
        .add_attribute("sender", info.sender)
        .add_attribute("proposal_id", proposal_id.to_string()))
}

pub fn execute_membership_hook(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    _diffs: Vec<MemberDiff>,
) -> Result<Response<Empty>, ContractError> {
    // This is now a no-op
    // But we leave the authorization check as a demo
    let cfg = CONFIG.load(deps.storage)?;
    if info.sender != cfg.group_addr.0 {
        return Err(ContractError::Unauthorized {});
    }

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Threshold {} => to_binary(&query_threshold(deps)?),
        QueryMsg::Proposal { proposal_id } => to_binary(&query_proposal(deps, env, proposal_id)?),
        QueryMsg::Vote { proposal_id, voter } => to_binary(&query_vote(deps, proposal_id, voter)?),
        QueryMsg::ListProposals { start_after, limit } => {
            to_binary(&list_proposals(deps, env, start_after, limit)?)
        }
        QueryMsg::ReverseProposals {
            start_before,
            limit,
        } => to_binary(&reverse_proposals(deps, env, start_before, limit)?),
        QueryMsg::ListVotes {
            proposal_id,
            start_after,
            limit,
        } => to_binary(&list_votes(deps, proposal_id, start_after, limit)?),
        QueryMsg::Voter { address } => to_binary(&query_voter(deps, address)?),
        QueryMsg::ListVoters { start_after, limit } => {
            to_binary(&list_voters(deps, start_after, limit)?)
        }
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
    }
}

fn query_threshold(deps: Deps) -> StdResult<ThresholdResponse> {
    let cfg = CONFIG.load(deps.storage)?;
    let total_weight = cfg.group_addr.total_weight(&deps.querier)?;
    Ok(cfg.threshold.to_response(total_weight))
}

fn query_config(deps: Deps) -> StdResult<Config> {
    CONFIG.load(deps.storage)
}

fn query_proposal(deps: Deps, env: Env, id: u64) -> StdResult<ProposalResponse> {
    let prop = PROPOSALS.load(deps.storage, id)?;
    let status = prop.current_status(&env.block);
    let threshold = prop.threshold.to_response(prop.total_weight);
    Ok(ProposalResponse {
        id,
        title: prop.title,
        description: prop.description,
        msgs: prop.msgs,
        status,
        expires: prop.expires,
        proposer: prop.proposer,
        deposit: prop.deposit,
        threshold,
    })
}

// settings for pagination
const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;

fn list_proposals(
    deps: Deps,
    env: Env,
    start_after: Option<u64>,
    limit: Option<u32>,
) -> StdResult<ProposalListResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = start_after.map(Bound::exclusive);
    let proposals = PROPOSALS
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|p| map_proposal(&env.block, p))
        .collect::<StdResult<_>>()?;

    Ok(ProposalListResponse { proposals })
}

fn reverse_proposals(
    deps: Deps,
    env: Env,
    start_before: Option<u64>,
    limit: Option<u32>,
) -> StdResult<ProposalListResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let end = start_before.map(Bound::exclusive);
    let props: StdResult<Vec<_>> = PROPOSALS
        .range(deps.storage, None, end, Order::Descending)
        .take(limit)
        .map(|p| map_proposal(&env.block, p))
        .collect();

    Ok(ProposalListResponse { proposals: props? })
}

fn map_proposal(
    block: &BlockInfo,
    item: StdResult<(u64, Proposal)>,
) -> StdResult<ProposalResponse> {
    item.map(|(id, prop)| {
        let status = prop.current_status(block);
        let threshold = prop.threshold.to_response(prop.total_weight);
        ProposalResponse {
            id,
            title: prop.title,
            description: prop.description,
            msgs: prop.msgs,
            status,
            expires: prop.expires,
            deposit: prop.deposit,
            proposer: prop.proposer,
            threshold,
        }
    })
}

fn query_vote(deps: Deps, proposal_id: u64, voter: String) -> StdResult<VoteResponse> {
    let voter_addr = deps.api.addr_validate(&voter)?;
    let prop = BALLOTS.may_load(deps.storage, (proposal_id, &voter_addr))?;
    let vote = prop.map(|b| VoteInfo {
        proposal_id,
        voter,
        data: b,
    });
    Ok(VoteResponse { vote })
}

fn list_votes(
    deps: Deps,
    proposal_id: u64,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<VoteListResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let addr = maybe_addr(deps.api, start_after)?;
    let start = addr.as_ref().map(Bound::exclusive);

    let votes = BALLOTS
        .prefix(proposal_id)
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            item.map(|(addr, data)| VoteInfo {
                proposal_id,
                voter: addr.into(),
                data,
            })
        })
        .collect::<StdResult<_>>()?;

    Ok(VoteListResponse { votes })
}

/// Check if this address is a member and returns its weight.
/// We dont use the group addr's is_member function because it queries using the key as &Addr, not Vec<u8> of CannonicalAddr in the latest version
/// The current production group addr on Oraichain is using the v0.13.2 version of CosmWasm, which uses CannonicalAddr
fn is_member(
    storage: &dyn Storage,
    querier: &QuerierWrapper,
    api: &dyn Api,
    member: &Addr,
    height: Option<u64>,
) -> StdResult<Option<u64>> {
    let cfg = CONFIG.load(storage)?;
    let mut old_ver_height = match height {
        Some(height) => cfg
            .group_addr
            .member_at_height(querier, member.to_string(), height.into()),
        None => Map::new(MEMBERS_KEY).query(
            querier,
            cfg.group_addr.addr(),
            api.addr_canonicalize(member.as_str())?.to_vec(),
        ),
    }?;
    // if None then we try to query using the new way
    if old_ver_height.is_none() {
        old_ver_height = Map::new(MEMBERS_KEY).query(querier, cfg.group_addr.addr(), member)?;
    }
    Ok(old_ver_height)
}

fn query_voter(deps: Deps, voter: String) -> StdResult<VoterResponse> {
    let voter_addr = deps.api.addr_validate(&voter)?;
    let weight = is_member(deps.storage, &deps.querier, deps.api, &voter_addr, None)?;

    Ok(VoterResponse { weight })
}

fn list_voters(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<VoterListResponse> {
    let cfg = CONFIG.load(deps.storage)?;
    let voters = cfg
        .group_addr
        .list_members(&deps.querier, start_after, limit)?
        .into_iter()
        .map(|member| VoterDetail {
            addr: member.addr,
            weight: member.weight,
        })
        .collect();
    Ok(VoterListResponse { voters })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    Ok(Response::default())
}
