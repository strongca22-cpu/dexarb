# Polygon Bor Full Node Setup on Hetzner
## Complete Guide: From Server to Production Bot

---

## 1. SERVER SELECTION & ORDERING

### Recommended Server: **Hetzner AX102**

**Specs:**
- CPU: AMD Ryzen 9 7950X (16 cores, 32 threads)
- RAM: 128GB DDR5 ECC
- Disk: 2x 1.92TB NVMe (software RAID1 or use one for chain data)
- Network: 1 Gbit/s
- Price: ~€129/month (~$140/month)

**Why this spec:**
- **CPU**: Bor is CPU-intensive during sync and block processing. The 7950X gives headroom for both node + bot
- **RAM**: 128GB handles Bor (64GB comfortable), Heimdall, and your Rust bot with room to spare
- **Disk**: 1.92TB gives you ~1.4TB usable after formatting. Current Polygon mainnet snapshot is ~600GB, growing ~50GB/month. This gives 12-16 months of headroom
- **Alternative**: AX41-NVMe works (64GB RAM, 2x 512GB NVMe) but you'll hit disk limits in 6-8 months

### Datacenter Selection

**Recommended: Falkenstein, Germany (FSN1)**

**Reasoning:**
- 50-60% of Polygon validators run on Hetzner DE or AWS eu-central-1 (Frankfurt)
- FSN1 gives you <10ms latency to other validators
- Better P2P peering (lower propagation delay)
- Blocks arrive faster to your node → your bot sees opportunities sooner

**Ordering Steps:**
1. Go to https://www.hetzner.com/dedicated-rootserver
2. Select AX102 (or AX41-NVMe if budget-constrained)
3. Choose Falkenstein datacenter
4. OS: **Ubuntu 22.04 LTS minimal** (select during checkout)
5. Add-ons: None needed (no backup service)
6. Complete order (~5-10 min provisioning)

---

## 2. INITIAL SERVER SETUP

### Step 1: First Login

You'll receive an email with root credentials:

```bash
ssh root@<your-server-ip>
# Enter the password from the email
```

**Change root password immediately:**
```bash
passwd
# Enter a strong password (use a password manager)
```

### Step 2: Create Non-Root User

```bash
adduser polygon
usermod -aG sudo polygon
```

**Set up SSH key authentication:**

On your local machine:
```bash
ssh-keygen -t ed25519 -C "polygon-node"
# Save to ~/.ssh/polygon_node
ssh-copy-id -i ~/.ssh/polygon_node.pub polygon@<your-server-ip>
```

Test it:
```bash
ssh -i ~/.ssh/polygon_node polygon@<your-server-ip>
```

### Step 3: SSH Hardening

```bash
sudo nano /etc/ssh/sshd_config
```

**Critical changes:**
```
PermitRootLogin no
PasswordAuthentication no
PubkeyAuthentication yes
Port 22  # Or change to custom port like 2222
```

Restart SSH:
```bash
sudo systemctl restart sshd
```

**IMPORTANT**: Open a second terminal and test SSH before closing the root session. If you lock yourself out, use Hetzner's rescue console.

### Step 4: Firewall Configuration

```bash
sudo ufw default deny incoming
sudo ufw default allow outgoing

# SSH (adjust if you changed port)
sudo ufw allow 22/tcp

# Bor P2P
sudo ufw allow 30303/tcp
sudo ufw allow 30303/udp

# Heimdall P2P
sudo ufw allow 26656/tcp

# Enable firewall
sudo ufw enable
sudo ufw status verbose
```

**Note**: We're NOT opening 8545 (RPC) or 8546 (WS) to the internet. Local access only.

### Step 5: Disk Configuration

Check available disks:
```bash
lsblk
df -h
```

For AX102 with 2x 1.92TB NVMe, you'll typically see them as `/dev/nvme0n1` and `/dev/nvme1n1`.

**Option A: Use entire disk for OS + chain data (simpler)**

If Hetzner provisioned a single partition, you're already set. Just verify you have >1.5TB free:

```bash
df -h /
```

**Option B: Separate partition for chain data (recommended for clean management)**

