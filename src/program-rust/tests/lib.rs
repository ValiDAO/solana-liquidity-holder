use borsh::BorshDeserialize;
use helloworld::{process_instruction, GreetingAccount};
use solana_program_test::*;
use solana_sdk::{
    account::Account,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signer::keypair::Keypair,
    signature::Signer,
    transaction::Transaction,
};
use spl_token::{
    ID as SPL_TOKEN_PROGRAM_ID
};
use std::mem;
use core::convert::TryInto;

#[tokio::test]
async fn test_staking() {

}

#[tokio::test]
async fn test_helloworld() {
    let program_id = Pubkey::new_unique();
    // spl-token create-token -ul
    // 6e13iFuJrkFVF1WsrDZ1ux5Ni5RTWKc6Br6cZTzsimVB
    let token_mint_account = Pubkey::new(&[
        0x53, 0xc4, 0xfa, 0x38, 0x7e, 0xbe, 0x8c, 0x81, 0xcd, 0x6c, 0x80, 0x50, 0x34, 0x85, 0x88, 0x3b,
        0xb6, 0x34, 0x5c, 0x04, 0xce, 0xa0, 0xed, 0xf0, 0xfe, 0xc4, 0xc8, 0x74, 0x79, 0x09, 0xd2, 0x3a]);

    // spl-token -ul create-account 6e13iFuJrkFVF1WsrDZ1ux5Ni5RTWKc6Br6cZTzsimVB
    // GfmS5GvqiH1HFi1xVLmdMjTv4eRpp4Qyg79wPFQcDmwE (with 1000 tokens)
    let owners_token_account = Pubkey::new(&[
        0xe8, 0xcd, 0x9d, 0x49, 0xd3, 0xe3, 0xa3, 0x7a, 0xe8, 0x93, 0x9b, 0xb1, 0x87, 0x6e, 0xf7, 0x1a,
        0xf4, 0xc9, 0x2e, 0x2c, 0xbf, 0x77, 0x69, 0xb6, 0xe9, 0x81, 0x6c, 0x78, 0xa9, 0x1b, 0x17, 0x69]);

    // spl-token mint 6e13iFuJrkFVF1WsrDZ1ux5Ni5RTWKc6Br6cZTzsimVB 1000 GfmS5GvqiH1HFi1xVLmdMjTv4eRpp4Qyg79wPFQcDmwE -ul
    let pool_token_account = Pubkey::new_unique();
    let (pool_manager_account, bump_seed) = Pubkey::find_program_address(&[&[0x50, 0x00, 0x00, 0x10, 0x20, 0xad, 0x35]], &program_id);

    // TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA
    let token_program = Pubkey::new(&[
        0x06, 0xdd, 0xf6, 0xe1, 0xd7, 0x65, 0xa1, 0x93, 0xd9, 0xcb, 0xe1, 0x46, 0xce, 0xeb, 0x79, 0xac,
        0x1c, 0xb4, 0x85, 0xed, 0x5f, 0x5b, 0x37, 0x91, 0x3a, 0x8c, 0xf5, 0x85, 0x7e, 0xff, 0x00, 0xa9]);

    let mut program_test = ProgramTest::new(
        "helloworld", // Run the BPF version with `cargo test-bpf`
        program_id,
        processor!(process_instruction), // Run the native version with `cargo test`
    );
    // Add token mint account
    program_test.add_account(
        token_mint_account,
        Account {
            lamports: 1,
            data: [
                0x01, 0x00, 0x00, 0x00, 0xb4, 0x0d, 0x20, 0xe1, 0xf1, 0x0d, 0x85, 0xe4, 0x7a, 0x2f, 0x7b, 0x7f,
                0x51, 0x5f, 0xd7, 0xae, 0x9f, 0x08, 0xa1, 0x88, 0x0f, 0x56, 0x93, 0x42, 0xf6, 0x27, 0x45, 0xae,
                0x3f, 0x05, 0xa5, 0x60, 0x00, 0x10, 0xa5, 0xd4, 0xe8, 0x00, 0x00, 0x00, 0x09, 0x01, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00
            ].to_vec(),
            owner: token_program,
            ..Account::default()
        }
    );

    let mut pool_token_account_data = vec![];
    pool_token_account_data.extend_from_slice(&token_mint_account.to_bytes());
    pool_token_account_data.extend_from_slice(&pool_manager_account.to_bytes());
    pool_token_account_data.extend_from_slice(&[
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00
    ]);
    program_test.add_account(
        pool_token_account,
        Account {
            lamports: 1,
            data: pool_token_account_data,
            owner: token_program,
            ..Account::default()
        }
    );

    let staking_account_id = Pubkey::new_unique();
    program_test.add_account(
        staking_account_id,
        Account {
            lamports: 1,
            data: [0u8;1 + 32 + 8 + 2 + 8 + 8 + 8].to_vec(),
            owner: program_id,
            ..Account::default()
        }
    );

    let owner = Keypair::new();

    let old_staking_account_id = Pubkey::new_unique();
    let mut old_staking_account_data = vec![0x01u8];
    old_staking_account_data.extend_from_slice(&owner.pubkey().to_bytes());
    program_test.add_account(
        old_staking_account_id,
        Account {
            lamports: 1,
            data: old_staking_account_data,
            owner: program_id,
            ..Account::default()
        }
    );

    let mut owners_token_account_data = vec![];
    owners_token_account_data.extend_from_slice(&token_mint_account.to_bytes());
    owners_token_account_data.extend_from_slice(&owner.pubkey().to_bytes());
    owners_token_account_data.extend_from_slice(&[
        0x00, 0x10, 0xa5, 0xd4, 0xe8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00
    ]);
    program_test.add_account(
        owners_token_account,
        Account {
            lamports: 1,
            data: owners_token_account_data,
            owner: token_program,
            ..Account::default()
        }
    );

    let (mut banks_client, payer, recent_blockhash) = program_test.start().await;

    // Stake
    let instruction_data: [u8;12] = [
        0x00,
        0xb4, 0x00,
        0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        bump_seed];
    let mut transaction = Transaction::new_with_payer(
        &[Instruction::new_with_bincode(
            program_id,
            &instruction_data,
            vec![
                AccountMeta::new(staking_account_id, false),
                AccountMeta::new(owner.pubkey(), true),
                AccountMeta::new(owners_token_account, false),
                AccountMeta::new(pool_token_account, false),
                AccountMeta::new_readonly(token_program, false),
            ],
        )],
        Some(&payer.pubkey()),
    );
    transaction.sign(&[&payer, &owner], recent_blockhash);
    banks_client.process_transaction(transaction).await.unwrap();

    // Проверка что деньги на пуле появились.
    let pool_token_account_after_staking = banks_client
        .get_account(pool_token_account)
        .await
        .expect("get_account")
        .expect("greeted_account not found");

    let amount_after_staking = u64::from_le_bytes(pool_token_account_after_staking.data.get(64..72).unwrap().try_into().unwrap());
    assert_eq!(18374686479671623685u64, amount_after_staking);

    let instruction_data: [u8;2] = [
        0x01,
        bump_seed];
    let mut transaction = Transaction::new_with_payer(
        &[Instruction::new_with_bincode(
            program_id,
            &instruction_data,
            vec![
                AccountMeta::new(staking_account_id, false),
                AccountMeta::new(owner.pubkey(), true),
                AccountMeta::new(owners_token_account, false),
                AccountMeta::new(pool_token_account, false),
                AccountMeta::new_readonly(token_program, false),
                AccountMeta::new_readonly(pool_manager_account, false),
            ],
        )],
        Some(&payer.pubkey()),
    );
    transaction.sign(&[&payer, &owner], recent_blockhash);
    banks_client.process_transaction(transaction).await.unwrap();   
}
