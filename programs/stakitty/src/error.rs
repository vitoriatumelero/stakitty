use anchor_lang::prelude::*;

#[error_code]
pub enum StakittyError {
    #[msg("Arithmetic overflow")]
    Overflow,
    #[msg("Amount must be greater than zero")]
    ZeroAmount,
    #[msg("Deposit is below the pool minimum")]
    DepositTooSmall,
    #[msg("Withdrawal exceeds the user's deposited principal")]
    InsufficientPrincipal,
    #[msg("Reserve does not hold enough liquid SOL for this withdrawal")]
    InsufficientLiquidity,
    #[msg("Caller does not own this user account")]
    Unauthorized,
    #[msg("Account is not an initialized vote account")]
    InvalidVoteAccount,
    #[msg("Signer is not the vote account's authorized withdrawer")]
    NotVoteWithdrawer,
    #[msg("Validator list is full")]
    ValidatorListFull,
    #[msg("Season is not open for sponsorship")]
    SeasonNotOpen,
    #[msg("Season has not reached its end epoch")]
    SeasonNotEnded,
    #[msg("Season is not the latest closed season")]
    SeasonNotClosed,
    #[msg("Season needs at least 4 sponsoring validators to close")]
    NotEnoughValidators,
    #[msg("Sponsorship accounts do not match the season totals")]
    SponsorshipMismatch,
    #[msg("Validator was already rebalanced for this season")]
    AlreadyRebalanced,
    #[msg("A transient stake is still pending; settle it after the epoch boundary")]
    StakeTransitionPending,
    #[msg("Stake account is deactivating; settle it after cooldown")]
    StakeNotSettled,
    #[msg("Nothing to settle yet")]
    NothingToSettle,
    #[msg("Account is not a delegated stake account")]
    InvalidStakeAccount,
    #[msg("Season length must be at least one epoch")]
    InvalidSeasonLength,
    #[msg("Round is not in the expected state")]
    InvalidRoundState,
    #[msg("Merkle total does not match the season's on-chain weight")]
    WeightMismatch,
    #[msg("Round has no weight to draw from")]
    NoParticipants,
    #[msg("Previous season account is required and must match")]
    MissingPreviousSeason,
    #[msg("Leaf does not contain the winning ticket")]
    NotWinningLeaf,
    #[msg("Merkle proof is invalid")]
    InvalidProof,
    #[msg("Prize claim window is still open")]
    ClaimWindowOpen,
    #[msg("Prize claim window has closed")]
    ClaimWindowClosed,
    #[msg("No realized yield to harvest")]
    NothingToHarvest,
    #[msg("Oracle queue is not the MagicBlock default queue")]
    InvalidOracleQueue,
}
