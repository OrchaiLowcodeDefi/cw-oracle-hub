use cosmwasm_std::Coin;
use cw3::{ProposalResponse, Status};
use cw_utils::{Duration, Threshold};
use osmosis_test_tube::{Module, OraichainTestApp, Wasm};
use test_tube::{Account, SigningAccount};

use crate::{
    msg::{ExecuteMsg, InstantiateMsg, QueryMsg},
    state::Executor,
};

const CW4_GROUP_WASM_BYTES: &[u8] = include_bytes!("../testdata/cw4-group.wasm");
const ORACLE_HUB_WASM_BYTES: &[u8] = include_bytes!("../testdata/cw-oracle-hub.wasm");

fn init_app() -> (OraichainTestApp, Vec<SigningAccount>, String) {
    let app = OraichainTestApp::default();
    let accounts = app
        .init_accounts(&[Coin::new(5_000_000_000_000u128, "orai")], 4)
        .unwrap();

    let (owner, member1, member2, member3) =
        (&accounts[0], &accounts[1], &accounts[2], &accounts[3]);
    let wasm = Wasm::new(&app);

    let cw4_code_id = wasm
        .store_code(CW4_GROUP_WASM_BYTES, None, owner)
        .unwrap()
        .data
        .code_id;

    let cw4_group_addr = wasm
        .instantiate(
            cw4_code_id,
            &cw4_group::msg::InstantiateMsg {
                admin: Some(owner.address()),
                members: vec![
                    cw4::Member {
                        addr: owner.address(),
                        weight: 1,
                    },
                    cw4::Member {
                        addr: member1.address(),
                        weight: 1,
                    },
                    cw4::Member {
                        addr: member2.address(),
                        weight: 1,
                    },
                    cw4::Member {
                        addr: member3.address(),
                        weight: 1,
                    },
                ],
            },
            Some(&owner.address()),
            Some("group-4"),
            &[],
            owner,
        )
        .unwrap()
        .data
        .address;

    let oracle_hub_code_id = wasm
        .store_code(ORACLE_HUB_WASM_BYTES, None, owner)
        .unwrap()
        .data
        .code_id;

    let cw_oracle_hub_addr = wasm
        .instantiate(
            oracle_hub_code_id,
            &InstantiateMsg {
                group_addr: cw4_group_addr.clone(),
                threshold: Threshold::AbsoluteCount { weight: 3 },
                max_submitting_period: Duration::Time(3600),
                executor: Some(Executor::Member),
                proposal_deposit: None,
                price_key: "orai".to_string(),
                hook_contracts: vec![],
            },
            Some(&owner.address()),
            Some("oracle-hub"),
            &[],
            owner,
        )
        .unwrap()
        .data
        .address;

    (app, accounts, cw_oracle_hub_addr)
}

#[test]
fn update_price_feed() {
    let (app, accounts, cw_oracle_hub_addr) = init_app();

    let wasm = Wasm::new(&app);

    // create first round then  update
    let (member0, member1, member2) = (&accounts[0], &accounts[1], &accounts[2]);

    // first user propose
    let proposal_id = u64::from_str_radix(
        &wasm
            .execute(
                &cw_oracle_hub_addr,
                &ExecuteMsg::Propose {
                    price: 11_000_000u128.into(),
                    latest: None,
                },
                &[],
                member0,
            )
            .unwrap()
            .events
            .iter()
            .find(|e| e.ty == "wasm")
            .unwrap()
            .attributes
            .iter()
            .find(|a| a.key == "proposal_id")
            .unwrap()
            .value,
        10,
    )
    .unwrap();

    // second user vote
    wasm.execute(
        &cw_oracle_hub_addr,
        &ExecuteMsg::Vote {
            proposal_id,
            price: 11_000_000u128.into(),
        },
        &[],
        member1,
    )
    .unwrap();

    // third user vote, should pass and execute
    wasm.execute(
        &cw_oracle_hub_addr,
        &ExecuteMsg::Vote {
            proposal_id,
            price: 11_000_000u128.into(),
        },
        &[],
        member2,
    )
    .unwrap();

    let proposal: ProposalResponse = wasm
        .query(&cw_oracle_hub_addr, &QueryMsg::Proposal { proposal_id })
        .unwrap();

    assert_eq!(proposal.status, Status::Executed);
}
