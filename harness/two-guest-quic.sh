#!/usr/bin/env bash
# Two-guest iroh/QUIC reproducer for the HermitOS capstone (docs/CAPSTONE-iroh-in-hermit.md).
# Boots two Hermit unikernel guests on a Linux bridge; the client opens a QUIC connection to the
# server and receives a 200 KB uni-stream. PASS = SERVER_SENT_BYTES=200000 + CLIENT_RECEIVED_BYTES=200000.
#
# Requires root (creates a bridge + taps). Set these to your build outputs:
#   UHYVE  = path to the (patched) uhyve binary  (see patches/uhyve/)
#   PROBE  = the iroh-hermit probe unikernel built for x86_64-unknown-hermit
# Private 10.9.9.0/24 + fixed MACs are arbitrary; change freely. No external network needed.
set -uo pipefail
UHYVE="${UHYVE:?set UHYVE to your patched uhyve binary}"
PROBE="${PROBE:?set PROBE to the iroh-hermit-probe unikernel}"
WORK="${WORK:-$(pwd)}"; cd "$WORK"

cleanup() {
  pkill -f "uhyve.*$(basename "$PROBE")" 2>/dev/null
  for t in tapA tapB; do ip link set $t down 2>/dev/null; ip tuntap del $t mode tap 2>/dev/null; done
  ip link set br0 down 2>/dev/null; ip link del br0 2>/dev/null
}
cleanup; sleep 1
ip link add br0 type bridge stp_state 0
ip addr add 10.9.9.1/24 dev br0; ip link set br0 up
for t in tapA tapB; do ip tuntap add $t mode tap; ip link set $t master br0; ip link set $t up; done
echo "net up"; : > srv.log; : > cli.log

# server guest
setsid bash -c "$UHYVE -e HERMIT_IP=10.9.9.2 -e HERMIT_GATEWAY=10.9.9.1 -e HERMIT_MASK=255.255.255.0 --net tap:tapA -c 2 -m 1GiB $PROBE > srv.log 2>&1" </dev/null >/dev/null 2>&1 & disown
sleep 2
ip link set tapA address 02:00:00:00:00:aa   # distinct tap MAC AFTER uhyve grabbed the original (bridge FDB learning is racy otherwise)
for i in $(seq 1 25); do grep -aq SERVER_LISTENING srv.log && break; sleep 1; done
grep -aq SERVER_LISTENING srv.log && echo "server LISTENING" || { echo "SERVER FAILED"; tail -4 srv.log; cleanup; exit 1; }

# client guest
setsid bash -c "$UHYVE -e HERMIT_ROLE=client -e HERMIT_IP=10.9.9.3 -e HERMIT_GATEWAY=10.9.9.1 -e HERMIT_MASK=255.255.255.0 --net tap:tapB -c 2 -m 1GiB $PROBE > cli.log 2>&1" </dev/null >/dev/null 2>&1 & disown
sleep 2
ip link set tapB address 02:00:00:00:00:bb
for i in $(seq 1 40); do grep -aqE "CLIENT_RECEIVED_BYTES|CLIENT_CONNECT_ERR|PROBE_DONE" cli.log && break; sleep 1; done
sleep 2
echo "================ SERVER ================"; grep -aE "IROH_SERVER_ID|SERVER_" srv.log
echo "================ CLIENT ================"; grep -aE "IROH_CLIENT_ID|CLIENT_" cli.log
cleanup; echo "net torn down"
