// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title ArbExecutor — Atomic DEX Arbitrage (V3 + V2 cross-protocol)
/// @notice Executes two swaps atomically in a single transaction.
///         Supports V3↔V3, V2↔V3, and V2↔V2 arbitrage — all zero leg risk.
///         If either swap fails or net profit < minProfit, the entire tx reverts.
///
/// @dev Supported router types (routed via fee parameter sentinel):
///   - fee = 0                → Algebra SwapRouter (QuickSwap V3, dynamic fees)
///   - fee = 1..65535         → Standard V3 SwapRouter (Uniswap V3, SushiSwap V3)
///   - fee = type(uint24).max → V2 Router (QuickSwap V2, SushiSwap V2, swapExactTokensForTokens)
///
/// @dev Token flow:
///   1. transferFrom(caller, this, amountIn)          — pull input tokens
///   2. Router A: swap(token0→token1)                  — buy leg (V2 or V3)
///   3. Router B: swap(token1→token0)                  — sell leg (V2 or V3)
///   4. require(balance >= amountIn + minProfit)        — profit check
///   5. transfer(caller, balance)                       — return all tokens
///
///   If any step fails, the entire transaction reverts — zero risk.
///
/// @author AI-Generated
/// @custom:created 2026-01-30
/// @custom:modified 2026-01-30 (V2: Algebra SwapRouter support via fee=0 sentinel)
/// @custom:modified 2026-01-30 (V3: V2 router support via fee=type(uint24).max sentinel)

import {IERC20} from "forge-std/interfaces/IERC20.sol";

/// @notice Minimal Uniswap V3 SwapRouter interface (exactInputSingle)
///         Works for both Uniswap V3 and SushiSwap V3 (same ABI)
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

    function exactInputSingle(ExactInputSingleParams calldata params)
        external
        payable
        returns (uint256 amountOut);
}

/// @notice Minimal Algebra SwapRouter interface (QuickSwap V3)
///         No fee parameter — Algebra uses dynamic fees, single pool per pair.
///         Uses limitSqrtPrice instead of sqrtPriceLimitX96.
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

    function exactInputSingle(ExactInputSingleParams calldata params)
        external
        payable
        returns (uint256 amountOut);
}

/// @notice Minimal Uniswap V2 Router interface (swapExactTokensForTokens)
///         Works for QuickSwap V2, SushiSwap V2, and all Uniswap V2 forks.
interface IUniswapV2Router02 {
    function swapExactTokensForTokens(
        uint256 amountIn,
        uint256 amountOutMin,
        address[] calldata path,
        address to,
        uint256 deadline
    ) external returns (uint256[] memory amounts);
}

