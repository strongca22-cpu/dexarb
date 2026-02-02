# Hetzner Dedicated Server + Bor Node Architecture

## Summary

Run a Polygon Bor full node and the DEX arbitrage bot on the same dedicated Hetzner server. This eliminates the 250ms Alchemy RPC round-trip that currently prevents successful trade capture, provides unfiltered P2P mempool access, and removes all rate limits.

**Expected impact:**

| Metric | Current (Vultr + Alchemy) | Target (Hetzner + Bor) |
|--------|--------------------------|------------------------|
| Block arrival | ~250ms (Alchemy WS relay) | <10ms (P2P, validator-peered) |
| RPC round-trip | ~250ms (network) | <1ms (IPC/localhost) |
| Mempool access | Filtered (Alchemy partial view) | Unfiltered (P2P gossip via txpool API) |
| Tx submission | ~250ms (to Alchemy, relay to network) | <1ms (direct P2P broadcast) |
| Rate limits | 30M CU/month (Alchemy free) | Unlimited |
| **Total latency** | **~253ms** | **~3-7ms** |

**Cost:** ~$140/mo (AX102) — breakeven at ~4% capture rate on current 32 pools.

---

## Architecture Diagram

```
┌─────────────────────────────────────────────────────┐
│              Hetzner AX102 (Falkenstein, DE)         │
│                                                     │
│  ┌──────────────┐     IPC        ┌───────────────┐  │
│  │  dexarb-bot  │ ◄──(socket)──► │   Bor Node    │  │
│  │  (Rust/alloy)│                │  (Polygon PoS) │  │
│  └──────┬───────┘                └───────┬───────┘  │
│         │                                │          │
│         │ sign + submit                  │ P2P      │
│         │ (direct to Bor)                │ gossip   │
│         │                                │          │
│  ┌──────▼───────┐                ┌───────▼───────┐  │
│  │ Local mempool │◄──txpool API──│  Heimdall     │  │
│  │ (unfiltered) │                │  (Tendermint)  │  │
│  └──────────────┘                └───────────────┘  │
│                                                     │
└────────────────────┬────────────────────────────────┘
                     │ P2P (port 30303 + 26656)
                     ▼
            ┌─────────────────┐
            │  Polygon Network │
            │  (validators,    │
            │   other nodes)   │
            └─────────────────┘
```

**Key points:**
- Bot communicates with Bor via IPC Unix socket (no TCP overhead)
- Bor peers with validators via P2P for fastest block propagation
- Tx submission goes directly to local Bor node, which broadcasts to P2P network
- No external RPC dependency (Alchemy becomes optional fallback)
- Single-chain: Polygon only. No other chain nodes on this server.

---

## Hardware Specification

### Recommended: Hetzner AX102

| Component | Spec | Why |
|-----------|------|-----|
| CPU | AMD Ryzen 9 7950X (16C/32T) | Bor is CPU-intensive during block processing. Headroom for bot. |
| RAM | 128GB DDR5 ECC | Bor comfortable at 64GB. Bot + OS + buffers need room. |
| Disk | 2x 1.92TB NVMe | Polygon snapshot ~600GB, growing ~50GB/mo. 12-16 months headroom. |
| Network | 1 Gbit/s unmetered | P2P peering, block propagation |
| Datacenter | **Falkenstein, Germany (FSN1)** | 50-60% of Polygon validators on Hetzner DE / AWS eu-central-1. <10ms to peers. |
| Price | ~€129/mo (~$140/mo) | |

### Budget Alternative: Hetzner AX52

| Component | Spec | Trade-off |
|-----------|------|-----------|
| CPU | AMD Ryzen 7 5800X (8C/16T) | Adequate for both node + bot |
| RAM | 64GB DDR4 ECC | Minimum for Bor, tight with bot running |
| Disk | 2x 1TB NVMe | Tighter — 6-8 months before disk pressure |
| Price | ~€70/mo (~$80/mo) | |

### Disk Layout

