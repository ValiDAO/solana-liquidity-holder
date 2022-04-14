use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    clock::{UnixTimestamp, Clock},
    entrypoint,
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
    system_program::{check_id, ID as SYSTEM_PROGRAM_ID},
    program::{invoke, invoke_signed},
};
use spl_token::{
    check_program_account,
    ID as SPL_TOKEN_PROGRAM_ID,
    instruction::transfer,
};
use std::convert::TryFrom;

const ALLOWED_DURATIONS_DAYS: [u16; 2] = [180, 360];
const ANNUAL_INTEREST_NOMITATORS: [u64; 2] = [15, 17];
const ANNUAL_INTEREST_DENOMITATORS: [u64; 2] = [100, 100];
const SECONDS_PER_YEAR: u64 = 360 * 24 * 3600;
const INTEREST_ALLOCATION_PERIOD_SECONDS: u64 = 60;
const ALLOCATION_PERDIODS_PER_YEAR: u64 = SECONDS_PER_YEAR / INTEREST_ALLOCATION_PERIOD_SECONDS;
const INTEREST_NOMITATORS_PER_ALLOCATION_PERIOD: [u64; 2] = ANNUAL_INTEREST_NOMITATORS;
const INTEREST_DENOMITATORS_PER_ALLOCATION_PERIOD: [u64; 2] = [
    ANNUAL_INTEREST_DENOMITATORS[0] * ALLOCATION_PERDIODS_PER_YEAR,
    ANNUAL_INTEREST_DENOMITATORS[1] * ALLOCATION_PERDIODS_PER_YEAR,
];
const POOL_ADDRESS_SEED: &[u8] = &[0x50, 0x00, 0x00, 0x10, 0x20, 0xad, 0x35];

#[derive(Clone, Debug, PartialEq)]
pub enum Instruction {
    // Checks and initializes an empty account.
    // Accepted accounts:
    //    [readable, signed] - owner account, signed, mostly to avoid fat finger errors.
    //    [writable] - bets account
    Stake{
        duration: u16,  // allowed 180, 360, (ALLOWED_DURATIONS_DAYS)
        amount: u64,
        bump_seed: u8,
    },
    WithdrawInterest{
        bump_seed: u8,
    },
    Compound{
        bump_seed: u8,
    },
    CloseAccount{
        bump_seed: u8,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum WithdrawStrategy {
    InterestOnly,
    Compound,
    CloseAccount,
}

impl Instruction {
    pub fn unpack(input: &[u8]) -> Result<Self, ProgramError> {
        use std::convert::TryInto;
        use ProgramError::InvalidInstructionData;
        let (&tag, rest) = input.split_first().ok_or(InvalidInstructionData)?;
        Ok(match tag {
            0 => {
                let (duration, rest) = rest.split_at(2);
                let duration = duration
                    .try_into()
                    .ok()
                    .map(u16::from_le_bytes)
                    .ok_or(InvalidInstructionData)?;
                let (amount, bump_seed) = rest.split_at(8);
                let amount = amount
                    .try_into()
                    .ok()
                    .map(u64::from_le_bytes)
                    .ok_or(InvalidInstructionData)?;
                let (bump_seed, _nothing) = bump_seed.split_first().ok_or(InvalidInstructionData)?;
                msg!("Duration {} Amount {} Bump {}", duration, amount, bump_seed);

                Self::Stake { duration: duration as u16, amount, bump_seed: *bump_seed }
            },
            1 => {
                let (bump_seed, _nothing) = rest.split_first().ok_or(InvalidInstructionData)?;
                Self::WithdrawInterest { bump_seed: *bump_seed }
            },
            2 => {
                let (bump_seed, _nothing) = rest.split_first().ok_or(InvalidInstructionData)?;
                Self::Compound { bump_seed: *bump_seed }
            },
            3 => {
                let (bump_seed, _nothing) = rest.split_first().ok_or(InvalidInstructionData)?;
                Self::CloseAccount { bump_seed: *bump_seed }
            },
            _ => unreachable!()
        })
    }
}


/// Define the type of state stored in accounts
#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub struct GreetingAccount {
    /// number of greetings
    pub counter: u32,
}
/// Define the type of state stored in accounts
#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub struct StakingAccount {
    pub initialized: bool,       // для предотвращения атак перезаписи
    pub holder: Pubkey,          // ключ владельца
    pub created: UnixTimestamp,  // время создания
    pub duration: u16,           // период лока
    pub token_amount: u64,       // количество застейканных токенов
    pub last_withdraw_date: UnixTimestamp, // время, начиная с которого у нас застейкано token_amount
    pub extra_not_withdrawn_tokens: u64,   // это самое сложное. Если в результате ре-стейкинга у нас изменяется
                                           // поле token_amount, то нам надо сохранить информацию о процентах,
                                           // набежавших до ре-стейкинга.
}
const STAKING_ACCOUNT_SIZE: usize = 1 + 32 + 8 + 2 + 8 + 8 + 8;