contract ArbExecutor {
    /// @notice Contract owner (only address that can execute arbs and rescue tokens)
    address public immutable owner;

    /// @notice Emitted on successful arbitrage execution
    event ArbExecuted(
        address indexed token0,
        address indexed token1,
        uint256 amountIn,
        uint256 amountOut,
        uint256 profit,
        address routerBuy,
        address routerSell
    );

    /// @notice Emitted when tokens are rescued by the owner
    event TokensRescued(address indexed token, uint256 amount);

    /// @notice Fee sentinel: type(uint24).max signals V2 router (swapExactTokensForTokens)
    ///         fee=0 → Algebra, fee=1..65535 → standard V3, fee=16777215 → V2
    uint24 public constant FEE_V2_SENTINEL = type(uint24).max; // 16777215

    error OnlyOwner();
    error InsufficientProfit(uint256 got, uint256 required);
    error ZeroAmount();

    modifier onlyOwner() {
        if (msg.sender != owner) revert OnlyOwner();
        _;
    }

    constructor() {
        owner = msg.sender;
    }

    /// @notice Execute an atomic two-leg arbitrage
    /// @param token0      The base token (e.g., USDC) — start and end with this
    /// @param token1      The intermediate token (e.g., WETH) — held only within this tx
    /// @param routerBuy   SwapRouter address for buy leg (token0→token1)
    /// @param routerSell  SwapRouter address for sell leg (token1→token0)
    /// @param feeBuy      Fee routing: 0 → Algebra, 1-65535 → V3 fee tier, 16777215 → V2 router
    /// @param feeSell     Fee routing: 0 → Algebra, 1-65535 → V3 fee tier, 16777215 → V2 router
    /// @param amountIn    Amount of token0 to use
    /// @param minProfit   Minimum profit in token0 units (revert if not met)
    /// @return profit     Net profit in token0 units
    function executeArb(
        address token0,
        address token1,
        address routerBuy,
        address routerSell,
        uint24 feeBuy,
        uint24 feeSell,
        uint256 amountIn,
        uint256 minProfit
    ) external onlyOwner returns (uint256 profit) {
        if (amountIn == 0) revert ZeroAmount();

        // 1. Pull token0 from caller
        require(IERC20(token0).transferFrom(msg.sender, address(this), amountIn), "transferFrom failed");

        // 2. Approve buy router for token0
        IERC20(token0).approve(routerBuy, amountIn);

        // 3. Buy leg: token0 → token1 on routerBuy
        //    fee=0 sentinel → Algebra router (no fee param), else standard V3 router
        uint256 token1Received = _swapSingle(
            routerBuy, feeBuy, token0, token1, amountIn, 0
        );

        // 4. Approve sell router for token1
        IERC20(token1).approve(routerSell, token1Received);

        // 5. Sell leg: token1 → token0 on routerSell
        uint256 token0Received = _swapSingle(
            routerSell, feeSell, token1, token0, token1Received, amountIn
        );

        // 6. Profit check — revert if below threshold
        if (token0Received < amountIn + minProfit) {
            revert InsufficientProfit(token0Received - amountIn, minProfit);
        }
        profit = token0Received - amountIn;

        // 7. Return all token0 to caller
        require(IERC20(token0).transfer(msg.sender, token0Received), "transfer out failed");

        emit ArbExecuted(
            token0, token1, amountIn, token0Received, profit, routerBuy, routerSell
        );
    }

    /// @dev Route a single swap based on the fee parameter sentinel:
    ///      fee = FEE_V2_SENTINEL (type(uint24).max) → V2 swapExactTokensForTokens
    ///      fee = 0 → Algebra SwapRouter (QuickSwap V3, no fee in params)
    ///      fee = 1..65535 → Standard V3 SwapRouter (Uniswap V3 / SushiSwap V3)
    function _swapSingle(
        address router,
        uint24 fee,
        address tokenIn,
        address tokenOut,
        uint256 amountIn,
        uint256 amountOutMin
    ) internal returns (uint256 amountOut) {
        if (fee == FEE_V2_SENTINEL) {
            // V2 Router (QuickSwap V2 / SushiSwap V2) — swapExactTokensForTokens
            address[] memory path = new address[](2);
            path[0] = tokenIn;
            path[1] = tokenOut;
            uint256[] memory amounts = IUniswapV2Router02(router).swapExactTokensForTokens(
                amountIn,
                amountOutMin,
                path,
                address(this),
                block.timestamp + 120
            );
            amountOut = amounts[amounts.length - 1];
        } else if (fee == 0) {
            // Algebra SwapRouter (QuickSwap V3) — no fee parameter
            amountOut = IAlgebraSwapRouter(router).exactInputSingle(
                IAlgebraSwapRouter.ExactInputSingleParams({
                    tokenIn: tokenIn,
                    tokenOut: tokenOut,
                    recipient: address(this),
                    deadline: block.timestamp + 120,
                    amountIn: amountIn,
                    amountOutMinimum: amountOutMin,
                    limitSqrtPrice: 0
                })
            );
        } else {
            // Standard V3 SwapRouter (Uniswap V3 / SushiSwap V3) — fee in params
            amountOut = ISwapRouter(router).exactInputSingle(
                ISwapRouter.ExactInputSingleParams({
                    tokenIn: tokenIn,
                    tokenOut: tokenOut,
                    fee: fee,
                    recipient: address(this),
                    deadline: block.timestamp + 120,
                    amountIn: amountIn,
                    amountOutMinimum: amountOutMin,
                    sqrtPriceLimitX96: 0
                })
            );
        }
    }

    /// @notice Rescue any tokens stuck in the contract (owner only)
    /// @param token The ERC20 token address
    function rescueTokens(address token) external onlyOwner {
        uint256 balance = IERC20(token).balanceOf(address(this));
        if (balance > 0) {
            require(IERC20(token).transfer(owner, balance), "rescue transfer failed");
            emit TokensRescued(token, balance);
        }
    }

    /// @notice Rescue native ETH/MATIC stuck in the contract (owner only)
    function rescueNative() external onlyOwner {
        uint256 balance = address(this).balance;
        if (balance > 0) {
            (bool ok, ) = owner.call{value: balance}("");
            require(ok, "Native transfer failed");
        }
    }

    /// @notice Accept native ETH/MATIC (in case it's sent accidentally)
    receive() external payable {}
}