```
/dev/nvme0n1  → OS partition (root /, 200GB)
/dev/nvme1n1  → /mnt/polygon-data (dedicated chain data, 1.7TB usable)
  ├── bor/         (Bor chaindata, ~600GB initial)
  ├── heimdall/    (Heimdall data, ~50GB)
  └── bot/         (bot binary, logs, data)
```

---

## Software Stack

| Component | Version | Purpose |
|-----------|---------|---------|
| **Ubuntu** | 22.04 LTS | Server OS |
| **Bor** | v1.4.x | Polygon execution layer (EVM, P2P, mempool) |
| **Heimdall** | v1.0.x | Polygon consensus layer (Tendermint, checkpoints) |
| **Go** | 1.21+ | Required for Bor/Heimdall builds (if building from source) |
| **Rust** | stable | Bot build toolchain |
| **dexarb-bot** | alloy 1.5.2 | Arbitrage bot binary |

---

## Latency Budget Breakdown

### Current (Vultr VPS + Alchemy)

```
T=0ms         Block produced by validator
T=120ms       Alchemy receives block via P2P relay
T=250ms       Block arrives at our VPS via Alchemy WS
T=300ms       eth_getLogs: pool events (~50ms RPC round-trip)
T=305ms       Opportunity scan (CPU, <5ms)
T=555ms       estimateGas round-trip (~250ms)
T=560ms       Sign tx (~5ms)
T=810ms       send_raw via Alchemy (~250ms)
T=815ms       Alchemy relays to P2P network
──────────────────────────────────────────
Total: ~815ms from block production to tx broadcast
```

### Target (Hetzner + Bor + IPC)

```
T=0ms         Block produced by validator
T=5ms         Bor receives block via P2P (validator-peered, same datacenter)
T=5.5ms       Block arrives at bot via IPC subscription
T=6ms         eth_getLogs via IPC (<0.5ms)
T=6.5ms       Opportunity scan (CPU, <0.5ms)
T=7ms         estimateGas via IPC (<0.5ms)
T=7.5ms       Sign tx (~0.5ms)
T=8ms         Submit to Bor via IPC (<0.5ms)
T=8.5ms       Bor broadcasts to P2P network
──────────────────────────────────────────
Total: ~8ms from block production to tx broadcast
```

**Improvement: ~100x faster end-to-end.**

---

## Setup Steps

### 1. Order Server

1. Go to https://www.hetzner.com/dedicated-rootserver
2. Select AX102 (or AX52 for budget)
3. Datacenter: **Falkenstein (FSN1)**
4. OS: Ubuntu 22.04 LTS minimal
5. No add-ons needed

### 2. Initial Server Setup

```bash
# SSH in as root (credentials in provisioning email)
ssh root@<server-ip>

# Create non-root user
adduser polygon && usermod -aG sudo polygon

# Set up SSH key auth (from local machine)
ssh-copy-id -i ~/.ssh/your_key.pub polygon@<server-ip>

# Harden SSH (edit /etc/ssh/sshd_config)
# PermitRootLogin no
# PasswordAuthentication no

# Firewall
sudo ufw default deny incoming && sudo ufw default allow outgoing
sudo ufw allow 22/tcp      # SSH
sudo ufw allow 30303/tcp   # Bor P2P
sudo ufw allow 30303/udp   # Bor P2P
sudo ufw allow 26656/tcp   # Heimdall P2P
sudo ufw enable
# NOTE: Do NOT open 8545/8546 — localhost only

# Disk setup (second NVMe for chain data)
sudo parted /dev/nvme1n1 mklabel gpt
sudo parted /dev/nvme1n1 mkpart primary ext4 0% 100%
sudo mkfs.ext4 /dev/nvme1n1p1
sudo mkdir -p /mnt/polygon-data
echo '/dev/nvme1n1p1 /mnt/polygon-data ext4 defaults 0 2' | sudo tee -a /etc/fstab
sudo mount -a && sudo chown -R polygon:polygon /mnt/polygon-data

# Base packages
sudo apt update && sudo apt upgrade -y
sudo apt install -y build-essential curl wget git tmux htop iotop jq aria2 pv ufw fail2ban
```

