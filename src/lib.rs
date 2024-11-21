use solana_program::{
    account_info::{next_account_info, AccountInfo}, entrypoint::ProgramResult, entrypoint, msg, program::{invoke, invoke_signed}, program_error::ProgramError, pubkey::Pubkey, rent::Rent, system_instruction, sysvar::Sysvar
};

use borsh::{
    BorshSerialize,
    BorshDeserialize
};


// pub token_mint: Option<Pubkey>, // 33
#[derive(BorshDeserialize, BorshSerialize, Debug)]
pub struct SellOrder {
    pub order_id: u64, //8
    pub seller: Pubkey, //32
    pub escrow_account: Pubkey, //32
    pub amount: u64,//8
    pub price: u64, //8
    pub status: OrderStatus, //1
}

// #[derive(BorshDeserialize, BorshSerialize, Debug)]
// pub struct Orders {
//     pub count: u64,
// }

#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub enum OrderStatus {
    Active = 0,
    Completed = 1,
    Cancelled = 2,
}

#[derive(BorshDeserialize, BorshSerialize, Debug)]
pub struct OrderCounter {
    pub total_orders: u64,
    pub authority: Pubkey
}

impl OrderCounter {
    pub const SEED_PREFIX: &'static [u8] = b"order_counter";
    pub const SIZE: usize = 8 + 32; // u64 + Pubkey
}

impl SellOrder {
    pub const SEED_PREFIX: &'static [u8] = b"escrow_order";
    // Calculate full size based on your struct
    pub const SIZE: usize = 8 + 32 + 32 + 8 + 8 + 1; // order_id + pubkey + amount + status
    // pub const SIZE: usize = 8 + 32 + 33 + 32 + 8 + 8 + 1; // order_id + pubkey + amount + status
}

entrypoint!(process_instruction);

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8]
) -> ProgramResult {
    let instruction = instruction_data[0];

    return match instruction {
        0 =>  create_sell_order(program_id, accounts, &instruction_data[1..]),
        1 =>  fulfil_buy_order(program_id, accounts, &instruction_data[1..]),
        _ =>  Err(ProgramError::InvalidInstructionData)
    };

    // Ok(())
}

pub fn create_sell_order(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    _instruction_data: &[u8],
) -> ProgramResult {
    let account_iterations = &mut accounts.iter();

    let authority_account = next_account_info(account_iterations)?;
    let counter_account = next_account_info(account_iterations)?;
    let system_program = next_account_info(account_iterations)?;
    let seller_account = next_account_info(account_iterations)?;
    let escrow_account = next_account_info(account_iterations)?;

    let sell_order_data = SellOrder::try_from_slice(_instruction_data).map_err(|err| {
        msg!("Error Deseriealizing order, {:?}", err);
        ProgramError::InvalidInstructionData
    })?;

    
    if counter_account.data_is_empty() {
        msg!("Creating COunter");
        let _ = initialize_counter(program_id, accounts);
    }

    let mut counter_data = OrderCounter::try_from_slice(&counter_account.data.borrow())?;
    let order_id = counter_data.total_orders;

    // Create seeds for the order PDA
    let seeds = &[
        SellOrder::SEED_PREFIX,
        seller_account.key.as_ref(),
        &order_id.to_le_bytes(),
    ];

    let (escrow_pda, bump) = Pubkey::find_program_address(seeds, program_id);

    // Validates that data address matches the account address
    if sell_order_data.seller != *seller_account.key {
        msg!("Invalid seller address");
        return Err(ProgramError::InvalidAccountData);
    }
    
    // Validate the program created the escrow account
    if escrow_account.key != &escrow_pda {
        msg!("escrow_account key, {:?}", escrow_account.key);
        msg!("escrow_account owner, {:?}", escrow_account.owner);
        msg!("program_id, {:?}", program_id);
        msg!("Invalid program ID");
        return Err(ProgramError::IncorrectProgramId)
    }
    // if escrow_account.owner != program_id {
    //     msg!("escrow_account key, {:?}", escrow_account.key);
    //     msg!("escrow_account owner, {:?}", escrow_account.owner);
    //     msg!("program_id, {:?}", program_id);
    //     msg!("Invalid program ID");
    //     return Err(ProgramError::IncorrectProgramId)
    // }
    
    
    // Derive PDA for the user
    if escrow_pda != *escrow_account.key {
        return Err(ProgramError::InvalidSeeds);
    }

    // Create order account
    let rent = Rent::get()?;
    let space = SellOrder::SIZE;
    let lamports = rent.minimum_balance(space);

    msg!("{}", space);

    invoke_signed(
        &system_instruction::create_account(
            authority_account.key,
            // seller_account.key,
            &escrow_pda,
            lamports,
            space as u64,
            program_id,
        ),
        &[
            authority_account.clone(),
            escrow_account.clone(),
            system_program.clone(),
        ],
        &[&[
            SellOrder::SEED_PREFIX,
            seller_account.key.as_ref(),
            &order_id.to_le_bytes(),
            &[bump],
        ]],
    )?;

    // Initialize order data
    let order_data = SellOrder {
        order_id,
        seller: *seller_account.key,
        amount: sell_order_data.amount,
        price: sell_order_data.price,
        escrow_account: escrow_pda,
        // token_mint: None,
        status: OrderStatus::Active,
    };

    order_data.serialize(&mut &mut escrow_account.data.borrow_mut()[..])?;

    counter_data.total_orders += 1;
    counter_data.serialize(&mut &mut counter_account.data.borrow_mut()[..])?;

    let _ = &invoke(
        &system_instruction::transfer(seller_account.key, escrow_account.key, sell_order_data.amount),
        &[
            seller_account.clone(),
            escrow_account.clone(),
        ]
    )?;


    Ok(())
}

pub fn initialize_counter(
    program_id: &Pubkey,
    accounts: &[AccountInfo]
) -> ProgramResult {
    let accounts_iterations = &mut accounts.iter();
    let authority = next_account_info(accounts_iterations)?;
    let counter_account = next_account_info(accounts_iterations)?;
    let system_program = next_account_info(accounts_iterations)?;

    let (counter_pda, bump) = Pubkey::find_program_address(
        &[OrderCounter::SEED_PREFIX],
        program_id
    );

    if counter_pda != *counter_account.key {
        return Err(ProgramError::InvalidSeeds);
    }

    if counter_account.data_is_empty() {
        let rent = Rent::get()?;
        let space = OrderCounter::SIZE;
        let lamports = rent.minimum_balance(space);

        solana_program::program::invoke_signed(
            &system_instruction::create_account(
                authority.key,
                &counter_pda,
                lamports,
                space as u64,
                program_id,
            ),
            &[
                authority.clone(),
                counter_account.clone(),
                system_program.clone(),
            ],
            &[&[OrderCounter::SEED_PREFIX, &[bump]]],
        )?;

        let counter_data = OrderCounter {
            total_orders: 0,
            authority: *authority.key,
        };

        counter_data.serialize(&mut &mut counter_account.data.borrow_mut()[..])?;
    }
    
    Ok(())
}


fn fulfil_buy_order(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    
    Ok(())
}