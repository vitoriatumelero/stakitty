//! LOCAL DEMO ONLY: a mock of MagicBlock's VRF oracle, not a source of secure randomness.
//!
//! - Any instruction other than `FULFILL` is accepted as a randomness request (a no-op).
//! - `FULFILL` = `[0xFF; 8] || callback instruction data`. Accounts: `[callback_program,
//!   callback accounts...]`. It CPIs the callback, signing with this program's
//!   `["identity", callback_program]` PDA: the same scoped identity the real VRF program
//!   signs with, so the callback program runs its normal checks.

use solana_program::{
    account_info::AccountInfo,
    entrypoint,
    entrypoint::ProgramResult,
    instruction::{AccountMeta, Instruction},
    msg,
    program::invoke_signed,
    program_error::ProgramError,
    pubkey::Pubkey,
};

const FULFILL: [u8; 8] = [0xFF; 8];
const IDENTITY: &[u8] = b"identity";

entrypoint!(process_instruction);

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    data: &[u8],
) -> ProgramResult {
    let Some(callback_data) = data.strip_prefix(&FULFILL) else {
        msg!("MOCK VRF: request accepted (local demo, no oracle)");
        return Ok(());
    };
    let [callback_program, callback_accounts @ ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    let (identity, bump) =
        Pubkey::find_program_address(&[IDENTITY, callback_program.key.as_ref()], program_id);
    let metas = callback_accounts
        .iter()
        .map(|a| AccountMeta {
            pubkey: *a.key,
            is_signer: *a.key == identity,
            is_writable: a.is_writable,
        })
        .collect();
    let ix = Instruction {
        program_id: *callback_program.key,
        accounts: metas,
        data: callback_data.to_vec(),
    };
    msg!("MOCK VRF: delivering randomness (local demo)");
    invoke_signed(
        &ix,
        callback_accounts,
        &[&[IDENTITY, callback_program.key.as_ref(), &[bump]]],
    )
}