### 3. Install Bor + Heimdall

```bash
export POLYGON_DATA=/mnt/polygon-data
mkdir -p $POLYGON_DATA/{bor,heimdall} ~/polygon-binaries && cd ~/polygon-binaries

# Heimdall (check latest: https://github.com/maticnetwork/heimdall/releases)
HEIMDALL_VERSION="v1.0.7"
wget https://github.com/maticnetwork/heimdall/releases/download/${HEIMDALL_VERSION}/heimdall-linux-amd64
chmod +x heimdall-linux-amd64 && sudo mv heimdall-linux-amd64 /usr/local/bin/heimdall
heimdall init --home $POLYGON_DATA/heimdall

# Bor (check latest: https://github.com/maticnetwork/bor/releases)
BOR_VERSION="v1.4.0"
wget https://github.com/maticnetwork/bor/releases/download/${BOR_VERSION}/bor-linux-amd64
chmod +x bor-linux-amd64 && sudo mv bor-linux-amd64 /usr/local/bin/bor

# Download genesis files
wget -O $POLYGON_DATA/heimdall/config/genesis.json \
  https://raw.githubusercontent.com/maticnetwork/heimdall/master/builder/files/genesis-mainnet-v1.json
wget -O ~/polygon-binaries/genesis.json \
  https://raw.githubusercontent.com/maticnetwork/bor/master/builder/files/genesis-mainnet-v1.json
bor init --datadir $POLYGON_DATA/bor ~/polygon-binaries/genesis.json
```

### 4. Download Polygon Snapshots

**Without snapshot: 3-5 days. With snapshot: 4-6 hours.**

```bash
# Bor snapshot (~600GB)
aria2c -x 16 -s 16 "https://snapshot-download.polygon.technology/bor-mainnet-parts.txt"

# Heimdall snapshot (~50GB)
aria2c -x 16 -s 16 "https://snapshot-download.polygon.technology/heimdall-mainnet-parts.txt"

# Extract to data directories
# (Follow snapshot service instructions for extraction)
```

### 5. Configure Bor for Low-Latency RPC

Key settings in `$POLYGON_DATA/bor/config.toml`:

```toml
[jsonrpc]
http = ["localhost:8545"]
ws = ["localhost:8546"]
api = ["eth", "net", "web3", "txpool", "bor"]
vhosts = ["*"]

[jsonrpc.http]
enabled = true
port = 8545
host = "127.0.0.1"    # localhost only — never expose to internet

[jsonrpc.ws]
enabled = true
port = 8546
host = "127.0.0.1"    # localhost only

[p2p]
max-peers = 80
# Add validator bootnodes for fastest block propagation
bootnodes = [
  "enode://b8f1cc9c5d4403703fbf377116469667d2b1823c0daf16b7250aa576bacf399e42c3930ccfcb02c5df6879565a2b8931335565f0e8d3f8e72385ecf4a4bf160a@3.36.224.80:30303",
  "enode://8729e0c825f3d9cad382555f3e46dcff21af323e89025a0e6312df541f4a9e73abfa562d64906f5e59c51fe6f0501b3e61b07979606c56329c020ed739910759@54.194.245.5:30303"
]

[txpool]
globalslots = 32768    # Large mempool buffer
globalqueue = 1024
lifetime = "3h"
pricelimit = 30000000000  # 30 Gwei (Polygon default)
```

### 6. Create Systemd Services

**Heimdall:**
```ini
# /etc/systemd/system/heimdall.service
[Unit]
Description=Heimdall Node
After=network.target

[Service]
Type=simple
User=polygon
ExecStart=/usr/local/bin/heimdall start --home /mnt/polygon-data/heimdall --rest-server
Restart=on-failure
RestartSec=10
LimitNOFILE=65536

[Install]
WantedBy=multi-user.target
```