pub fn _process_staking_instruction(
        program_id: &Pubkey, 
        staking_acc: &AccountInfo,
        owners_acc: &AccountInfo,
        owner_token_acc: &AccountInfo,
        pool_token_acc: &AccountInfo,
        token_amount: u64,
        now: UnixTimestamp,
        duration: u16,
        bump_seed: u8) -> ProgramResult {
    if staking_acc.owner != program_id {
        msg!("Staking account does not have the correct program id");
        return Err(ProgramError::IncorrectProgramId);
    }
    // POOL = DjHnG6xbtxyT297XZWKyqxPsAngtoy9GkuehavoGUcY4
    // let expected_pool_address = &Pubkey::create_program_address(
    //     &[POOL_ADDRESS_SEED, &[bump_seed]],
    //     program_id
    // )?;
    // if pool_token_acc.key != expected_pool_address {
    //     msg!("Wrong pool address. Expected {} but got {}", expected_pool_address, pool_token_acc.key);
    //     return Err(ProgramError::InvalidAccountData);
    // }
    let expected_pool_address = pool_token_acc.key;
    // Остальные проверки касательно токенов выполнятся самой токен-программой.

    if ALLOWED_DURATIONS_DAYS.iter().position(|&allowed_duration| allowed_duration == duration).is_none() {
        msg!("Selected duration {} is not allowed", duration);
        return Err(ProgramError::InvalidInstructionData);
    }
    let mut staking_info = StakingAccount::try_from_slice(&staking_acc.data.borrow())?;
    if staking_info.initialized { 
        return Err(ProgramError::AccountAlreadyInitialized);
    }

    staking_info.initialized = true;
    staking_info.holder = *owners_acc.key;
    staking_info.created = now;
    staking_info.token_amount = token_amount;
    staking_info.last_withdraw_date = now;
    staking_info.duration = duration;
    staking_info.serialize(&mut &mut staking_acc.data.borrow_mut()[..])?;
    Ok(())
}