```bash
# Create partition on second NVMe
sudo parted /dev/nvme1n1 mklabel gpt
sudo parted /dev/nvme1n1 mkpart primary ext4 0% 100%
sudo mkfs.ext4 /dev/nvme1n1p1

# Create mount point
sudo mkdir -p /mnt/polygon-data

# Add to fstab for auto-mount
echo '/dev/nvme1n1p1 /mnt/polygon-data ext4 defaults 0 2' | sudo tee -a /etc/fstab
sudo mount -a

# Verify
df -h /mnt/polygon-data
```

Set ownership:
```bash
sudo chown -R polygon:polygon /mnt/polygon-data
```

### Step 6: Swap Configuration

With 128GB RAM, you don't need much swap, but add 16GB as safety:

```bash
sudo fallocate -l 16G /swapfile
sudo chmod 600 /swapfile
sudo mkswap /swapfile
sudo swapon /swapfile

# Make permanent
echo '/swapfile none swap sw 0 0' | sudo tee -a /etc/fstab

# Verify
free -h
```

### Step 7: System Updates & Base Dependencies

```bash
sudo apt update && sudo apt upgrade -y

# Essential packages
sudo apt install -y \
  build-essential \
  curl \
  wget \
  git \
  tmux \
  htop \
  iotop \
  jq \
  aria2 \
  pv \
  ufw \
  fail2ban

# Install Go (required for building Bor/Heimdall if not using binaries)
wget https://go.dev/dl/go1.21.5.linux-amd64.tar.gz
sudo rm -rf /usr/local/go
sudo tar -C /usr/local -xzf go1.21.5.linux-amd64.tar.gz
echo 'export PATH=$PATH:/usr/local/go/bin' >> ~/.bashrc
source ~/.bashrc
go version
```

---

## 3. BOR + HEIMDALL NODE SETUP

### Directory Structure

```bash
# If using separate partition:
export POLYGON_DATA=/mnt/polygon-data

# If using root partition:
# export POLYGON_DATA=/home/polygon/polygon-data

mkdir -p $POLYGON_DATA/{bor,heimdall}
mkdir -p ~/polygon-binaries
cd ~/polygon-binaries
```

### Step 1: Install Heimdall

**Download latest binary:**

```bash
# Check latest version at: https://github.com/maticnetwork/heimdall/releases
HEIMDALL_VERSION="v1.0.7"  # Verify latest on GitHub

wget https://github.com/maticnetwork/heimdall/releases/download/${HEIMDALL_VERSION}/heimdall-linux-amd64
chmod +x heimdall-linux-amd64
sudo mv heimdall-linux-amd64 /usr/local/bin/heimdall

# Verify
heimdall version
```

**Initialize Heimdall:**

```bash
heimdall init --home $POLYGON_DATA/heimdall

# Download genesis file
wget -O $POLYGON_DATA/heimdall/config/genesis.json https://raw.githubusercontent.com/maticnetwork/heimdall/master/builder/files/genesis-mainnet-v1.json

# Download seeds
curl -s https://raw.githubusercontent.com/maticnetwork/heimdall/master/builder/files/seeds/mainnet-v1-seeds.txt > $POLYGON_DATA/heimdall/config/seeds.txt
```

**Configure Heimdall:**

```bash
nano $POLYGON_DATA/heimdall/config/config.toml
```

Key settings:
```toml
# Line ~188 - Add seeds
seeds = "2a53a15ffc70ad41b6876ecbe05c50a66af01e20@3.211.248.31:26656,6f829065789e5b156cbbf076f9d133b4d7725847@3.212.183.151:26656,7285a532bad665f051c0aadc31054e2e61ca2b3d@3.93.224.197:26656,0b431127d21c8970f1c353ab212be4f1ba86c3bf@184.73.124.158:26656,f4f605d60b8ffaaf15240564e58a81103510631c@159.203.9.164:26656,31b79cf4a628a4619e8e9ae95b72e4354c5a5d90@44.232.55.71:26656"

# Line ~212 - Increase max peers
max_num_inbound_peers = 80
max_num_outbound_peers = 40

# Line ~281 - Enable prometheus
prometheus = true
prometheus_listen_addr = ":26660"
```

### Step 2: Install Bor

**Download latest binary:**

```bash
# Check latest version at: https://github.com/maticnetwork/bor/releases
BOR_VERSION="v1.4.0"  # Verify latest on GitHub

wget https://github.com/maticnetwork/bor/releases/download/${BOR_VERSION}/bor-linux-amd64
chmod +x bor-linux-amd64
sudo mv bor-linux-amd64 /usr/local/bin/bor

# Verify
bor version
```

