//! Centralized Contract Definitions
//!
//! All Solidity contract interfaces for the DEX arbitrage bot,
//! defined using alloy's `sol!` macro (replaces ethers-rs `abigen!`).
//!
//! Each interface is annotated with `#[sol(rpc)]` to generate
//! contract instance types that can make RPC calls via any alloy Provider.
//!
//! Author: AI-Generated
//! Created: 2026-02-01
//! Modified: 2026-02-01 — initial migration from ethers-rs abigen!

use alloy::sol;

// ── ERC20 ─────────────────────────────────────────────────────────────

sol! {
    #[sol(rpc)]
    interface IERC20 {
        function approve(address spender, uint256 amount) external returns (bool);
        function allowance(address owner, address spender) external view returns (uint256);
        function balanceOf(address account) external view returns (uint256);
        function decimals() external view returns (uint8);
    }
}

// ── Uniswap V2 ───────────────────────────────────────────────────────

sol! {
    #[sol(rpc)]
    interface IUniswapV2Factory {
        function getPair(address tokenA, address tokenB) external view returns (address pair);
        function allPairs(uint256) external view returns (address pair);
        function allPairsLength() external view returns (uint256);
    }
}

sol! {
    #[sol(rpc)]
    interface IUniswapV2Pair {
        function getReserves() external view returns (uint112 reserve0, uint112 reserve1, uint32 blockTimestampLast);
        function token0() external view returns (address);
        function token1() external view returns (address);
    }
}

sol! {
    #[sol(rpc)]
    interface IUniswapV2Router02 {
        function swapExactTokensForTokens(uint256 amountIn, uint256 amountOutMin, address[] calldata path, address to, uint256 deadline) external returns (uint256[] memory amounts);
        function getAmountsOut(uint256 amountIn, address[] calldata path) external view returns (uint256[] memory amounts);
    }
}

// ── Uniswap V3 ───────────────────────────────────────────────────────

sol! {
    #[sol(rpc)]
    interface UniswapV3Factory {
        function getPool(address tokenA, address tokenB, uint24 fee) external view returns (address pool);
    }
}

sol! {
    #[sol(rpc)]
    interface UniswapV3Pool {
        function slot0() external view returns (uint160 sqrtPriceX96, int24 tick, uint16 observationIndex, uint16 observationCardinality, uint16 observationCardinalityNext, uint8 feeProtocol, bool unlocked);
        function liquidity() external view returns (uint128);
        function fee() external view returns (uint24);
        function token0() external view returns (address);
        function token1() external view returns (address);
    }
}

sol! {
    #[sol(rpc)]
    interface ISwapRouter {
        struct ExactInputSingleParams {
            address tokenIn;
            address tokenOut;
            uint24 fee;
            address recipient;
            uint256 deadline;
            uint256 amountIn;
            uint256 amountOutMinimum;
            uint160 sqrtPriceLimitX96;
        }

        function exactInputSingle(ExactInputSingleParams calldata params) external payable returns (uint256 amountOut);
    }
}

sol! {
    #[sol(rpc)]
    interface IQuoter {
        function quoteExactInputSingle(address tokenIn, address tokenOut, uint24 fee, uint256 amountIn, uint160 sqrtPriceLimitX96) external returns (uint256 amountOut);
    }
}

sol! {
    #[sol(rpc)]
    interface IQuoterV2 {
        struct QuoteExactInputSingleParams {
            address tokenIn;
            address tokenOut;
            uint256 amountIn;
            uint24 fee;
            uint160 sqrtPriceLimitX96;
        }

        function quoteExactInputSingle(QuoteExactInputSingleParams memory params) external returns (uint256 amountOut, uint160 sqrtPriceX96After, uint32 initializedTicksCrossed, uint256 gasEstimate);
    }
}

// ── QuickSwap V3 (Algebra) ───────────────────────────────────────────

sol! {
    #[sol(rpc)]
    interface AlgebraPool {
        function globalState() external view returns (uint160 price, int24 tick, uint16 fee, uint16 timepointIndex, uint8 communityFeeToken0, uint8 communityFeeToken1, bool unlocked);
        function liquidity() external view returns (uint128);
        function token0() external view returns (address);
        function token1() external view returns (address);
    }
}

sol! {
    #[sol(rpc)]
    interface IAlgebraSwapRouter {
        struct ExactInputSingleParams {
            address tokenIn;
            address tokenOut;
            address recipient;
            uint256 deadline;
            uint256 amountIn;
            uint256 amountOutMinimum;
            uint160 limitSqrtPrice;
        }

        function exactInputSingle(ExactInputSingleParams calldata params) external payable returns (uint256 amountOut);
    }
}

sol! {
    #[sol(rpc)]
    interface IAlgebraQuoter {
        function quoteExactInputSingle(address tokenIn, address tokenOut, uint256 amountIn, uint160 limitSqrtPrice) external returns (uint256 amountOut, uint16 fee);
    }
}

// ── ArbExecutor (custom atomic arb contract) ─────────────────────────

sol! {
    #[sol(rpc)]
    interface IArbExecutor {
        function executeArb(address token0, address token1, address routerBuy, address routerSell, uint24 feeBuy, uint24 feeSell, uint256 amountIn, uint256 minProfit) external returns (uint256 profit);
    }
}
