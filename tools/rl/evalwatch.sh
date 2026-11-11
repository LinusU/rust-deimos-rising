#!/bin/sh
# Evaluates new snapshots of a run as training writes them: evalwatch.sh runs/<run> [games]
while true; do
  uv run python -W ignore eval.py "$1" --every 2 --games "${2:-16}" >> "$1/evalwatch.txt" 2>&1
  sleep 300
done