**Bor:**
```ini
# /etc/systemd/system/bor.service
[Unit]
Description=Bor Node
After=network.target heimdall.service
Wants=heimdall.service

[Service]
Type=simple
User=polygon
ExecStart=/usr/local/bin/bor server \
  --config /mnt/polygon-data/bor/config.toml \
  --datadir /mnt/polygon-data/bor \
  --chain mainnet \
  --syncmode full \
  --gcmode archive \
  --cache 8192 \
  --maxpeers 80
Restart=on-failure
RestartSec=10
LimitNOFILE=65536

[Install]
WantedBy=multi-user.target
```

**Bot (optional systemd — can also use tmux):**
```ini
# /etc/systemd/system/polygon-bot.service
[Unit]
Description=Polygon DEX Arbitrage Bot
After=bor.service
Requires=bor.service

[Service]
Type=simple
User=polygon
WorkingDirectory=/home/polygon/bots/dexarb/src/rust-bot
ExecStart=/home/polygon/bots/dexarb/src/rust-bot/target/release/dexarb-bot --chain polygon
Restart=on-failure
RestartSec=10

[Install]
WantedBy=multi-user.target
```

**Start services:**
```bash
sudo systemctl daemon-reload
sudo systemctl enable heimdall bor
sudo systemctl start heimdall
sleep 30  # Wait for Heimdall to initialize
sudo systemctl start bor
```

---

## Bot Configuration Changes

### .env.polygon (update for local node)

```env
# === Local Bor node ===
POLYGON_HTTP_RPC=http://127.0.0.1:8545
POLYGON_WS_RPC=ws://127.0.0.1:8546
# POLYGON_IPC_PATH=/mnt/polygon-data/bor/bor.ipc  # Enable after A7 Phase 7

# === Previous Alchemy config (keep as fallback) ===
# POLYGON_HTTP_RPC=https://polygon-mainnet.g.alchemy.com/v2/YOUR_KEY
# POLYGON_WS_RPC=wss://polygon-mainnet.g.alchemy.com/v2/YOUR_KEY

# === Mempool mode ===
MEMPOOL_MONITOR=execute  # Full mempool execution via txpool API

# === Everything else stays the same ===
PRIVATE_KEY=0x...
CHAIN_ID=137
MIN_PROFIT_USD=0.10
MAX_TRADE_SIZE_USD=500
MAX_SLIPPAGE_PERCENT=0.5
```

---

## Validation Checklist (after setup)

### Node Health

```bash
# Heimdall synced?
curl -s localhost:26657/status | jq '.result.sync_info.catching_up'
# Expected: false

# Bor synced?
curl -s -X POST -H "Content-Type: application/json" \
  --data '{"jsonrpc":"2.0","method":"eth_syncing","params":[],"id":1}' \
  http://localhost:8545 | jq '.result'
# Expected: false

# Current block (compare to Polygonscan)
curl -s -X POST -H "Content-Type: application/json" \
  --data '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' \
  http://localhost:8545 | jq -r '.result' | xargs printf '%d\n'

# Peer count (should be >30)
curl -s -X POST -H "Content-Type: application/json" \
  --data '{"jsonrpc":"2.0","method":"net_peerCount","params":[],"id":1}' \
  http://localhost:8545 | jq -r '.result'

# Mempool size (should be >0)
curl -s -X POST -H "Content-Type: application/json" \
  --data '{"jsonrpc":"2.0","method":"txpool_status","params":[],"id":1}' \
  http://localhost:8545 | jq '.result'
```

### Bot Validation

1. Start bot in dry-run mode, verify it connects to localhost:8545
2. Confirm block subscription works (WS to localhost:8546)
3. Confirm eth_getLogs returns pool events
4. Run for 1 hour, compare opportunity detection rate to Alchemy baseline
5. Enable live execution, monitor for first successful trade

---

## Monitoring & Maintenance

### Monitoring Script

