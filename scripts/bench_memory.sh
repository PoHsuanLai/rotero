#!/bin/bash
# Memory comparison: Rotero vs Zotero
# Usage: ./scripts/bench_memory.sh [runs]
#
# 1. Open both Rotero and Zotero with a similar library loaded
# 2. Wait ~10s for both to settle
# 3. Run this script

RUNS=${1:-3}

echo "=== Memory Benchmark: Rotero vs Zotero ==="
echo "Runs: $RUNS"
echo ""

measure() {
    local name="$1"
    local pattern="$2"
    # Sum RSS across all processes matching the pattern (in KB)
    local rss_kb
    rss_kb=$(ps -A -o rss,comm | grep -i "$pattern" | grep -v grep | awk '{sum+=$1} END {print sum+0}')
    echo "$rss_kb"
}

rotero_total=0
zotero_total=0

for i in $(seq 1 "$RUNS"); do
    echo "--- Run $i ---"

    r_kb=$(measure "rotero" "rotero")
    z_kb=$(measure "zotero" "zotero")

    if [ "$r_kb" -eq 0 ]; then
        echo "  Rotero: not running"
    else
        r_mb=$(echo "scale=1; $r_kb / 1024" | bc)
        echo "  Rotero: ${r_mb} MB  (${r_kb} KB across $(ps -A -o comm | grep -ic rotero) processes)"
        rotero_total=$((rotero_total + r_kb))
    fi

    if [ "$z_kb" -eq 0 ]; then
        echo "  Zotero: not running"
    else
        z_mb=$(echo "scale=1; $z_kb / 1024" | bc)
        echo "  Zotero: ${z_mb} MB  (${z_kb} KB across $(ps -A -o comm | grep -ic zotero) processes)"
        zotero_total=$((zotero_total + z_kb))
    fi

    if [ "$i" -lt "$RUNS" ]; then
        echo "  (waiting 5s before next run...)"
        sleep 5
    fi
done

echo ""
echo "=== Averages over $RUNS runs ==="
if [ "$rotero_total" -gt 0 ]; then
    avg_r=$(echo "scale=1; $rotero_total / $RUNS / 1024" | bc)
    echo "Rotero: ${avg_r} MB"
fi
if [ "$zotero_total" -gt 0 ]; then
    avg_z=$(echo "scale=1; $zotero_total / $RUNS / 1024" | bc)
    echo "Zotero: ${avg_z} MB"
fi
if [ "$rotero_total" -gt 0 ] && [ "$zotero_total" -gt 0 ]; then
    ratio=$(echo "scale=1; $zotero_total / $rotero_total" | bc)
    echo ""
    echo "Zotero uses ~${ratio}x more memory than Rotero"
fi
