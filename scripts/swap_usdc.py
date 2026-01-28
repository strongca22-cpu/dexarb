#!/usr/bin/env python3
"""
USDC Swap Utility

Purpose:
    Swap between Native USDC and USDC.e (bridged) on Polygon
    Uses Uniswap V3 0.01% pool for minimal fees

Author: AI-Generated
Created: 2026-01-28
Modified: 2026-01-28

Dependencies:
    - web3
    - eth_account

Usage:
    python scripts/swap_usdc.py [--amount AMOUNT] [--direction native_to_bridged|bridged_to_native] [--execute]

    Default: Shows quote only (dry run)
    Add --execute to actually perform the swap

Notes:
    - Requires PRIVATE_KEY in environment or .env file
    - Uses Uniswap V3 SwapRouter02 on Polygon
    - 0.01% fee tier for USDC<->USDC.e swaps
"""

import os
import sys
import argparse
from decimal import Decimal
from web3 import Web3
from eth_account import Account

# Constants
USDC_NATIVE = '0x3c499c542cEF5E3811e1192ce70d8cC03d5c3359'
USDC_BRIDGED = '0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174'
V3_ROUTER = '0xE592427A0AEce92De3Edee1F18E0157C05861564'  # SwapRouter
V3_QUOTER = '0xb27308f9F90D607463bb33eA1BeBb41C27CE5AB6'   # Quoter

# Fee tier: 100 = 0.01%
FEE_TIER = 100

# ABIs
ERC20_ABI = [
    {"constant": True, "inputs": [{"name": "_owner", "type": "address"}], "name": "balanceOf", "outputs": [{"name": "balance", "type": "uint256"}], "type": "function"},
    {"constant": True, "inputs": [{"name": "_owner", "type": "address"}, {"name": "_spender", "type": "address"}], "name": "allowance", "outputs": [{"name": "", "type": "uint256"}], "type": "function"},
    {"constant": False, "inputs": [{"name": "_spender", "type": "address"}, {"name": "_value", "type": "uint256"}], "name": "approve", "outputs": [{"name": "", "type": "bool"}], "type": "function"},
    {"constant": True, "inputs": [], "name": "decimals", "outputs": [{"name": "", "type": "uint8"}], "type": "function"},
    {"constant": True, "inputs": [], "name": "symbol", "outputs": [{"name": "", "type": "string"}], "type": "function"},
]

QUOTER_ABI = [
    {"inputs": [{"name": "tokenIn", "type": "address"}, {"name": "tokenOut", "type": "address"}, {"name": "fee", "type": "uint24"}, {"name": "amountIn", "type": "uint256"}, {"name": "sqrtPriceLimitX96", "type": "uint160"}], "name": "quoteExactInputSingle", "outputs": [{"name": "amountOut", "type": "uint256"}], "stateMutability": "nonpayable", "type": "function"}
]

ROUTER_ABI = [
    {"inputs": [{"components": [{"name": "tokenIn", "type": "address"}, {"name": "tokenOut", "type": "address"}, {"name": "fee", "type": "uint24"}, {"name": "recipient", "type": "address"}, {"name": "deadline", "type": "uint256"}, {"name": "amountIn", "type": "uint256"}, {"name": "amountOutMinimum", "type": "uint256"}, {"name": "sqrtPriceLimitX96", "type": "uint160"}], "name": "params", "type": "tuple"}], "name": "exactInputSingle", "outputs": [{"name": "amountOut", "type": "uint256"}], "stateMutability": "payable", "type": "function"}
]


def load_env():
    """Load environment variables from .env file."""
    env_path = os.path.join(os.path.dirname(__file__), '..', 'src', 'rust-bot', '.env')
    if os.path.exists(env_path):
        with open(env_path) as f:
            for line in f:
                line = line.strip()
                if line and not line.startswith('#') and '=' in line:
                    key, value = line.split('=', 1)
                    os.environ.setdefault(key, value)


def get_quote(w3, token_in, token_out, amount_in):
    """Get quote for swap."""
    quoter = w3.eth.contract(address=Web3.to_checksum_address(V3_QUOTER), abi=QUOTER_ABI)

    try:
        # quoteExactInputSingle is a non-view function that we call statically
        amount_out = quoter.functions.quoteExactInputSingle(
            Web3.to_checksum_address(token_in),
            Web3.to_checksum_address(token_out),
            FEE_TIER,
            amount_in,
            0  # sqrtPriceLimitX96 = 0 means no limit
        ).call()
        return amount_out
    except Exception as e:
        print(f"Error getting quote: {e}")
        return None