**Initialize Bor:**

```bash
bor init --datadir $POLYGON_DATA/bor ~/polygon-binaries/genesis.json
```

Wait, we need the genesis file first:

```bash
wget -O ~/polygon-binaries/genesis.json https://raw.githubusercontent.com/maticnetwork/bor/master/builder/files/genesis-mainnet-v1.json

# Now initialize
bor init --datadir $POLYGON_DATA/bor ~/polygon-binaries/genesis.json
```

**Download Bor config:**

```bash
wget -O $POLYGON_DATA/bor/config.toml https://raw.githubusercontent.com/maticnetwork/bor/master/builder/files/config-mainnet-v1.toml
```

### Step 3: Download Polygon Snapshot (Critical for Fast Sync)

**Without a snapshot, syncing from genesis takes 3-5 days. With snapshot: 4-6 hours.**

**Fastest method: Use Polygon's official snapshot service**

Check available snapshots:
```bash
# Heimdall snapshot
aria2c -x 16 -s 16 "https://snapshot-download.polygon.technology/heimdall-mainnet-parts.txt"

# Bor snapshot (this is the large one)
aria2c -x 16 -s 16 "https://snapshot-download.polygon.technology/bor-mainnet-parts.txt"
```

**Alternative (if official is slow): Ankr or QuickNode snapshots**

```bash
# Example with aria2c (16 parallel connections)
aria2c -x 16 -s 16 -k 1M \
  -d $POLYGON_DATA/bor \
  "https://polygon-bor-snapshot.s3.amazonaws.com/latest.tar.gz"
```

**Extract snapshot:**

```bash
cd $POLYGON_DATA/bor
tar -xzvf latest.tar.gz --strip-components=1
rm latest.tar.gz

# Verify data exists
ls -lh $POLYGON_DATA/bor/bor/chaindata
```

**For Heimdall (much smaller, ~50GB):**

```bash
cd $POLYGON_DATA/heimdall/data
# Download and extract Heimdall snapshot similarly
```

### Step 4: Configure Bor for Low-Latency RPC

Edit `$POLYGON_DATA/bor/config.toml`:

```bash
nano $POLYGON_DATA/bor/config.toml
```

**Critical settings:**

```toml
[eth]
# Enable txpool for mempool access
txpool = true

[jsonrpc]
# RPC endpoints (localhost only)
http = ["localhost:8545"]
ws = ["localhost:8546"]
api = ["eth", "net", "web3", "txpool", "bor"]  # Note: txpool API enabled
vhosts = ["*"]
corsdomain = ["*"]
ws-origins = ["*"]

[jsonrpc.http]
enabled = true
port = 8545
host = "127.0.0.1"  # CRITICAL: localhost only

[jsonrpc.ws]
enabled = true
port = 8546
host = "127.0.0.1"  # CRITICAL: localhost only

[p2p]
max-peers = 80
no-discover = false
static-nodes = []  # Will auto-discover via bootnodes

[txpool]
locals = []
no-locals = false
journal = "transactions.rlp"
rejournal = "1h"
pricelimit = 30000000000  # 30 Gwei minimum (Polygon default)
pricebump = 10
accountslots = 16
globalslots = 32768  # Increase for better mempool view
accountqueue = 64
globalqueue = 1024
lifetime = "3h"

[miner]
# Not mining, but these affect mempool
gaslimit = 20000000
gasprice = "30000000000"  # 30 Gwei
```

**Add bootnodes to Bor config:**

```toml
[p2p]
max-peers = 80
bootnodes = [
  "enode://b8f1cc9c5d4403703fbf377116469667d2b1823c0daf16b7250aa576bacf399e42c3930ccfcb02c5df6879565a2b8931335565f0e8d3f8e72385ecf4a4bf160a@3.36.224.80:30303",
  "enode://8729e0c825f3d9cad382555f3e46dcff21af323e89025a0e6312df541f4a9e73abfa562d64906f5e59c51fe6f0501b3e61b07979606c56329c020ed739910759@54.194.245.5:30303"
]
```

### Step 5: Create Systemd Services

**Heimdall service:**

```bash
sudo nano /etc/systemd/system/heimdall.service
```

