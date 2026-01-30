// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Script, console} from "forge-std/Script.sol";
import {ArbExecutor} from "../src/ArbExecutor.sol";

/// @title Deploy ArbExecutor to Polygon
/// @notice Run: forge script script/DeployArb.s.sol --rpc-url polygon --broadcast --private-key $PK
/// @author AI-Generated
/// @custom:created 2026-01-30

contract DeployArb is Script {
    function run() external {
        vm.startBroadcast();
        ArbExecutor arb = new ArbExecutor();
        console.log("ArbExecutor deployed at:", address(arb));
        console.log("Owner:", arb.owner());
        vm.stopBroadcast();
    }
}