def approve_token(w3, account, token_address, spender, amount):
    """Approve token spending."""
    token = w3.eth.contract(address=Web3.to_checksum_address(token_address), abi=ERC20_ABI)

    # Check current allowance
    current_allowance = token.functions.allowance(account.address, Web3.to_checksum_address(spender)).call()

    if current_allowance >= amount:
        print(f"Already approved: {current_allowance / 1e6:.2f} USDC")
        return True

    print(f"Approving {amount / 1e6:.2f} USDC for router...")

    # Build approval transaction
    nonce = w3.eth.get_transaction_count(account.address)
    gas_price = w3.eth.gas_price

    tx = token.functions.approve(
        Web3.to_checksum_address(spender),
        amount
    ).build_transaction({
        'from': account.address,
        'nonce': nonce,
        'gas': 100000,
        'gasPrice': gas_price,
        'chainId': 137
    })

    # Sign and send
    signed_tx = account.sign_transaction(tx)
    tx_hash = w3.eth.send_raw_transaction(signed_tx.raw_transaction)
    print(f"Approval tx: {tx_hash.hex()}")

    # Wait for confirmation
    receipt = w3.eth.wait_for_transaction_receipt(tx_hash, timeout=120)
    if receipt['status'] == 1:
        print("Approval confirmed!")
        return True
    else:
        print("Approval failed!")
        return False


def execute_swap(w3, account, token_in, token_out, amount_in, min_amount_out):
    """Execute the swap."""
    router = w3.eth.contract(address=Web3.to_checksum_address(V3_ROUTER), abi=ROUTER_ABI)

    # Deadline: 10 minutes from now
    import time
    deadline = int(time.time()) + 600

    # Build swap params
    params = (
        Web3.to_checksum_address(token_in),
        Web3.to_checksum_address(token_out),
        FEE_TIER,
        account.address,
        deadline,
        amount_in,
        min_amount_out,
        0  # sqrtPriceLimitX96
    )

    nonce = w3.eth.get_transaction_count(account.address)
    gas_price = w3.eth.gas_price

    # Estimate gas
    try:
        gas_estimate = router.functions.exactInputSingle(params).estimate_gas({
            'from': account.address,
            'value': 0
        })
        gas_limit = int(gas_estimate * 1.2)  # 20% buffer
    except Exception as e:
        print(f"Gas estimation failed: {e}")
        gas_limit = 200000

    tx = router.functions.exactInputSingle(params).build_transaction({
        'from': account.address,
        'nonce': nonce,
        'gas': gas_limit,
        'gasPrice': gas_price,
        'value': 0,
        'chainId': 137
    })

    # Sign and send
    signed_tx = account.sign_transaction(tx)
    tx_hash = w3.eth.send_raw_transaction(signed_tx.raw_transaction)
    print(f"Swap tx: {tx_hash.hex()}")
    print(f"View on Polygonscan: https://polygonscan.com/tx/{tx_hash.hex()}")

    # Wait for confirmation
    print("Waiting for confirmation...")
    receipt = w3.eth.wait_for_transaction_receipt(tx_hash, timeout=120)

    if receipt['status'] == 1:
        print("Swap successful!")
        return True
    else:
        print("Swap failed!")
        return False