```ini
[Unit]
Description=Heimdall Node
After=network.target

[Service]
Type=simple
User=polygon
WorkingDirectory=/mnt/polygon-data/heimdall
ExecStart=/usr/local/bin/heimdall start \
  --home /mnt/polygon-data/heimdall \
  --rest-server
Restart=on-failure
RestartSec=10
LimitNOFILE=65536

[Install]
WantedBy=multi-user.target
```

**Bor service:**

```bash
sudo nano /etc/systemd/system/bor.service
```

```ini
[Unit]
Description=Bor Node
After=network.target heimdall.service
Wants=heimdall.service

[Service]
Type=simple
User=polygon
WorkingDirectory=/mnt/polygon-data/bor
ExecStart=/usr/local/bin/bor server \
  --config /mnt/polygon-data/bor/config.toml \
  --datadir /mnt/polygon-data/bor \
  --chain mainnet \
  --syncmode full \
  --gcmode archive \
  --txpool.locals 0x0000000000000000000000000000000000000000 \
  --cache 8192 \
  --maxpeers 80
Restart=on-failure
RestartSec=10
LimitNOFILE=65536

[Install]
WantedBy=multi-user.target
```

**Note on `--gcmode archive`**: This keeps all historical state, needed for some DEX queries. If disk space becomes an issue later, switch to `--gcmode full` (prunes old state).

**Enable and start services:**

```bash
sudo systemctl daemon-reload
sudo systemctl enable heimdall bor
sudo systemctl start heimdall

# Wait 30 seconds for Heimdall to initialize
sleep 30

sudo systemctl start bor
```

**Check status:**

```bash
sudo systemctl status heimdall
sudo systemctl status bor
```

**Monitor logs in real-time:**

```bash
# Heimdall logs
sudo journalctl -u heimdall -f

# Bor logs (in another terminal)
sudo journalctl -u bor -f
```

**Common startup gotcha**: If Bor fails with "connection refused" to Heimdall, Heimdall isn't ready yet. Give it 60 seconds and restart Bor:

```bash
sudo systemctl restart bor
```

---

## 4. NODE OPTIMIZATION

### Verify Sync Progress

**Heimdall sync status:**

```bash
curl -s localhost:26657/status | jq '.result.sync_info'
```

Look for:
- `catching_up: false` (synced)
- `latest_block_height` should match current Polygon block height (check on Polygonscan)

**Bor sync status:**

```bash
curl -s -X POST -H "Content-Type: application/json" \
  --data '{"jsonrpc":"2.0","method":"eth_syncing","params":[],"id":1}' \
  http://localhost:8545 | jq
```

- If syncing: Returns object with `currentBlock`, `highestBlock`
- If synced: Returns `false`

**Check current block:**

```bash
curl -s -X POST -H "Content-Type: application/json" \
  --data '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' \
  http://localhost:8545 | jq -r '.result' | xargs printf '%d\n'
```

Compare to Polygonscan's latest block.

### Enable WebSocket Subscriptions

Already enabled in config (`ws = ["localhost:8546"]`). Test it:

```bash
# Install wscat for testing
npm install -g wscat

# Test WebSocket connection
wscat -c ws://localhost:8546

# Once connected, subscribe to new blocks:
{"jsonrpc":"2.0","id":1,"method":"eth_subscribe","params":["newHeads"]}

# You should see block headers streaming in real-time
```

### Verify Txpool API Access

```bash
curl -s -X POST -H "Content-Type: application/json" \
  --data '{"jsonrpc":"2.0","method":"txpool_status","params":[],"id":1}' \
  http://localhost:8545 | jq
```

Should return:
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "pending": 123,  # Number of pending txs in mempool
    "queued": 45
  }
}
```

**Get actual pending transactions:**

```bash
curl -s -X POST -H "Content-Type: application/json" \
  --data '{"jsonrpc":"2.0","method":"txpool_content","params":[],"id":1}' \
  http://localhost:8545 | jq '.result.pending | keys | length'
```

This shows how many addresses have pending transactions.

### Pruning Strategy

**Current config: Archive mode** (`--gcmode archive`)

- **Pros**: Full historical state, needed for complex DEX queries, backfilling
- **Cons**: Disk usage grows ~50GB/month indefinitely

**Alternative: Full mode** (`--gcmode full`)

Change in `/etc/systemd/system/bor.service`:
```
--gcmode full
```

Then:
```bash
sudo systemctl daemon-reload
sudo systemctl restart bor
```

- **Pros**: Prunes state older than 128 blocks, slower disk growth (~20GB/month)
- **Cons**: Can't query historical state beyond 128 blocks

**Recommendation for arbitrage**: Start with archive mode. If disk fills up after 10-12 months, snapshot your data, wipe, and restart with full mode.

### Monitoring Setup

**Create monitoring script:**

```bash
nano ~/monitor-node.sh
```

```bash
#!/bin/bash