pub fn _process_withdraw_interest_instruction(
    program_id: &Pubkey,
    staking_acc: &AccountInfo,
    owners_acc: &AccountInfo,
    owner_token_acc: &AccountInfo,
    pool_token_acc: &AccountInfo,
    now: UnixTimestamp,
    bump_seed: u8,
    withdraw_strategy: WithdrawStrategy,
) -> Result<u64, ProgramError> {
    if staking_acc.owner != program_id {
        msg!("Staking account does not have the correct program id");
        return Err(ProgramError::IncorrectProgramId);
    }
    let mut staking_info = StakingAccount::try_from_slice(&staking_acc.data.borrow())?;
    if !staking_info.initialized {
        return Err(ProgramError::AccountAlreadyInitialized);
    }
    if &staking_info.holder != owners_acc.key {
        msg!("Staking account can not be withdrawn to someone else");
        return Err(ProgramError::InvalidAccountData);
    }
    if !owners_acc.is_signer {
        msg!("Withdraw instruction must be signed, otherwise, even the money is not stolen, you are loosing a chance to get compound interest");
        return Err(ProgramError::InvalidAccountData);
    }
    if now < staking_info.last_withdraw_date {
        msg!("Staking account was created in the future?!");
        return Err(ProgramError::InvalidAccountData);
    }
    
    let mut interest_index = ALLOWED_DURATIONS_DAYS.len();
    for i in 0..ALLOWED_DURATIONS_DAYS.len() {
        if ALLOWED_DURATIONS_DAYS[i] == staking_info.duration {
            interest_index = i;
        }
    }
    if interest_index == ALLOWED_DURATIONS_DAYS.len() {
        msg!("Duration {} is not supported", staking_info.duration);
        return Err(ProgramError::InvalidAccountData);
    }

    let intervals_from_last_withdrawal: u64 = u64::try_from((now - staking_info.last_withdraw_date) / (INTEREST_ALLOCATION_PERIOD_SECONDS as i64)).or(Err(ProgramError::InvalidAccountData))?;
    staking_info.last_withdraw_date += i64::try_from(INTEREST_ALLOCATION_PERIOD_SECONDS * intervals_from_last_withdrawal).or(Err(ProgramError::InvalidAccountData))?;
    let accumulated_interest = staking_info.token_amount * INTEREST_NOMITATORS_PER_ALLOCATION_PERIOD[interest_index] * intervals_from_last_withdrawal / INTEREST_DENOMITATORS_PER_ALLOCATION_PERIOD[interest_index];

    match withdraw_strategy {
        WithdrawStrategy::InterestOnly => {
            staking_info.serialize(&mut &mut staking_acc.data.borrow_mut()[..])?;
            Ok(accumulated_interest)
        },
        WithdrawStrategy::Compound => {
            staking_info.token_amount += accumulated_interest;
            staking_info.serialize(&mut &mut staking_acc.data.borrow_mut()[..])?;
            Ok(0)
        },
        WithdrawStrategy::CloseAccount => {
            if ((now - staking_info.created) as u64) / (3600u64 * 24u64) < staking_info.duration.into() {
                Err(ProgramError::InvalidInstructionData)
            } else {
                let total_to_withdraw = staking_info.token_amount + accumulated_interest;
                staking_info.token_amount = 0;
                staking_info.last_withdraw_date = now;
                staking_info.serialize(&mut &mut staking_acc.data.borrow_mut()[..])?;
                Ok(total_to_withdraw)
            }
        }
    }
}

// Declare and export the program's entrypoint
entrypoint!(process_instruction);

// Program entrypoint's implementation
pub fn process_instruction(
    program_id: &Pubkey, // Public key of the account the hello world program was loaded into
    accounts: &[AccountInfo], // The account to say hello to
    _instruction_data: &[u8], // Ignored, all helloworld instructions are hellos
) -> ProgramResult {
    use solana_program::sysvar::Sysvar;
    let instruction = Instruction::unpack(_instruction_data)?;
    let account_info_iter = &mut accounts.iter();
    match instruction {
        Instruction::Stake { duration, amount, bump_seed } => {
            let staking_account = next_account_info(account_info_iter)?;
            let owner_account = next_account_info(account_info_iter)?;
            let owner_token_account = next_account_info(account_info_iter)?;
            let pool_token_account = next_account_info(account_info_iter)?;
            let token_program = next_account_info(account_info_iter)?;
            _process_staking_instruction(
                program_id,
                staking_account,
                owner_account,
                owner_token_account,
                pool_token_account,
                amount,
                Clock::get()?.unix_timestamp,
                duration,
                bump_seed)?;
            if token_program.key != &SPL_TOKEN_PROGRAM_ID {
                return Err(ProgramError::IncorrectProgramId);
            }
            // let expected_pool_address = &Pubkey::create_program_address(
            //     &[POOL_ADDRESS_SEED, &[bump_seed]],
            //     program_id
            // )?;
            let ix = spl_token::instruction::transfer(
                &SPL_TOKEN_PROGRAM_ID,
                owner_token_account.key,
                pool_token_account.key,
                owner_account.key,
                &[&owner_account.key],
                amount,
            )?;
            msg!("Signer info: {}", owner_account.key);
            msg!("Owner info: {}", Pubkey::new(&owner_token_account.data.borrow()[32..64]));
            invoke(&ix, &[
                owner_token_account.clone(),
                pool_token_account.clone(),
                owner_account.clone(),
                token_program.clone(),
            ])?;
        },
        Instruction::WithdrawInterest {bump_seed} | Instruction::Compound {bump_seed} | Instruction::CloseAccount {bump_seed} => {
            let staking_account = next_account_info(account_info_iter)?;
            let owner_account = next_account_info(account_info_iter)?;
            let owner_token_account = next_account_info(account_info_iter)?;
            let pool_token_account = next_account_info(account_info_iter)?;
            let token_program = next_account_info(account_info_iter)?;
            let pool_manager_account = next_account_info(account_info_iter)?;
            let amount = _process_withdraw_interest_instruction(
                program_id,
                staking_account,
                owner_account,
                owner_token_account,
                pool_token_account,
                Clock::get()?.unix_timestamp,
                bump_seed,
                match instruction {
                    Instruction::WithdrawInterest{..} => WithdrawStrategy::InterestOnly,
                    Instruction::Compound{..} => WithdrawStrategy::Compound,
                    Instruction::CloseAccount{..} => WithdrawStrategy::CloseAccount,
                    _ => unreachable!(),
                })?;
            if token_program.key != &SPL_TOKEN_PROGRAM_ID {
                return Err(ProgramError::IncorrectProgramId);
            }
            let pool_owner = &Pubkey::create_program_address(
                &[POOL_ADDRESS_SEED, &[bump_seed]],
                program_id
            )?;
            let ix = spl_token::instruction::transfer(
                &SPL_TOKEN_PROGRAM_ID,
                pool_token_account.key,
                owner_token_account.key,
                pool_owner,
                &[&pool_owner],
                amount,
            )?;
            msg!("Invoke signed. Pool owner={}. Sending {} from pool", pool_owner, amount);
            invoke_signed(&ix, &[
                pool_manager_account.clone(),
                pool_token_account.clone(),
                owner_token_account.clone(),
                owner_account.clone(),
                token_program.clone(),
            ], &[&[&[0x50, 0x00, 0x00, 0x10, 0x20, 0xad, 0x35][..], &[bump_seed]]])?;
        }
    }

    Ok(())
}

