#!/bin/bash
while true
do
	cat /dev/ttyACM0 2> /dev/null
  echo "===================waiting===================="
	sleep 1
done