echo "=== HEIMDALL STATUS ==="
curl -s localhost:26657/status | jq -r '.result.sync_info | "Block: \(.latest_block_height) | Catching up: \(.catching_up)"'

echo -e "\n=== BOR STATUS ==="
BLOCK_HEX=$(curl -s -X POST -H "Content-Type: application/json" --data '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' http://localhost:8545 | jq -r '.result')
BLOCK_DEC=$((16#${BLOCK_HEX#0x}))
echo "Current block: $BLOCK_DEC"

SYNC=$(curl -s -X POST -H "Content-Type: application/json" --data '{"jsonrpc":"2.0","method":"eth_syncing","params":[],"id":1}' http://localhost:8545 | jq -r '.result')
if [ "$SYNC" = "false" ]; then
  echo "Status: SYNCED ✓"
else
  echo "Status: Syncing..."
  echo $SYNC | jq
fi

echo -e "\n=== PEER COUNT ==="
PEERS=$(curl -s -X POST -H "Content-Type: application/json" --data '{"jsonrpc":"2.0","method":"net_peerCount","params":[],"id":1}' http://localhost:8545 | jq -r '.result')
PEERS_DEC=$((16#${PEERS#0x}))
echo "Connected peers: $PEERS_DEC"

echo -e "\n=== MEMPOOL STATUS ==="
curl -s -X POST -H "Content-Type: application/json" --data '{"jsonrpc":"2.0","method":"txpool_status","params":[],"id":1}' http://localhost:8545 | jq '.result'

echo -e "\n=== DISK USAGE ==="
df -h /mnt/polygon-data | tail -1

echo -e "\n=== SYSTEM RESOURCES ==="
free -h | grep Mem
top -bn1 | grep "Cpu(s)" | sed "s/.*, *\([0-9.]*\)%* id.*/\1/" | awk '{print "CPU Usage: " 100 - $1"%"}'
```

Make executable:
```bash
chmod +x ~/monitor-node.sh
```

Run it:
```bash
./monitor-node.sh
```

**Set up cron job for disk alerts:**

```bash
crontab -e
```

Add:
```
0 * * * * df -h /mnt/polygon-data | tail -1 | awk '{if($5+0 > 85) print "WARNING: Polygon disk usage at "$5}'  | mail -s "Polygon Disk Alert" your-email@example.com
```

---

## 5. BOT DEPLOYMENT

### Install Rust Toolchain

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Choose option 1 (default installation)

source $HOME/.cargo/env
rustc --version
```

### Clone and Build Bot

```bash
cd ~
git clone https://github.com/your-username/your-bot-repo.git
cd your-bot-repo

# Build in release mode (optimized)
cargo build --release

# The binary will be at: ./target/release/your-bot-name
```

### Configure Environment

Create `.env` file:

```bash
nano .env
```

Example:
```env
# RPC endpoints (LOCAL NODE)
POLYGON_HTTP_RPC=http://localhost:8545
POLYGON_WS_RPC=ws://localhost:8546

# Previous Alchemy config (comment out or remove)
# POLYGON_HTTP_RPC=https://polygon-mainnet.g.alchemy.com/v2/YOUR_KEY
# POLYGON_WS_RPC=wss://polygon-mainnet.g.alchemy.com/v2/YOUR_KEY

# Your private key (for signing transactions)
PRIVATE_KEY=0x1234...  # Your wallet private key

# Bot settings
MIN_PROFIT_WEI=1000000000000000  # 0.001 MATIC minimum profit
MAX_GAS_PRICE_GWEI=500
POOLS_WHITELIST=factory1,factory2  # Expand this list

# Monitoring
LOG_LEVEL=info
METRICS_PORT=9090
```

**CRITICAL SECURITY NOTE**: 
- This `.env` contains your private key
- Never commit it to git
- Restrict permissions: `chmod 600 .env`
- Consider using a separate wallet with limited funds for the bot

### Test Bot Connection

```bash
./target/release/your-bot-name --dry-run
```

This should:
1. Connect to `localhost:8545`
2. Fetch current block
3. Subscribe to pending transactions via `localhost:8546`
4. Print connection stats

Look for:
```
[INFO] Connected to local RPC at localhost:8545
[INFO] Current block: 12345678
[INFO] WebSocket subscription active
[INFO] Mempool: 234 pending transactions
```

### Run Bot in Production

**Option A: Using tmux (quick start)**

```bash
tmux new -s polygon-bot
./target/release/your-bot-name

# Detach: Ctrl+B, then D
# Reattach: tmux attach -t polygon-bot
```

**Option B: Using systemd (recommended for production)**

```bash
sudo nano /etc/systemd/system/polygon-bot.service
```

```ini
[Unit]
Description=Polygon DEX Arbitrage Bot
After=network.target bor.service
Requires=bor.service

[Service]
Type=simple
User=polygon
WorkingDirectory=/home/polygon/your-bot-repo
Environment="PATH=/home/polygon/.cargo/bin:/usr/local/bin:/usr/bin:/bin"
ExecStart=/home/polygon/your-bot-repo/target/release/your-bot-name
Restart=on-failure
RestartSec=10
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
```

Enable and start:
```bash
sudo systemctl daemon-reload
sudo systemctl enable polygon-bot
sudo systemctl start polygon-bot
sudo systemctl status polygon-bot
```

Monitor logs:
```bash
sudo journalctl -u polygon-bot -f
```

---

## 6. VALIDATION CHECKLIST

### ✓ Node is Fully Synced

**Heimdall:**
```bash
curl -s localhost:26657/status | jq '.result.sync_info.catching_up'
# Should return: false
```

**Bor:**
```bash
curl -s -X POST -H "Content-Type: application/json" --data '{"jsonrpc":"2.0","method":"eth_syncing","params":[],"id":1}' http://localhost:8545 | jq '.result'
# Should return: false
```

**Cross-check block height:**
```bash
# Get your node's block
YOUR_BLOCK=$(curl -s -X POST -H "Content-Type: application/json" --data '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' http://localhost:8545 | jq -r '.result' | xargs printf '%d\n')

# Compare to Polygonscan
echo "Your node: $YOUR_BLOCK"
echo "Check Polygonscan: https://polygonscan.com/"
# Should be within 1-2 blocks
```

### ✓ Mempool/Txpool Access Working

```bash
curl -s -X POST -H "Content-Type: application/json" \
  --data '{"jsonrpc":"2.0","method":"txpool_content","params":[],"id":1}' \
  http://localhost:8545 | jq '.result.pending | to_entries | length'
```

Should return number of pending transactions (typically 50-500 depending on network activity).

**If you see 0 pending transactions:**
- Check peer count: `curl -s -X POST -H "Content-Type: application/json" --data '{"jsonrpc":"2.0","method":"net_peerCount","params":[],"id":1}' http://localhost:8545 | jq -r '.result'` (should be >30)
- Verify txpool is enabled in config
- Restart Bor: `sudo systemctl restart bor`

### ✓ Latency Improvement Measurement

**Create latency test script:**

```bash
nano ~/test-latency.sh
```

```bash
#!/bin/bash

echo "Testing block arrival latency..."

# Subscribe to new blocks and measure timestamp difference
for i in {1..10}; do
  START=$(date +%s%N)
  BLOCK=$(curl -s -X POST -H "Content-Type: application/json" --data '{"jsonrpc":"2.0","method":"eth_getBlockByNumber","params":["latest", false],"id":1}' http://localhost:8545 | jq -r '.result.timestamp')
  END=$(date +%s%N)
  
  BLOCK_TIME=$((16#${BLOCK#0x}))
  NOW=$(date +%s)
  AGE=$((NOW - BLOCK_TIME))
  
  LATENCY=$(( (END - START) / 1000000 ))  # Convert to milliseconds
  
  echo "Block age: ${AGE}s | RPC latency: ${LATENCY}ms"
  sleep 2
done
```

```bash
chmod +x ~/test-latency.sh
./test-latency.sh
```

**Expected results:**
- **Block age**: 0-2 seconds (how old the latest block is)
- **RPC latency**: <10ms (localhost response time)

**Compare to Alchemy:**
- Alchemy typically shows 100-300ms RPC latency
- Block age of 1-3 seconds

**Your improvement: 20-30x faster RPC, fresher blocks**

### ✓ Ongoing Node Health Monitoring

**Set up automated health check:**

```bash
nano ~/health-check.sh
```

```bash
#!/bin/bash

# Get sync status
SYNCING=$(curl -s -X POST -H "Content-Type: application/json" --data '{"jsonrpc":"2.0","method":"eth_syncing","params":[],"id":1}' http://localhost:8545 | jq -r '.result')

if [ "$SYNCING" != "false" ]; then
  echo "WARNING: Node is syncing, not producing blocks normally"
  exit 1
fi

# Check peer count
PEERS=$(curl -s -X POST -H "Content-Type: application/json" --data '{"jsonrpc":"2.0","method":"net_peerCount","params":[],"id":1}' http://localhost:8545 | jq -r '.result')
PEERS_DEC=$((16#${PEERS#0x}))

if [ $PEERS_DEC -lt 20 ]; then
  echo "WARNING: Low peer count ($PEERS_DEC). Should be 40-80."
  exit 1
fi

# Check block freshness
BLOCK_HEX=$(curl -s -X POST -H "Content-Type: application/json" --data '{"jsonrpc":"2.0","method":"eth_getBlockByNumber","params":["latest", false],"id":1}' http://localhost:8545 | jq -r '.result.timestamp')
BLOCK_TIME=$((16#${BLOCK_HEX#0x}))
NOW=$(date +%s)
AGE=$((NOW - BLOCK_TIME))

if [ $AGE -gt 30 ]; then
  echo "WARNING: Latest block is ${AGE}s old. Node may be stalled."
  exit 1
fi

echo "OK: Node healthy. Peers: $PEERS_DEC, Block age: ${AGE}s"
```

```bash
chmod +x ~/health-check.sh
```

**Add to cron (every 5 minutes):**

```bash
crontab -e
```

```
*/5 * * * * /home/polygon/health-check.sh >> /home/polygon/health-check.log 2>&1
```

**Set up alerts (optional, requires mailutils):**

```bash
sudo apt install -y mailutils

# Test email
echo "Test from Polygon node" | mail -s "Test Alert" your-email@example.com
```

Modify health-check.sh to send email on failure:
```bash
if [ $AGE -gt 30 ]; then
  echo "WARNING: Latest block is ${AGE}s old" | mail -s "Polygon Node Alert" your-email@example.com
  exit 1
fi
```

---

## COMMON GOTCHAS & TROUBLESHOOTING

### Issue: Bor fails to start with "connection refused"

**Cause**: Heimdall isn't running or ready yet.

**Fix**:
```bash
sudo systemctl status heimdall
# If not running:
sudo systemctl start heimdall
sleep 30
sudo systemctl restart bor
```

### Issue: Node stuck syncing at same block

**Symptoms**: `eth_syncing` shows same `currentBlock` for >5 minutes

**Diagnosis**:
```bash
sudo journalctl -u bor -n 100 | grep -i error
```

**Common causes:**
1. **Bad peers**: Restart to get new peers
   ```bash
   sudo systemctl restart bor
   ```

2. **Disk full**: Check with `df -h`

3. **Heimdall out of sync**: Verify Heimdall is synced first

### Issue: Low peer count (<10 peers)

**Check firewall:**
```bash
sudo ufw status | grep 30303
sudo ufw status | grep 26656
```

**Manually add peers:**

Edit `$POLYGON_DATA/bor/config.toml`:
```toml
[p2p]
static-nodes = [
  "enode://b8f1cc9c5d4403703fbf377116469667d2b1823c0daf16b7250aa576bacf399e42c3930ccfcb02c5df6879565a2b8931335565f0e8d3f8e72385ecf4a4bf160a@3.36.224.80:30303"
]
```

Restart:
```bash
sudo systemctl restart bor
```

### Issue: Bot can't connect to localhost:8545

**Check if Bor is listening:**
```bash
sudo netstat -tlnp | grep 8545
```

Should show:
```
tcp  0  0  127.0.0.1:8545  0.0.0.0:*  LISTEN  12345/bor
```

**If not listening:**
- Check Bor config: `nano $POLYGON_DATA/bor/config.toml`
- Verify `http = ["localhost:8545"]` is set
- Restart: `sudo systemctl restart bor`

### Issue: txpool_content returns empty mempool

**Cause**: Txpool not configured or disabled

**Fix**:

1. Verify in config.toml:
   ```toml
   [jsonrpc]
   api = ["eth", "net", "web3", "txpool", "bor"]
   ```

2. Check Bor startup args in systemd service include no conflicting flags

3. Restart Bor and wait 2-3 minutes for mempool to populate

### Issue: Disk filling up faster than expected

**Check current growth rate:**
```bash
du -sh $POLYGON_DATA/bor/bor/chaindata
# Wait 24 hours
du -sh $POLYGON_DATA/bor/bor/chaindata
# Compare sizes
```

**Options:**
1. Switch to full mode (prunes old state)
2. Add more disk capacity
3. Set up automated pruning scripts

### Issue: High CPU usage

**Normal behavior:**
- During sync: 80-100% CPU on multiple cores is expected
- After sync: 10-30% CPU average, spikes to 60% during block processing

**If CPU stays at 100% after sync:**
- Check for many small bot requests in a tight loop
- Verify bot isn't DOSing the local RPC
- Add rate limiting to bot queries

---

## PERFORMANCE BENCHMARKS

After full setup, you should see:

| Metric | Alchemy (before) | Local Node (after) | Improvement |
|--------|------------------|-------------------|-------------|
| RPC latency | 200-300ms | 1-5ms | 50-200x faster |
| Block arrival | 2-5s after production | 0-2s after production | Up to 5s earlier |
| Mempool visibility | Filtered by Alchemy | Full unfiltered P2P mempool | Complete view |
| Rate limits | 300 req/s hard cap | Unlimited | No limits |
| Cost | $199/mo (Growth plan) | €129/mo server | 35% cheaper |

**Expected bot performance improvement:**
- **Opportunities detected**: 3-5x more (due to full mempool)
- **Win rate**: 20-40% higher (due to latency advantage)
- **Reverted transactions**: Potentially higher initially (more competition at mempool level, need better filtering)

---

## NEXT STEPS

1. **Monitor for 48 hours**: Let the node run and verify stability
2. **Expand pool whitelist**: Now that you have unfiltered mempool access, add more DEX pools to monitor
3. **Optimize bot logic**: Use the latency advantage to implement more aggressive strategies
4. **Set up metrics**: Add Prometheus + Grafana for visual monitoring (optional)
5. **Backtest historical data**: Use archive node to backfill and improve strategies

---

## MAINTENANCE SCHEDULE

**Daily:**
- Check `~/monitor-node.sh` for sync status
- Review bot logs for errors

**Weekly:**
- Run `./health-check.sh` manually
- Check disk usage: `df -h /mnt/polygon-data`
- Review bot profit/loss

**Monthly:**
- Update Bor/Heimdall if new versions released
- Rotate logs: `sudo journalctl --vacuum-time=30d`
- Review and adjust bot strategy based on performance data

**Every 6 months:**
- Evaluate disk usage projections
- Consider adding storage or switching to full mode
- Review Hetzner server costs vs. Alchemy alternatives

---

## COST BREAKDOWN

| Item | Monthly Cost |
|------|-------------|
| Hetzner AX102 | €129 (~$140) |
| Electricity (included) | €0 |
| Bandwidth (unlimited) | €0 |
| **Total** | **~$140/mo** |

**Compare to:**
- Alchemy Growth plan: $199/mo (with 300 req/s limit)
- QuickNode: $250+/mo for similar specs
- AWS equivalent: $400+/mo (EC2 + EBS)

**ROI**: If your bot generates >$150/month profit, the node pays for itself and gives you unlimited scalability.

---

## SUPPORT RESOURCES

- **Polygon Docs**: https://docs.polygon.technology/
- **Bor GitHub**: https://github.com/maticnetwork/bor
- **Heimdall GitHub**: https://github.com/maticnetwork/heimdall
- **Polygon Discord**: https://discord.gg/polygon (check #node-runners channel)
- **Hetzner Support**: Open ticket at https://robot.hetzner.com

If you encounter issues not covered here, check Bor/Heimdall GitHub issues or ask in Polygon Discord's #node-runners channel. Most node operators are helpful with troubleshooting.

---

**You're now running a production-grade Polygon full node. The latency advantage is real – use it wisely. Good luck with the arbitrage! 🚀**