// Sanity tests
#[cfg(test)]
mod test {
    use super::*;
    use solana_program::clock::Epoch;
    use std::mem;

    #[test]
    fn test_initialize_staking_account() {
        // Проверка инициализации staking-PDA аккаунта.
        // Нужные значения сохраняются + не допускается повторная инициализация.
        let program_id = Pubkey::new_unique();
    
        let owner = Pubkey::new_unique();

        let staking_account_key = Pubkey::new_unique();
        let mut staking_account_lamports = 0;
        let mut staking_account_data = vec![0; STAKING_ACCOUNT_SIZE];
        let mut staking_account = AccountInfo::new(
            &staking_account_key,
            false,
            true,
            &mut staking_account_lamports,
            &mut staking_account_data,
            &program_id,
            false,
            Epoch::default(),
        );

        let owners_token_account_key = Pubkey::new_unique();
        let mut owners_token_account_lamports = 0;
        let mut owners_token_account_data = vec![0; 0];
        let owners_token_account = AccountInfo::new(
            &owners_token_account_key,
            false,
            true,
            &mut owners_token_account_lamports,
            &mut owners_token_account_data,
            &SPL_TOKEN_PROGRAM_ID,
            false,
            Epoch::default(),
        );

        let (pools_token_account_key, bump_seed) = Pubkey::find_program_address(&[POOL_ADDRESS_SEED], &program_id);
        let mut pools_token_account_lamports = 0;
        let mut pools_token_account_data = vec![0; 0];
        let pools_token_account = AccountInfo::new(
            &pools_token_account_key,
            false,
            true,
            &mut pools_token_account_lamports,
            &mut pools_token_account_data,
            &SPL_TOKEN_PROGRAM_ID,
            false,
            Epoch::default()
        );

        let mut owners_account_lamports = 0;
        let mut owners_account_data = vec![0; 0];
        let owners_account = AccountInfo::new(
            &owner,
            true,
            false,
            &mut owners_account_lamports,
            &mut owners_account_data,
            &SYSTEM_PROGRAM_ID,
            false,
            Epoch::default(),
        );


        let instruction_data: Vec<u8> = Vec::new();

        assert_eq!(
            StakingAccount::try_from_slice(&staking_account.data.borrow())
                .unwrap()
                .initialized,
            false
        );
        
        let result = _process_staking_instruction(
            &program_id, 
            &mut staking_account,
            &owners_account,
            &owners_token_account,
            &pools_token_account,
            12u64,
            1234567890 as UnixTimestamp,
            360u16,
            bump_seed).unwrap();

        let staking_account_initialized = StakingAccount::try_from_slice(&staking_account.data.borrow()).unwrap();
        assert_eq!(staking_account_initialized.initialized, true);
        assert_eq!(staking_account_initialized.holder, owner);
        assert_eq!(staking_account_initialized.created, 1234567890 as UnixTimestamp);
        assert_eq!(staking_account_initialized.token_amount, 12u64);
        assert_eq!(staking_account_initialized.extra_not_withdrawn_tokens, 0u64);
        assert_eq!(staking_account_initialized.last_withdraw_date, 1234567890 as UnixTimestamp);

        let second_invocation_result = _process_staking_instruction(
            &program_id, 
            &mut staking_account,
            &owners_account,
            &owners_token_account,
            &pools_token_account,
            12u64,
            1234567890 as UnixTimestamp,
            360u16,
            bump_seed);
        assert_eq!(second_invocation_result.is_ok(), false);
    }

