use solana_program::{
    account_info::{next_account_info, AccountInfo}, entrypoint::ProgramResult, entrypoint, msg, native_token::LAMPORTS_PER_SOL, program::{invoke, invoke_signed}, program_error::ProgramError, pubkey::Pubkey, rent::Rent, system_instruction, system_program, sysvar::Sysvar
};

use spl_token::instruction::transfer as spl_token_transfer;

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

#[derive(BorshDeserialize, BorshSerialize, Debug)]
pub struct BuyOrder {
    pub order_id: u64, //8
    pub buyer: Pubkey, //32
    pub escrow_account: Pubkey, //32
    pub amount: u64,//8
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
    pub const SEED_PREFIX: &'static [u8] = b"order_counter_v2";
    pub const SIZE: usize = 8 + 32; // u64 + Pubkey
}

impl SellOrder {
    pub const SEED_PREFIX: &'static [u8] = b"escrow_order_v2";
    pub const SEED_TOKEN_PREFIX: &'static [u8] = b"escrow_token_order_v2";
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
    let escrow_sol_token_account = next_account_info(account_iterations)?;

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

    let seeds = &[
        SellOrder::SEED_TOKEN_PREFIX,
        escrow_pda.as_ref(),
    ];

    let (escrow_sol_token_pda, escrow_sol_token_bump) = Pubkey::find_program_address(seeds, program_id);

    // Validates that data address matches the account address
    if sell_order_data.seller != *seller_account.key {
        msg!("Invalid seller address");
        return Err(ProgramError::InvalidAccountData);
    }
    
