#!/bin/bash
pkill -f cipherroute 2>/dev/null || true
sleep 2
./target/debug/cipherroute &
echo $! > cipherroute.pid
sleep 5
ps aux | grep cipherroute | grep -v grep
