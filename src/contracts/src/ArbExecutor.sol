// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title ArbExecutor — Atomic DEX Arbitrage (V2: Algebra support)
/// @notice Executes two V3 swaps atomically in a single transaction.
///         If the second swap fails or net profit < minProfit, the entire tx reverts.
///         Supports Uniswap V3, SushiSwap V3 (same ABI), and QuickSwap V3 (Algebra, different ABI).
///
/// @dev Token flow:
///   1. transferFrom(caller, this, amountIn)          — pull input tokens
///   2. SwapRouter A: exactInputSingle(token0→token1)  — buy leg
///   3. SwapRouter B: exactInputSingle(token1→token0)  — sell leg
///   4. require(balance >= amountIn + minProfit)        — profit check
///   5. transfer(caller, balance)                       — return all tokens
///
///   If any step fails, the entire transaction reverts — zero risk.
///
///   Algebra (QuickSwap V3) routing: fee=0 signals Algebra SwapRouter
///   (no fee parameter in ExactInputSingleParams). Otherwise uses standard V3 ABI.
///
/// @author AI-Generated
/// @custom:created 2026-01-30
/// @custom:modified 2026-01-30 (V2: Algebra SwapRouter support via fee=0 sentinel)

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
    /// @param feeBuy      V3 fee tier for buy pool (e.g., 500 = 0.05%). fee=0 → Algebra router (no fee param)
    /// @param feeSell     V3 fee tier for sell pool (e.g., 3000 = 0.30%). fee=0 → Algebra router (no fee param)
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

    /// @dev Route a single swap to either standard V3 SwapRouter or Algebra SwapRouter
    ///      based on the fee parameter. fee=0 → Algebra (no fee in params).
    function _swapSingle(
        address router,
        uint24 fee,
        address tokenIn,
        address tokenOut,
        uint256 amountIn,
        uint256 amountOutMin
    ) internal returns (uint256 amountOut) {
        if (fee == 0) {
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
