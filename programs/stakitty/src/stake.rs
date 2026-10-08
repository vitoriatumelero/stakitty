//! Hand-encoded Stake program CPIs and the account-layout reads they depend on.
//! Encoding is bincode: a u32 LE variant tag followed by fixed-size arguments.

use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::{get_return_data, invoke, invoke_signed},
};

use crate::{constants::*, error::StakittyError};

pub const STAKE_ACCOUNT_SPACE: u64 = 200;

const IX_INITIALIZE: u32 = 0;
const IX_DELEGATE: u32 = 2;
const IX_SPLIT: u32 = 3;
const IX_WITHDRAW: u32 = 4;
const IX_DEACTIVATE: u32 = 5;
const IX_MERGE: u32 = 7;
const IX_GET_MINIMUM_DELEGATION: u32 = 13;

const STATE_TAG_STAKE: u32 = 2;
// StakeStateV2::Stake = tag(4) + Meta(120) + Delegation{voter(32), stake(8), activation, deactivation}.
const ACTIVATION_EPOCH_OFFSET: usize = 164;
const DEACTIVATION_EPOCH_OFFSET: usize = 172;

/// Vote state versions 1 (1.14.11), 2 (V3) and 3 (V4) all start with node_pubkey then
/// authorized_withdrawer. Version 0 (0.23.5) uses another layout and is rejected.
const VOTE_WITHDRAWER_OFFSET: usize = 4 + 32;

pub struct StakeInfo {
    pub lamports: u64,
    pub activation_epoch: u64,
    pub deactivation_epoch: u64,
}

impl StakeInfo {
    pub fn is_deactivating(&self) -> bool {
        self.deactivation_epoch != u64::MAX
    }
}

