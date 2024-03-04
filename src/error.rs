use cosmwasm_std::StdError;
use cw3::DepositError;
use cw_utils::{PaymentError, ThresholdError};

use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    Threshold(#[from] ThresholdError),

    #[error("Group contract invalid address '{addr}'")]
    InvalidGroup { addr: String },

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Proposal is not open")]
    NotOpen {},

    #[error("Proposal voting period has expired")]
    Expired {},

    #[error("Proposal must expire before you can close it")]
    NotExpired {},

    #[error("Wrong expiration option")]
    WrongExpiration {},

    #[error("Wrong vote data")]
    WrongVoteData {},

    #[error("Already voted on this proposal")]
    AlreadyVoted {},

    #[error("Cannot close completed or passed proposals")]
    WrongCloseStatus {},

    #[error("Last proposal must have been executed before you can propose")]
    CanNotPropose {},

    #[error("{0}")]
    Payment(#[from] PaymentError),

    #[error("{0}")]
    Deposit(#[from] DepositError),
}