def main():
    parser = argparse.ArgumentParser(description='Swap USDC Native <-> USDC.e on Polygon')
    parser.add_argument('--amount', type=float, help='Amount to swap (default: full balance)')
    parser.add_argument('--direction', choices=['native_to_bridged', 'bridged_to_native'],
                        default='native_to_bridged', help='Swap direction')
    parser.add_argument('--execute', action='store_true', help='Actually execute the swap (default: dry run)')
    parser.add_argument('--slippage', type=float, default=0.5, help='Max slippage percent (default: 0.5)')

    args = parser.parse_args()

    # Load environment
    load_env()

    # Connect to Polygon
    rpc_url = os.environ.get('RPC_URL', 'https://polygon-mainnet.g.alchemy.com/v2/jwcuVSA1FrZ97ftmb8id8')
    # Convert WSS to HTTPS if needed
    if rpc_url.startswith('wss://'):
        rpc_url = rpc_url.replace('wss://', 'https://')

    w3 = Web3(Web3.HTTPProvider(rpc_url))

    if not w3.is_connected():
        print("Failed to connect to Polygon RPC")
        sys.exit(1)

    # Load wallet
    private_key = os.environ.get('PRIVATE_KEY')
    if not private_key:
        print("PRIVATE_KEY not found in environment")
        sys.exit(1)

    account = Account.from_key(private_key)
    print(f"Wallet: {account.address}")

    # Determine tokens based on direction
    if args.direction == 'native_to_bridged':
        token_in = USDC_NATIVE
        token_out = USDC_BRIDGED
        label_in = "USDC (Native)"
        label_out = "USDC.e (Bridged)"
    else:
        token_in = USDC_BRIDGED
        token_out = USDC_NATIVE
        label_in = "USDC.e (Bridged)"
        label_out = "USDC (Native)"

    # Get balances
    token_in_contract = w3.eth.contract(address=Web3.to_checksum_address(token_in), abi=ERC20_ABI)
    token_out_contract = w3.eth.contract(address=Web3.to_checksum_address(token_out), abi=ERC20_ABI)

    balance_in = token_in_contract.functions.balanceOf(account.address).call()
    balance_out = token_out_contract.functions.balanceOf(account.address).call()
    matic_balance = w3.eth.get_balance(account.address)

    print(f"\n=== Current Balances ===")
    print(f"{label_in}: {balance_in / 1e6:.6f}")
    print(f"{label_out}: {balance_out / 1e6:.6f}")
    print(f"MATIC: {w3.from_wei(matic_balance, 'ether'):.6f}")

    # Determine amount to swap
    if args.amount:
        amount_in = int(args.amount * 1e6)
    else:
        amount_in = balance_in

    if amount_in == 0:
        print(f"\nNo {label_in} to swap!")
        sys.exit(0)

    if amount_in > balance_in:
        print(f"\nInsufficient balance! Have {balance_in / 1e6:.2f}, want to swap {amount_in / 1e6:.2f}")
        sys.exit(1)

    print(f"\n=== Swap Details ===")
    print(f"From: {amount_in / 1e6:.6f} {label_in}")

    # Get quote
    quote = get_quote(w3, token_in, token_out, amount_in)
    if quote is None:
        print("Failed to get quote")
        sys.exit(1)

    print(f"To: ~{quote / 1e6:.6f} {label_out}")
    print(f"Rate: 1 {label_in} = {quote / amount_in:.6f} {label_out}")
    print(f"Fee tier: 0.01%")

    # Calculate minimum output with slippage
    min_out = int(quote * (1 - args.slippage / 100))
    print(f"Min output (with {args.slippage}% slippage): {min_out / 1e6:.6f}")

    # Estimate gas cost
    gas_price = w3.eth.gas_price
    estimated_gas = 150000  # Typical for V3 swap
    gas_cost_wei = gas_price * estimated_gas
    gas_cost_matic = w3.from_wei(gas_cost_wei, 'ether')
    print(f"Estimated gas: ~{gas_cost_matic:.4f} MATIC")

    if not args.execute:
        print(f"\n=== DRY RUN ===")
        print("Add --execute to actually perform the swap")
        sys.exit(0)

    print(f"\n=== EXECUTING SWAP ===")

    # Check MATIC for gas
    if matic_balance < gas_cost_wei * 2:
        print(f"Warning: Low MATIC balance for gas!")

    # Approve if needed
    if not approve_token(w3, account, token_in, V3_ROUTER, amount_in):
        print("Approval failed, aborting")
        sys.exit(1)

    # Execute swap
    if execute_swap(w3, account, token_in, token_out, amount_in, min_out):
        # Show final balances
        new_balance_in = token_in_contract.functions.balanceOf(account.address).call()
        new_balance_out = token_out_contract.functions.balanceOf(account.address).call()

        print(f"\n=== Final Balances ===")
        print(f"{label_in}: {new_balance_in / 1e6:.6f}")
        print(f"{label_out}: {new_balance_out / 1e6:.6f}")
        print(f"\nSwap complete!")
    else:
        print("Swap failed!")
        sys.exit(1)


if __name__ == "__main__":
    main()