pub fn vote_withdrawer(vote: &AccountInfo) -> Result<Pubkey> {
    require_keys_eq!(
        *vote.owner,
        VOTE_PROGRAM_ID,
        StakittyError::InvalidVoteAccount
    );
    let data = vote.try_borrow_data()?;
    let version = data
        .get(0..4)
        .and_then(|b| b.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or(StakittyError::InvalidVoteAccount)?;
    require!(
        (1..=3).contains(&version),
        StakittyError::InvalidVoteAccount
    );
    let withdrawer = data
        .get(VOTE_WITHDRAWER_OFFSET..VOTE_WITHDRAWER_OFFSET + 32)
        .ok_or(StakittyError::InvalidVoteAccount)?;
    Pubkey::try_from(withdrawer).map_err(|_| StakittyError::InvalidVoteAccount.into())
}

/// `None` when the PDA is not (yet) a stake account; donated lamports on it are absorbed later.
pub fn read_stake(account: &AccountInfo) -> Result<Option<StakeInfo>> {
    if *account.owner != STAKE_PROGRAM_ID || account.lamports() == 0 {
        return Ok(None);
    }
    let data = account.try_borrow_data()?;
    let read_u64 = |offset: usize| -> Result<u64> {
        data.get(offset..offset + 8)
            .and_then(|b| b.try_into().ok())
            .map(u64::from_le_bytes)
            .ok_or(StakittyError::InvalidStakeAccount.into())
    };
    let tag = data
        .get(0..4)
        .and_then(|b| b.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or(StakittyError::InvalidStakeAccount)?;
    require!(tag == STATE_TAG_STAKE, StakittyError::InvalidStakeAccount);
    Ok(Some(StakeInfo {
        lamports: account.lamports(),
        activation_epoch: read_u64(ACTIVATION_EPOCH_OFFSET)?,
        deactivation_epoch: read_u64(DEACTIVATION_EPOCH_OFFSET)?,
    }))
}

pub fn minimum_delegation() -> Result<u64> {
    let ix = Instruction {
        program_id: STAKE_PROGRAM_ID,
        accounts: vec![],
        data: IX_GET_MINIMUM_DELEGATION.to_le_bytes().to_vec(),
    };
    invoke(&ix, &[])?;
    let (program, data) = get_return_data().ok_or(StakittyError::InvalidStakeAccount)?;
    require_keys_eq!(
        program,
        STAKE_PROGRAM_ID,
        StakittyError::InvalidStakeAccount
    );
    data.get(0..8)
        .and_then(|b| b.try_into().ok())
        .map(u64::from_le_bytes)
        .ok_or(StakittyError::InvalidStakeAccount.into())
}

/// Accounts every stake CPI below draws from. The reserve PDA is staker, withdrawer and funder.
pub struct StakeCtx<'a, 'info> {
    pub reserve: &'a AccountInfo<'info>,
    pub reserve_seeds: &'a [&'a [u8]],
    pub clock: &'a AccountInfo<'info>,
    pub stake_history: &'a AccountInfo<'info>,
    pub system_program: &'a AccountInfo<'info>,
}

impl<'info> StakeCtx<'_, 'info> {
    /// Turns a system-owned PDA into an uninitialized, stake-owned account holding `lamports`
    /// in total (a valid split destination). Uses transfer + allocate + assign instead of
    /// create_account, so lamports donated to the PDA beforehand cannot block creation.
    pub fn allocate(
        &self,
        stake: &AccountInfo<'info>,
        stake_seeds: &[&[u8]],
        lamports: u64,
    ) -> Result<()> {
        let missing = lamports.saturating_sub(stake.lamports());
        if missing > 0 {
            anchor_lang::system_program::transfer(
                CpiContext::new_with_signer(
                    self.system_program.key(),
                    anchor_lang::system_program::Transfer {
                        from: self.reserve.clone(),
                        to: stake.clone(),
                    },
                    &[self.reserve_seeds],
                ),
                missing,
            )?;
        }
        anchor_lang::system_program::allocate(
            CpiContext::new_with_signer(
                self.system_program.key(),
                anchor_lang::system_program::Allocate {
                    account_to_allocate: stake.clone(),
                },
                &[stake_seeds],
            ),
            STAKE_ACCOUNT_SPACE,
        )?;
        anchor_lang::system_program::assign(
            CpiContext::new_with_signer(
                self.system_program.key(),
                anchor_lang::system_program::Assign {
                    account_to_assign: stake.clone(),
                },
                &[stake_seeds],
            ),
            &STAKE_PROGRAM_ID,
        )?;
        Ok(())
    }

    /// `allocate` + Initialize with the reserve as staker and withdrawer, no lockup.
    pub fn create(
        &self,
        stake: &AccountInfo<'info>,
        stake_seeds: &[&[u8]],
        lamports: u64,
        rent_sysvar: &AccountInfo<'info>,
    ) -> Result<()> {
        self.allocate(stake, stake_seeds, lamports)?;
        let authority = self.reserve.key();
        let mut data = Vec::with_capacity(4 + 64 + 48);
        data.extend_from_slice(&IX_INITIALIZE.to_le_bytes());
        data.extend_from_slice(authority.as_ref()); // staker
        data.extend_from_slice(authority.as_ref()); // withdrawer
        data.extend_from_slice(&[0u8; 48]); // Lockup::default()
        let ix = Instruction {
            program_id: STAKE_PROGRAM_ID,
            accounts: vec![
                AccountMeta::new(stake.key(), false),
                AccountMeta::new_readonly(rent_sysvar.key(), false),
            ],
            data,
        };
        invoke(&ix, &[stake.clone(), rent_sysvar.clone()])?;
        Ok(())
    }

    pub fn delegate(
        &self,
        stake: &AccountInfo<'info>,
        vote: &AccountInfo<'info>,
        stake_config: &AccountInfo<'info>,
    ) -> Result<()> {
        let ix = Instruction {
            program_id: STAKE_PROGRAM_ID,
            accounts: vec![
                AccountMeta::new(stake.key(), false),
                AccountMeta::new_readonly(vote.key(), false),
                AccountMeta::new_readonly(self.clock.key(), false),
                AccountMeta::new_readonly(self.stake_history.key(), false),
                AccountMeta::new_readonly(stake_config.key(), false),
                AccountMeta::new_readonly(self.reserve.key(), true),
            ],
            data: IX_DELEGATE.to_le_bytes().to_vec(),
        };
        let infos = [
            stake.clone(),
            vote.clone(),
            self.clock.clone(),
            self.stake_history.clone(),
            stake_config.clone(),
            self.reserve.clone(),
        ];
        invoke_signed(&ix, &infos, &[self.reserve_seeds])?;
        Ok(())
    }

    pub fn deactivate(&self, stake: &AccountInfo<'info>) -> Result<()> {
        let ix = Instruction {
            program_id: STAKE_PROGRAM_ID,
            accounts: vec![
                AccountMeta::new(stake.key(), false),
                AccountMeta::new_readonly(self.clock.key(), false),
                AccountMeta::new_readonly(self.reserve.key(), true),
            ],
            data: IX_DEACTIVATE.to_le_bytes().to_vec(),
        };
        let infos = [stake.clone(), self.clock.clone(), self.reserve.clone()];
        invoke_signed(&ix, &infos, &[self.reserve_seeds])?;
        Ok(())
    }

    /// `dest` must come from `allocate` (uninitialized, stake-owned, rent-funded).
    pub fn split(
        &self,
        source: &AccountInfo<'info>,
        dest: &AccountInfo<'info>,
        lamports: u64,
    ) -> Result<()> {
        let mut data = IX_SPLIT.to_le_bytes().to_vec();
        data.extend_from_slice(&lamports.to_le_bytes());
        let ix = Instruction {
            program_id: STAKE_PROGRAM_ID,
            accounts: vec![
                AccountMeta::new(source.key(), false),
                AccountMeta::new(dest.key(), false),
                AccountMeta::new_readonly(self.reserve.key(), true),
            ],
            data,
        };
        let infos = [source.clone(), dest.clone(), self.reserve.clone()];
        invoke_signed(&ix, &infos, &[self.reserve_seeds])?;
        Ok(())
    }

    pub fn merge(&self, dest: &AccountInfo<'info>, source: &AccountInfo<'info>) -> Result<()> {
        let ix = Instruction {
            program_id: STAKE_PROGRAM_ID,
            accounts: vec![
                AccountMeta::new(dest.key(), false),
                AccountMeta::new(source.key(), false),
                AccountMeta::new_readonly(self.clock.key(), false),
                AccountMeta::new_readonly(self.stake_history.key(), false),
                AccountMeta::new_readonly(self.reserve.key(), true),
            ],
            data: IX_MERGE.to_le_bytes().to_vec(),
        };
        let infos = [
            dest.clone(),
            source.clone(),
            self.clock.clone(),
            self.stake_history.clone(),
            self.reserve.clone(),
        ];
        invoke_signed(&ix, &infos, &[self.reserve_seeds])?;
        Ok(())
    }

    /// Withdraws every lamport back to the reserve; the emptied stake account is deleted.
    pub fn withdraw_all(&self, stake: &AccountInfo<'info>) -> Result<u64> {
        let lamports = stake.lamports();
        let mut data = IX_WITHDRAW.to_le_bytes().to_vec();
        data.extend_from_slice(&lamports.to_le_bytes());
        let ix = Instruction {
            program_id: STAKE_PROGRAM_ID,
            accounts: vec![
                AccountMeta::new(stake.key(), false),
                AccountMeta::new(self.reserve.key(), false),
                AccountMeta::new_readonly(self.clock.key(), false),
                AccountMeta::new_readonly(self.stake_history.key(), false),
                AccountMeta::new_readonly(self.reserve.key(), true),
            ],
            data,
        };
        let infos = [
            stake.clone(),
            self.reserve.clone(),
            self.clock.clone(),
            self.stake_history.clone(),
        ];
        invoke_signed(&ix, &infos, &[self.reserve_seeds])?;
        Ok(lamports)
    }
}
