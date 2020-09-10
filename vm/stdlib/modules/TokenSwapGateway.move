address 0x1 {
module TokenSwapGateway {
    use 0x1::TokenSwap;
    use 0x1::LiquidityToken::LiquidityToken;
    use 0x1::TokenSwapHelper;
    use 0x1::Account;
    use 0x1::Signer;
    use 0x1::Token;

    // use 0x1::Debug;
    const INSUFFICIENT_X_AMOUNT: u64 = 1010;
    const INSUFFICIENT_Y_AMOUNT: u64 = 1011;

    public fun liquidity<X, Y>(account: address): u128 {
        let order = TokenSwap::compare_token<X, Y>();
        assert(order != 0, 1000);
        if (order == 1) {
            Account::balance<LiquidityToken<X, Y>>(account)
        } else {
            Account::balance<LiquidityToken<Y, X>>(account)
        }
    }

    public fun total_liquidity<X, Y>(): u128 {
        let order = TokenSwap::compare_token<X, Y>();
        assert(order != 0, 1000);
        if (order == 1) {
            Token::market_cap<LiquidityToken<X, Y>>()
        } else {
            Token::market_cap<LiquidityToken<Y, X>>()
        }
    }

    public fun add_liquidity<X, Y>(
        signer: &signer,
        amount_x_desired: u128,
        amount_y_desired: u128,
        amount_x_min: u128,
        amount_y_min: u128,
    ) {
        let order = TokenSwap::compare_token<X, Y>();
        assert(order != 0, 1000);
        if (order == 1) {
            _add_liquidity<X, Y>(
                signer,
                amount_x_desired,
                amount_y_desired,
                amount_x_min,
                amount_y_min,
            );
        } else {
            _add_liquidity<Y, X>(
                signer,
                amount_y_desired,
                amount_x_desired,
                amount_y_min,
                amount_x_min,
            );
        }
    }

    fun _add_liquidity<X, Y>(
        signer: &signer,
        amount_x_desired: u128,
        amount_y_desired: u128,
        amount_x_min: u128,
        amount_y_min: u128,
    ) {
        let (amount_x, amount_y) = _calculate_amount_for_liquidity<X, Y>(
            amount_x_desired,
            amount_y_desired,
            amount_x_min,
            amount_y_min,
        );
        let x_token = Account::withdraw<X>(signer, amount_x);
        let y_token = Account::withdraw<Y>(signer, amount_y);
        let liquidity_token = TokenSwap::mint(x_token, y_token);
        if (!Account::is_accepts_token<LiquidityToken<X, Y>>(Signer::address_of(signer))) {
            Account::accept_token<LiquidityToken<X, Y>>(signer);
        };
        Account::deposit(signer, liquidity_token);
    }

    fun _calculate_amount_for_liquidity<X, Y>(
        amount_x_desired: u128,
        amount_y_desired: u128,
        amount_x_min: u128,
        amount_y_min: u128,
    ): (u128, u128) {
        let (reserve_x, reserve_y) = TokenSwap::get_reserves<X, Y>();
        if (reserve_x == 0 && reserve_y == 0) {
            return (amount_x_desired, amount_y_desired)
        } else {
            let amount_y_optimal = TokenSwapHelper::quote(amount_x_desired, reserve_x, reserve_y);
            if (amount_y_optimal <= amount_y_desired) {
                assert(amount_y_optimal >= amount_y_min, INSUFFICIENT_Y_AMOUNT);
                return (amount_x_desired, amount_y_optimal)
            } else {
                let amount_x_optimal = TokenSwapHelper::quote(
                    amount_y_desired,
                    reserve_y,
                    reserve_x,
                );
                assert(amount_x_optimal <= amount_x_desired, 1000);
                assert(amount_x_optimal >= amount_x_min, INSUFFICIENT_X_AMOUNT);
                return (amount_x_optimal, amount_y_desired)
            }
        }
    }

    public fun remove_liquidity<X, Y>(
        signer: &signer,
        liquidity: u128,
        amount_x_min: u128,
        amount_y_min: u128,
    ) {
        let order = TokenSwap::compare_token<X, Y>();
        assert(order != 0, 1000);
        if (order == 1) {
            _remove_liquidity<X, Y>(signer, liquidity, amount_x_min, amount_y_min);
        } else {
            _remove_liquidity<Y, X>(signer, liquidity, amount_y_min, amount_x_min);
        }
    }

    fun _remove_liquidity<X, Y>(
        signer: &signer,
        liquidity: u128,
        amount_x_min: u128,
        amount_y_min: u128,
    ) {
        let liquidity_token = Account::withdraw<LiquidityToken<X, Y>>(signer, liquidity);
        let (token_x, token_y) = TokenSwap::burn(liquidity_token);
        assert(Token::value(&token_x) >= amount_x_min, 1000);
        assert(Token::value(&token_y) >= amount_y_min, 1000);
        Account::deposit(signer, token_x);
        Account::deposit(signer, token_y);
    }

    public fun swap_exact_token_for_token<X, Y>(
        signer: &signer,
        amount_x_in: u128,
        amount_y_out_min: u128,
    ) {
        let order = TokenSwap::compare_token<X, Y>();
        assert(order != 0, 1000);
        // calculate actual y out
        let (reserve_x, reserve_y) = TokenSwap::get_reserves<X, Y>();
        let y_out = TokenSwapHelper::get_amount_out(amount_x_in, reserve_x, reserve_y);
        assert(y_out >= amount_y_out_min, 4000);
        // do actual swap
        let token_x = Account::withdraw<X>(signer, amount_x_in);
        let (token_x_out, token_y_out);
        if (order == 1) {
            (token_x_out, token_y_out) = TokenSwap::swap<X, Y>(token_x, y_out, Token::zero(), 0);
        } else {
            (token_y_out, token_x_out) = TokenSwap::swap<Y, X>(Token::zero(), 0, token_x, y_out);
        };
        Token::destroy_zero(token_x_out);
        Account::deposit(signer, token_y_out);
    }

    public fun swap_token_for_exact_token<X, Y>(
        signer: &signer,
        amount_x_in_max: u128,
        amount_y_out: u128,
    ) {
        let order = TokenSwap::compare_token<X, Y>();
        assert(order != 0, 1000);
        // calculate actual y out
        let (reserve_x, reserve_y) = TokenSwap::get_reserves<X, Y>();
        let x_in = TokenSwapHelper::get_amount_in(amount_y_out, reserve_x, reserve_y);
        assert(x_in <= amount_x_in_max, 4000);
        // do actual swap
        let token_x = Account::withdraw<X>(signer, x_in);
        let (token_x_out, token_y_out);
        if (order == 1) {
            (token_x_out, token_y_out) =
                TokenSwap::swap<X, Y>(token_x, amount_y_out, Token::zero(), 0);
        } else {
            (token_y_out, token_x_out) =
                TokenSwap::swap<Y, X>(Token::zero(), 0, token_x, amount_y_out);
        };
        Token::destroy_zero(token_x_out);
        Account::deposit(signer, token_y_out);
    }
}
}