```bash
#!/bin/bash
# ~/monitor-node.sh
echo "=== HEIMDALL ==="
curl -s localhost:26657/status | jq -r '.result.sync_info | "Block: \(.latest_block_height) | Synced: \(.catching_up | not)"'

echo "=== BOR ==="
BLOCK=$(curl -s -X POST -H "Content-Type: application/json" \
  --data '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' \
  http://localhost:8545 | jq -r '.result' | xargs printf '%d\n')
echo "Block: $BLOCK"

echo "=== PEERS ==="
PEERS=$(curl -s -X POST -H "Content-Type: application/json" \
  --data '{"jsonrpc":"2.0","method":"net_peerCount","params":[],"id":1}' \
  http://localhost:8545 | jq -r '.result')
echo "Peers: $((16#${PEERS#0x}))"

echo "=== MEMPOOL ==="
curl -s -X POST -H "Content-Type: application/json" \
  --data '{"jsonrpc":"2.0","method":"txpool_status","params":[],"id":1}' \
  http://localhost:8545 | jq '.result'

echo "=== DISK ==="
df -h /mnt/polygon-data | tail -1
```

### Maintenance Schedule

| Frequency | Task |
|-----------|------|
| **Daily** | Check `monitor-node.sh`, review bot logs |
| **Weekly** | `df -h /mnt/polygon-data`, review bot PnL |
| **Monthly** | Check Bor/Heimdall releases, `journalctl --vacuum-time=30d` |
| **6 months** | Evaluate disk usage, consider pruning strategy |

### Disk Growth

- **Archive mode** (`--gcmode archive`): ~50GB/month growth. 12-16 months on 1.92TB.
- **Full mode** (`--gcmode full`): ~20GB/month growth. Prunes state older than 128 blocks.
- **Recommendation:** Start archive mode. Switch to full mode when disk reaches 80%.

---

## Cost Analysis

| Item | Monthly |
|------|---------|
| Hetzner AX102 | ~$140 |
| Alchemy (fallback, free tier) | $0 |
| **Total** | **~$140** |

**Comparison:**

| Provider | Monthly | Rate Limits |
|----------|---------|-------------|
| Hetzner AX102 (own node) | $140 | Unlimited |
| Alchemy Pay As You Go | $0.45/M CU | 300 req/s |
| QuickNode | $250+ | Varies |
| AWS equivalent (EC2+EBS) | $400+ | None |

**ROI:** At conservative $520/mo projection (10% capture, 32 pools), net profit after server cost = $380/mo. With pool expansion: significantly higher.

---

## Single-Chain Design

This server runs **Polygon only**. Rationale:

1. **Resource isolation.** Bor wants 64GB RAM and fast NVMe I/O. A second chain node would compete for these resources, degrading the latency advantage we're paying for.

2. **Base doesn't benefit from a local node the same way.** Base uses a centralized sequencer — mempool access comes from the sequencer feed, not P2P gossip. A local node gives less edge on L2s with sequencers.

3. **Clean failure domains.** If Bor crashes or needs maintenance, only the Polygon bot is affected.

**Future multi-chain strategy:**
- **Option A:** Add observer nodes on cheaper VPS instances for data collection
- **Option B:** Dedicated server per chain (replicate this model for chain #2)
- Decision deferred until Polygon server is generating consistent revenue

---

## Troubleshooting

| Issue | Cause | Fix |
|-------|-------|-----|
| Bor "connection refused" | Heimdall not ready | Wait 60s, `sudo systemctl restart bor` |
| Node stuck syncing | Bad peers or disk full | Restart bor, check `df -h` |
| Low peer count (<10) | Firewall blocking 30303 | Check `ufw status`, ensure P2P ports open |
| Bot can't connect to 8545 | Bor not listening | Check config `host = "127.0.0.1"`, restart bor |
| txpool_content empty | txpool not enabled | Verify `api = ["eth","net","web3","txpool","bor"]` in config |
| High CPU after sync | Tight bot loop | Add rate limiting to bot queries |

---

*Full step-by-step setup guide archived at `docs/archive/polygon-bor-setup-guide.md`.*
*Last updated: 2026-02-02.*
