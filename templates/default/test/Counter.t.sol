// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Counter} from "../src/Counter.sol";

contract CounterTest {
    function testIncrement() public {
        Counter counter = new Counter();
        counter.increment();
        require(counter.number() == 1, "counter did not increment");
    }
}