    #[test]
    fn test_wrong_owner() {
        // Это довольно странный тест, но все так делают почему-то...
        // Мне страшно идти на поводу своих принципов и не делать его,
        // даже если я не вижу в этом смысла.
        let program_id = Pubkey::new_unique();
        let other_program_id = Pubkey::new_unique();
    
        let owner = Pubkey::new_unique();

        let staking_account_key = Pubkey::new_unique();
        let mut staking_account_lamports = 0;
        let mut staking_account_data = vec![0; STAKING_ACCOUNT_SIZE];
        let mut staking_account = AccountInfo::new(
            &staking_account_key,
            false,
            true,
            &mut staking_account_lamports,
            &mut staking_account_data,
            &other_program_id,
            false,
            Epoch::default(),
        );

        let owners_token_account_key = Pubkey::new_unique();
        let mut owners_token_account_lamports = 0;
        let mut owners_token_account_data = vec![0; 0];
        let owners_token_account = AccountInfo::new(
            &owners_token_account_key,
            false,
            true,
            &mut owners_token_account_lamports,
            &mut owners_token_account_data,
            &SPL_TOKEN_PROGRAM_ID,
            false,
            Epoch::default(),
        );

        let (pools_token_account_key, bump_seed) = Pubkey::find_program_address(&[POOL_ADDRESS_SEED], &program_id);
        let mut pools_token_account_lamports = 0;
        let mut pools_token_account_data = vec![0; 0];
        let pools_token_account = AccountInfo::new(
            &pools_token_account_key,
            false,
            true,
            &mut pools_token_account_lamports,
            &mut pools_token_account_data,
            &SPL_TOKEN_PROGRAM_ID,
            false,
            Epoch::default(),
        );

        let mut owners_account_lamports = 0;
        let mut owners_account_data = vec![0; 0];
        let owners_account = AccountInfo::new(
            &owner,
            true,
            false,
            &mut owners_account_lamports,
            &mut owners_account_data,
            &SYSTEM_PROGRAM_ID,
            false,
            Epoch::default(),
        );

        let result = _process_staking_instruction(
            &program_id, 
            &mut staking_account,
            &owners_account,
            &owners_token_account,
            &pools_token_account,
            12u64,
            1234567890 as UnixTimestamp,
            360u16,
            bump_seed);
        assert_eq!(result.is_ok(), false);
    }

    // #[test]
    // fn test_wrong_pool_id() {
    //     let program_id = Pubkey::new_unique();    
    //     let owner = Pubkey::new_unique();

    //     let staking_account_key = Pubkey::new_unique();
    //     let mut staking_account_lamports = 0;
    //     let mut staking_account_data = vec![0; STAKING_ACCOUNT_SIZE];
    //     let mut staking_account = AccountInfo::new(
    //         &staking_account_key,
    //         false,
    //         true,
    //         &mut staking_account_lamports,
    //         &mut staking_account_data,
    //         &program_id,
    //         false,
    //         Epoch::default(),
    //     );

    //     let owners_token_account_key = Pubkey::new_unique();
    //     let mut owners_token_account_lamports = 0;
    //     let mut owners_token_account_data = vec![0; 0];
    //     let owners_token_account = AccountInfo::new(
    //         &owners_token_account_key,
    //         false,
    //         true,
    //         &mut owners_token_account_lamports,
    //         &mut owners_token_account_data,
    //         &SPL_TOKEN_PROGRAM_ID,
    //         false,
    //         Epoch::default(),
    //     );