    // Validate the program created the escrow account
    if escrow_account.key != &escrow_pda {
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

    // Derive PDA for the user
    if escrow_sol_token_pda != *escrow_sol_token_account.key {
        msg!("Escrow token account does not match");
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

    // create sol token account
    let space = 0;
    let lamports = rent.minimum_balance(space);
    let signer_seeds = &[
        SellOrder::SEED_TOKEN_PREFIX,
        escrow_pda.as_ref(),
        &[escrow_sol_token_bump]
    ];

    invoke_signed(
        &system_instruction::create_account(
            authority_account.key,
            &escrow_sol_token_pda,
            lamports,
            space as u64,
            &system_program::ID,
        ),
        &[
            authority_account.clone(),
            escrow_sol_token_account.clone(),
            system_program.clone(),
        ],
        &[signer_seeds],
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
        &system_instruction::transfer(seller_account.key, escrow_sol_token_account.key, sell_order_data.amount),
        &[
            seller_account.clone(),
            escrow_sol_token_account.clone(),
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
    _instruction_data: &[u8],
) -> ProgramResult {
    // const USDC_TOKEN_MINT: &str = "Gh9ZwEmdLJ8DscKNTkTqPbNwLNNBjuSzaG9Vp2KGtKJr";
    // const USDC_TOKEN_MINT: &str = "4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU";
    // const USDC_DECIMAL: u64 = 1000000;
    let accounts_iterations = &mut accounts.iter();

    let authority = next_account_info(accounts_iterations)?;
    let token_program_account = next_account_info(accounts_iterations)?;
    let system_program = next_account_info(accounts_iterations)?;
    let seller_account = next_account_info(accounts_iterations)?;
    let buyer_account = next_account_info(accounts_iterations)?;
    let escrow_account = next_account_info(accounts_iterations)?;
    let seller_associated_token_account = next_account_info(accounts_iterations)?;
    let buyer_associated_token_account = next_account_info(accounts_iterations)?;
    let escrow_sol_token_account = next_account_info(accounts_iterations)?;

    let buy_order_data = BuyOrder::try_from_slice(_instruction_data).map_err(|err| {
        msg!("Error Deseriealizing order, {:?}", err);
        ProgramError::InvalidInstructionData
    })?;

    let mut escrow_account_data = SellOrder::try_from_slice(&escrow_account.data.borrow()).map_err(|err| {
        msg!("Error Deseriealizing order, {:?}", err);
        ProgramError::InvalidInstructionData
    })?;

    if &buy_order_data.escrow_account != escrow_account.key {
        msg!("Escrow account does not match instruction");
        return Err(ProgramError::InvalidInstructionData);
    }

    if &buy_order_data.buyer != buyer_account.key {
        msg!("Buyer address doe not match instruction");
        return Err(ProgramError::InvalidInstructionData); 
    }

    if buy_order_data.amount < escrow_account_data.amount {
        msg!("Error Insufficient funds");
        return Err(ProgramError::InsufficientFunds);
    }

    if escrow_account_data.order_id != buy_order_data.order_id {
        msg!("Order Id on account does not match orer id in instruction");
        return Err(ProgramError::InvalidArgument);
    }

    // if escrow_account.owner != program_id { 
    if escrow_account.owner != program_id {
        msg!("account is not owned by program");
        return Err(ProgramError::IncorrectProgramId);
    }

    let seeds = &[
        SellOrder::SEED_PREFIX,
        seller_account.key.as_ref(),
        &buy_order_data.order_id.to_le_bytes(),
    ];

    let (escrow_pda, escrow_pda_bump) = Pubkey::find_program_address(seeds, program_id);

    if escrow_account.key != &escrow_pda {
        msg!("Escrow account does not match derived escrow account");
        return Err(ProgramError::InvalidSeeds);
    }

    let seeds = &[
        SellOrder::SEED_TOKEN_PREFIX,
        escrow_pda.as_ref(),
    ];

    let (escrow_sol_token_address, bump) = Pubkey::find_program_address(seeds, program_id);
          
    let signer_seeds  = &[
        SellOrder::SEED_TOKEN_PREFIX,
        escrow_pda.as_ref(),
        &[bump]
    ];

    let usdc_to_transfer_in_decimals = (escrow_account_data.amount * escrow_account_data.price) / LAMPORTS_PER_SOL;
    // let usdc_to_transfer_in_decimals = ((escrow_account_data.amount * escrow_account_data.price) / LAMPORTS_PER_SOL) * USDC_DECIMAL;

    let transfer_to_seller_ix = spl_token_transfer(
        &spl_token::id(), // Token program ID (usually "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA")
        buyer_associated_token_account.key, // Source token account
        seller_associated_token_account.key, // Destination token account
        buyer_account.key, // Authority (signer)
        &[], // Signers (for multisig accounts, otherwise empty)
        usdc_to_transfer_in_decimals, // Amount of tokens to transfer
    )?;

    // Transfer USDC to seller
    invoke(
        &transfer_to_seller_ix,
        &[
            buyer_associated_token_account.clone(),
            seller_associated_token_account.clone(),
            buyer_account.clone(),
            token_program_account.clone()
        ]
    )?;

    let sol_to_transfer_in_lamports = escrow_account_data.amount;

    //Manual trasfer logic
    // if escrow_account.lamports() < sol_to_transfer_in_lamports {
    //     return Err(ProgramError::InsufficientFunds);
    // }

    // // Subtract lamports from source
    // **escrow_account.lamports.borrow_mut() -= sol_to_transfer_in_lamports;
    
    // // Add lamports to destination
    // **buyer_account.lamports.borrow_mut() += sol_to_transfer_in_lamports;

    if escrow_sol_token_account.key != &escrow_sol_token_address {
        msg!("Escrow token account derived do not match");
        return Err(ProgramError::InvalidArgument)
    }

    let transfer_to_buyer_ix = &system_instruction::transfer(
        &escrow_sol_token_account.key,
        buyer_account.key,
        sol_to_transfer_in_lamports
    );

    //Clear here

    // Transfer SOl to buyer
    invoke_signed(
        transfer_to_buyer_ix,
        &[
            escrow_sol_token_account.clone(),
            authority.clone(),
            buyer_account.clone(),
            system_program.clone(),
        ],
        &[signer_seeds],
        // &[seeds, &[&[bump]]],
    )?;

    escrow_account_data.status = OrderStatus::Completed;
    let _ = escrow_account_data.serialize(&mut &mut escrow_account.data.borrow_mut()[..]);


    // Step 1: Transfer remaining SOL from the PDA to the authority account
    let escrow_account_balance = **escrow_account.lamports.borrow();
    let escrow_sol_token_account_balance = **escrow_sol_token_account.lamports.borrow();
    
    // closing escrow sol token account
    if escrow_sol_token_account_balance > 0 {
        msg!("Transferring {} lamports to authority", escrow_sol_token_account_balance);

        // **escrow_account.lamports.borrow_mut() -= escrow_account_balance;

        // **authority.lamports.borrow_mut() += escrow_account_balance;

        let transfer_instruction = system_instruction::transfer(
            &escrow_sol_token_account.key,    // From PDA account
            authority.key,     // To authority account
            escrow_sol_token_account_balance,               // Amount to transfer
        );

        invoke_signed(
            &transfer_instruction,
            &[
                escrow_sol_token_account.clone(),
                authority.clone(),
                system_program.clone(),
            ],
            &[signer_seeds], // PDA seeds for signing
            // &[seeds, &[&[bump]]], // PDA seeds for signing
        )?
        ;
    }
    // Set the PDA's lamports to 0 and assign ownership to the System Program
    **escrow_sol_token_account.lamports.borrow_mut() = 0;
    escrow_sol_token_account.assign(&system_program::ID);

    // let signer_seeds = &[
    //     SellOrder::SEED_PREFIX,
    //     seller_account.key.as_ref(),
    //     &buy_order_data.order_id.to_le_bytes(),
    //     &[escrow_pda_bump]
    // ];

    // closing escrow data account
    if escrow_account_balance > 0 {
        msg!("Transferring {} lamports to authority", escrow_account_balance);

        //Manual deduction because the program is the owner of the account
        **escrow_account.lamports.borrow_mut() -= escrow_account_balance;

        **authority.lamports.borrow_mut() += escrow_account_balance;

        // let transfer_instruction = system_instruction::transfer(
        //     &escrow_account.key,    // From PDA account
        //     authority.key,     // To authority account
        //     escrow_account_balance,               // Amount to transfer
        // );

        // invoke_signed(
        //     &transfer_instruction,
        //     &[
        //         escrow_account.clone(),
        //         authority.clone(),
        //         system_program.clone(),
        //     ],
        //     &[signer_seeds], // PDA seeds for signing
        //     // &[seeds, &[&[bump]]], // PDA seeds for signing
        // )?;
    }
 
     // Step 2: Close the PDA account
     msg!("Closing the PDA account");
 
     // Set the PDA's lamports to 0 and assign ownership to the System Program
     **escrow_account.lamports.borrow_mut() = 0;
    //  **authority.lamports.borrow_mut() += escrow_account_balance;
     escrow_account.try_borrow_mut_data()?.fill(0); // Clear any account data
     escrow_account.assign(&system_program::ID); // Assign ownership to the system program
 
     msg!("PDA account successfully closed");

    Ok(())
}



// fn close_pda_account(
//     program_id: &Pubkey,
//     accounts: &[AccountInfo],
//     seeds: &[&[u8]], 
// ) -> ProgramResult {
    
//     let escrow_account_balance = **escrow_account.lamports.borrow();
//     let seeds = &[
//         SellOrder::SEED_PREFIX,
//         seller_account.key.as_ref(),
//         &escrow_account_data.order_id.to_le_bytes(),
//     ];

//     if escrow_account_balance > 0 {
//         msg!("Transferring {} lamports to authority", escrow_account_balance);

//         let transfer_instruction = system_instruction::transfer(
//             escrow_account.key,    // From PDA account
//             authority.key,     // To authority account
//             escrow_account_balance,               // Amount to transfer
//         );

//         invoke_signed(
//             &transfer_instruction,
//             &[
//             escrow_account.clone(),
//                 authority.clone(),
//                 system_program.clone(),
//             ],
//             &[seeds], // PDA seeds for signing
//         )?;
//     }
 
//      // Step 2: Close the PDA account
//      msg!("Closing the PDA account");
 
//      // Set the PDA's lamports to 0 and assign ownership to the System Program
//      **escrow_account.lamports.borrow_mut() = 0;
//     //  **authority.lamports.borrow_mut() += escrow_account_balance;
//      escrow_account.try_borrow_mut_data()?.fill(0); // Clear any account data
//      escrow_account.assign(&system_program::ID); // Assign ownership to the system program
 
//      msg!("PDA account successfully closed");
//     Ok(())
// }