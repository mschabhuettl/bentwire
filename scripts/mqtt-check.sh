#!/usr/bin/env bash
# MQTT bridge end to end (spec B-MQTT-1): starts a private mosquitto with a password on a free
# local port and runs the ignored broker test of bentwire-mqtt against it (the test starts its
# own dbus-daemon and headless bentwired). Needs mosquitto (with mosquitto_passwd),
# dbus-daemon and python3. Prints "NOT RUN" and exits 77 without mosquitto.
set -euo pipefail
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.."
mosquitto=$(command -v mosquitto || echo /usr/sbin/mosquitto)
if [ ! -x "$mosquitto" ] || ! command -v mosquitto_passwd >/dev/null; then
  echo "NOT RUN: install mosquitto (apt install mosquitto, pacman -S mosquitto)"
  exit 77
fi
tmp=$(mktemp -d)
pid=
cleanup() {
  [ -n "$pid" ] && kill "$pid" 2>/dev/null && wait "$pid" 2>/dev/null
  rm -rf "${tmp:?}"
}
trap cleanup EXIT
port=$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
user=bentwire-test
pass=secret-$RANDOM$RANDOM
mosquitto_passwd -c -b "$tmp/passwd" "$user" "$pass" 2>/dev/null
chmod 0700 "$tmp/passwd"
cat >"$tmp/mosquitto.conf" <<EOF
listener $port 127.0.0.1
allow_anonymous false
password_file $tmp/passwd
persistence false
EOF
# A copy: the distribution's AppArmor profile for the installed path only lets it read
# /etc/mosquitto.
cp "$mosquitto" "$tmp/mosquitto"
"$tmp/mosquitto" -c "$tmp/mosquitto.conf" >"$tmp/mosquitto.log" 2>&1 &
pid=$!
for _ in $(seq 50); do
  python3 -c "import socket; socket.create_connection(('127.0.0.1', $port), 0.2)" 2>/dev/null && break
  sleep 0.1
done
if ! kill -0 "$pid" 2>/dev/null; then
  cat "$tmp/mosquitto.log"
  echo "mosquitto did not start"
  exit 1
fi
echo "mosquitto on 127.0.0.1:$port"
BENTWIRE_MQTT_PORT=$port BENTWIRE_MQTT_USERNAME=$user BENTWIRE_MQTT_PASSWORD=$pass \
  cargo test -p bentwire-mqtt --test broker -- --ignored --nocapture "$@"