    //     let (pools_token_account_key, bump_seed) = Pubkey::find_program_address(&[POOL_ADDRESS_SEED], &program_id);
    //     let mut pools_token_account_lamports = 0;
    //     let mut pools_token_account_data = vec![0; 0];
    //     let pools_token_account = AccountInfo::new(
    //         &pools_token_account_key,
    //         false,
    //         true,
    //         &mut pools_token_account_lamports,
    //         &mut pools_token_account_data,
    //         &SPL_TOKEN_PROGRAM_ID,
    //         false,
    //         Epoch::default()
    //     );

    //     let mut owners_account_lamports = 0;
    //     let mut owners_account_data = vec![0; 0];
    //     let owners_account = AccountInfo::new(
    //         &owner,
    //         true,
    //         false,
    //         &mut owners_account_lamports,
    //         &mut owners_account_data,
    //         &SYSTEM_PROGRAM_ID,
    //         false,
    //         Epoch::default(),
    //     );

    //     let result = _process_staking_instruction(
    //         &program_id, 
    //         &mut staking_account,
    //         &owners_account,
    //         &owners_token_account,
    //         &pools_token_account,
    //         12u64,
    //         1234567890 as UnixTimestamp,
    //         360u16,
    //         bump_seed - 1);
    //     assert_eq!(result.is_ok(), false);
    // }

    #[test]
    fn test_wrong_duration() {
        let program_id = Pubkey::new_unique();    
        let owner = Pubkey::new_unique();

        let staking_account_key = Pubkey::new_unique();
        let mut staking_account_lamports = 0;
        let mut staking_account_data = vec![0; STAKING_ACCOUNT_SIZE];
        let mut staking_account = AccountInfo::new(
            &staking_account_key,
            false,
            true,
            &mut staking_account_lamports,
            &mut staking_account_data,
            &program_id,
            false,
            Epoch::default(),
        );

        let owners_token_account_key = Pubkey::new_unique();
        let mut owners_token_account_lamports = 0;
        let mut owners_token_account_data = vec![0; 0];
        let owners_token_account = AccountInfo::new(
            &owners_token_account_key,
            false,
            true,
            &mut owners_token_account_lamports,
            &mut owners_token_account_data,
            &SPL_TOKEN_PROGRAM_ID,
            false,
            Epoch::default(),
        );

        let (pools_token_account_key, bump_seed) = Pubkey::find_program_address(&[POOL_ADDRESS_SEED], &program_id);
        let mut pools_token_account_lamports = 0;
        let mut pools_token_account_data = vec![0; 0];
        let pools_token_account = AccountInfo::new(
            &pools_token_account_key,
            false,
            true,
            &mut pools_token_account_lamports,
            &mut pools_token_account_data,
            &SPL_TOKEN_PROGRAM_ID,
            false,
            Epoch::default()
        );

        let mut owners_account_lamports = 0;
        let mut owners_account_data = vec![0; 0];
        let owners_account = AccountInfo::new(
            &owner,
            true,
            false,
            &mut owners_account_lamports,
            &mut owners_account_data,
            &SYSTEM_PROGRAM_ID,
            false,
            Epoch::default(),
        );

        let result = _process_staking_instruction(
            &program_id, 
            &mut staking_account,
            &owners_account,
            &owners_token_account,
            &pools_token_account,
            12u64,
            1234567890 as UnixTimestamp,
            45u16,
            bump_seed);
        assert_eq!(result.is_ok(), false);
    }

    #[test]
    fn test_unpacking_instructions() {
        let data = [0, 0xb4, 0x00, 5, 0, 0, 0, 0, 0, 0, 0, 2];
        let instruction = Instruction::unpack(&data).unwrap();
        assert_eq!(instruction, Instruction::Stake{duration: 180, amount: 5, bump_seed: 2})
    }

    #[test]
    #[should_panic]
    fn test_unpacking_instructions_short_data() {
        let data = [0, 0xb4, 0x00, 5, 0, 0, 0 , 0, 0, 0];
        Instruction::unpack(&data);
    }

    #[test]
    #[should_panic]
    fn test_unpacking_instructions_bad_first_byte() {
        let data = [0xff, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 5, 0, 0, 0 , 0, 0, 0, 0];
        Instruction::unpack(&data);
    }

    #[test]
    fn test_interest_on_unlocked_account() {

    }

    #[test]
    fn test_unstaking_wrong_owner() {

    }


    #[test]
    fn test_no_interest_on_closed_account() {

    }
}
