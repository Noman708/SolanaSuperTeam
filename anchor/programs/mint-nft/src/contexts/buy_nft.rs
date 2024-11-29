use anchor_lang::prelude::*;
use anchor_lang::system_program;
use anchor_spl::{
    token::{self, Token, TokenAccount, Mint},
    associated_token::AssociatedToken,
};
use crate::errors::*;
use crate::state::{ListingAccount, MarketplaceConfig};

#[derive(Accounts)]
pub struct BuyNft<'info> {
    #[account(mut)]
    pub buyer: Signer<'info>,
    
    #[account(mut)]
    pub seller: SystemAccount<'info>,
    
    #[account(
        mut,
        seeds = [b"marketplace"],
        bump,
        constraint = !config.is_paused @ MarketplaceError::MarketplacePaused
    )]
    pub config: Account<'info, MarketplaceConfig>,
    
    #[account(
        mut,
        seeds = [b"listing", nft_mint.key().as_ref()],
        bump = listing_account.bump,
        close = seller,
        has_one = seller,
        constraint = listing_account.is_active @ MarketplaceError::ListingNotActive
    )]
    pub listing_account: Account<'info, ListingAccount>,
    
    #[account(mut)]
    pub nft_mint: Account<'info, Mint>,
    
    #[account(
        mut,
        associated_token::mint = nft_mint,
        associated_token::authority = seller
    )]
    pub seller_token_account: Account<'info, TokenAccount>,
    
    #[account(
        mut,
        associated_token::mint = nft_mint,
        associated_token::authority = listing_account,
        constraint = escrow_token_account.key() == listing_account.escrow_token_account
    )]
    pub escrow_token_account: Account<'info, TokenAccount>,
    
    #[account(
        init_if_needed,
        payer = buyer,
        associated_token::mint = nft_mint,
        associated_token::authority = buyer
    )]
    pub buyer_token_account: Account<'info, TokenAccount>,
    
    #[account(mut)]
    pub treasury_wallet: SystemAccount<'info>,
    
    #[account(mut)]
    pub creator_wallet: SystemAccount<'info>, // Add creator wallet

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub rent: Sysvar<'info, Rent>,
}

impl<'info> BuyNft<'info> {
    pub fn buy_nft(&mut self) -> Result<()> {
        // Check seller & balance
        require!(
            self.buyer.lamports() >= self.listing_account.price,
            MarketplaceError::InsufficientBalance
        );
        require!(
            self.buyer.key() != self.seller.key(),
            MarketplaceError::CannotBuyOwnNFT
        );

        let listing_price = self.listing_account.price;
        let fee_percentage = self.config.fee_percentage;

        // Calculate marketplace fee
        let fee_amount = (listing_price as u128)
            .checked_mul(fee_percentage as u128)
            .unwrap()
            .checked_div(10000)
            .unwrap() as u64;

        // Calculate royalty amount (example: 5%)
        let royalty_percentage = 500; // 5%
        let royalty_amount = (listing_price as u128)
            .checked_mul(royalty_percentage as u128)
            .unwrap()
            .checked_div(10000)
            .unwrap() as u64;

        // Calculate final seller amount after fees and royalties
        let seller_amount = listing_price
            .checked_sub(fee_amount)
            .unwrap()
            .checked_sub(royalty_amount)
            .unwrap();

        // Transfer SOL to seller
        system_program::transfer(
            CpiContext::new(
                self.system_program.to_account_info(),
                system_program::Transfer {
                    from: self.buyer.to_account_info(),
                    to: self.seller.to_account_info(),
                }
            ),
            seller_amount,
        )?;

        // Transfer marketplace fee
        system_program::transfer(
            CpiContext::new(
                self.system_program.to_account_info(),
                system_program::Transfer {
                    from: self.buyer.to_account_info(),
                    to: self.treasury_wallet.to_account_info(),
                }
            ),
            fee_amount,
        )?;

        // Transfer royalties to creator(s)
        // Note: You'll need to add creator accounts to the context
        system_program::transfer(
            CpiContext::new(
                self.system_program.to_account_info(),
                system_program::Transfer {
                    from: self.buyer.to_account_info(),
                    to: self.creator_wallet.to_account_info(), // Add this to context
                }
            ),
            royalty_amount,
        )?;

        // Transfer NFT
        token::transfer(
            CpiContext::new_with_signer(
                self.token_program.to_account_info(),
                token::Transfer {
                    from: self.escrow_token_account.to_account_info(),
                    to: self.buyer_token_account.to_account_info(),
                    authority: self.listing_account.to_account_info(),
                },
                &[&[
                    b"listing",
                    self.nft_mint.key().as_ref(),
                    &[self.listing_account.bump]
                ]]
            ),
            1,
        )?;

        Ok(())
    }
}