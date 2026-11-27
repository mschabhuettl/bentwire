#!/bin/bash
# Real sound output through each audio backend, each in a fresh Arch container (rootless
# podman). A null sink (ALSA: a file device) is recorded while the daemon shows the
# placeholder and plays Show (its first frame has a sound); a test tone played by the
# server's own tool proves the recording works. Prints peak and RMS per recording.
#
#   scripts/audio-backends.sh [BINDIR] [SETUP...]
#
# BINDIR holds bentwired and bentwire (default target/release). SETUPs (default all):
#   pulse           PulseAudio, no pulseaudio-alsa
#   pipewire-pulse  PipeWire + WirePlumber + pipewire-pulse, no pipewire-alsa
#   pipewire        PipeWire + WirePlumber, no ALSA device "pipewire" (pipewire-audio's
#                   50-pipewire.conf removed; pw-record needs the package), and a
#                   stale pulse/native socket
#   alsa            no sound server; ALSA default is a file device
#   speech          PulseAudio with espeak-ng: `bentwire speech on`, then a `say` (B-TTS-6)
#                   is recorded on its own and the mouths drawn meanwhile (Test1 Snapshot)
#                   are counted. The placeholder holds a Speak frame without mouths, so it
#                   shows none; for lip sync set BENTWIRE_AUDIO_CHARS to a search root with
#                   a character whose Speaking frame has them (classics merlin: a directory
#                   with characters/merlin.bwc).
# Downloaded packages are kept in $BENTWIRE_PACMAN_CACHE (default ~/.cache/bentwire/pacman).
set -euo pipefail

stats() { # FILE.wav LABEL: peak and RMS of a 16-bit mono WAV (data chunk to EOF)
    python3 -I - "$1" "$2" <<'PY'
import sys, struct, array
d = open(sys.argv[1], "rb").read() if __import__("os").path.exists(sys.argv[1]) else b""
i = d.find(b"data")
a = array.array("h", d[i + 8:len(d) - (len(d) - i - 8) % 2] if i >= 0 else b"")
n = len(a)
peak = max((abs(x) for x in a), default=0) / 32768
rms = (sum(x * x for x in a) / n) ** 0.5 / 32768 if n else 0.0
w = 1102  # 50 ms at 22050 Hz
win = max(((sum(x * x for x in a[j:j + w]) / w) ** 0.5 / 32768
           for j in range(0, max(n - w, 0) + 1, w // 2)), default=0.0) if n >= w else rms
# Bursts: runs of loud 50 ms windows after at least 100 ms of quiet.
bursts, quiet = 0, 4
for j in range(0, max(n - w, 0) + 1, w // 2):
    loud = (sum(x * x for x in a[j:j + w]) / w) ** 0.5 / 32768 > 0.01
    bursts += loud and quiet >= 4
    quiet = 0 if loud else quiet + 1
print(f"  {sys.argv[2]}: {n} samples, peak {peak:.4f}, RMS {rms:.5f}, "
      f"loudest 50 ms RMS {win:.4f}, {bursts} bursts -> {'SOUND' if peak >= 0.01 else 'SILENT'}")
PY
}

session() { # as user b, inside dbus-run-session
    local s=$1
    cd "$HOME"
    python3 -I -c '
import math, struct, wave
w = wave.open("tone.wav", "wb"); w.setnchannels(1); w.setsampwidth(2); w.setframerate(22050)
w.writeframes(b"".join(struct.pack("<h", int(8000 * math.sin(2 * math.pi * 440 * i / 22050))) for i in range(11025)))'
    case $s in
    pulse | speech)
        pulseaudio -n --daemonize=no --exit-idle-time=-1 --load=module-native-protocol-unix \
            --load="module-null-sink sink_name=rec" >server.log 2>&1 &
        ;;
    pipewire*)
        pipewire >server.log 2>&1 &
        wireplumber >wp.log 2>&1 &
        [ "$s" = pipewire-pulse ] && { pipewire-pulse >pp.log 2>&1 & }
        for _ in $(seq 50); do [ -S "$XDG_RUNTIME_DIR/pipewire-0" ] && break; sleep 0.1; done
        pw-cli create-node adapter '{ factory.name=support.null-audio-sink node.name=rec
            media.class=Audio/Sink object.linger=true audio.position=[FL FR] }' >/dev/null
        if [ "$s" = pipewire ]; then # a socket left behind by a pulse server that is gone
            mkdir -p "$XDG_RUNTIME_DIR/pulse"
            python3 -I -c 'import socket, sys; socket.socket(socket.AF_UNIX).bind(sys.argv[1])' \
                "$XDG_RUNTIME_DIR/pulse/native"
        fi
        sleep 1
        ;;
    alsa)
        cat >.asoundrc <<EOF
pcm.rec { type file; slave.pcm "null"; file "$HOME/rec.wav"; format "wav" }
pcm.!default { type plug; slave { pcm "rec"; format S16_LE; channels 1; rate 22050 } }
EOF
        ;;
    esac
    if [ "$s" = pulse ] || [ "$s" = pipewire-pulse ] || [ "$s" = speech ]; then
        for _ in $(seq 50); do [ -S "$XDG_RUNTIME_DIR/pulse/native" ] && break; sleep 0.1; done
        sleep 1
    fi
    rec() {
        case $s in
        pulse | pipewire-pulse | speech) parecord -d rec.monitor --latency-msec=20 --format=s16le --rate=22050 --channels=1 "$1" & ;;
        pipewire) pw-record --target rec -P stream.capture.sink=true --format s16 --rate 22050 --channels 1 "$1" & ;;
        alsa) return ;;
        esac
        recpid=$!
        sleep 0.5
    }
    unrec() { [ "$s" = alsa ] && { cat rec.wav >"$1"; return; }; sleep 0.5; kill -INT "$recpid"; wait "$recpid" || true; }

    rec /out/control.wav
    case $s in pulse | pipewire-pulse | speech) paplay tone.wav ;; pipewire) pw-play tone.wav ;; alsa) aplay -q tone.wav ;; esac >/out/control.log 2>&1
    unrec /out/control.wav

    printf 'output HEADLESS-1 resolution 1024x768\n' >sway.cfg
    cp /usr/bin/sway ./sway
    WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_LIBINPUT_NO_DEVICES=1 ./sway -c sway.cfg >sway.log 2>&1 &
    for _ in $(seq 100); do [ -S "$XDG_RUNTIME_DIR/wayland-1" ] && break; sleep 0.1; done
    export WAYLAND_DISPLAY=wayland-1
    rec /out/daemon.wav
    local extra=()
    [ "$s" = speech ] && extra=(--test-interface)
    RUST_LOG=info,bentwired::app=debug,zbus=error /b/bentwired --characters /chars --no-tray "${extra[@]}" 2>/out/daemon.log &
    for _ in $(seq 100); do busctl --user status org.bentwire.Agent >/dev/null 2>&1 && break; sleep 0.1; done
    /b/bentwire show --wait >/dev/null
    for _ in 1 2 3; do /b/bentwire play Show --wait >/dev/null; sleep 0.3; done
    streams() { # bentwire's playback streams at the server (grep -c exits 1 on a count of 0)
        case $s in
        pulse | pipewire-pulse | speech) pactl list sink-inputs | grep -c 'application.name = "bentwire"' || [ $? = 1 ] ;;
        pipewire) pw-dump | grep -c '"application.name": "bentwire"' || [ $? = 1 ] ;;
        alsa) echo n/a ;;
        esac
    }
    echo "streams after the last sound: $(streams), 1.5 s later:" >/out/streams.txt
    sleep 1.5 # LINGER; the ALSA file device is complete once the stream is closed
    streams >>/out/streams.txt
    unrec /out/daemon.wav
    if [ "$s" = speech ]; then
        rec /out/speech.wav
        /b/bentwire speech on
        /b/bentwire say --wait 'Hello! I am speaking with espeak-ng, word by word.' >/dev/null &
        local say=$!
        while kill -0 "$say" 2>/dev/null; do # the drawn mouth, as often as the daemon answers
            busctl --user call org.bentwire.Agent /org/bentwire/Test org.bentwire.Test1 Snapshot |
                grep -o 'mouth\\":[a-z0-9]*' | head -1 || true
            sleep 0.02
        done >/out/mouths.txt
        unrec /out/speech.wav
    fi
    /b/bentwire quit
    sleep 0.5
}

case ${1:-} in
--session) session "$2" ;;
--root)
    s=$2
    case $s in
    pulse) pkgs=(pulseaudio) ;;
    speech) pkgs=(pulseaudio espeak-ng) ;;
    pipewire-pulse) pkgs=(pipewire pipewire-audio wireplumber pipewire-pulse) ;;
    pipewire) pkgs=(pipewire pipewire-audio wireplumber) ;;
    alsa) pkgs=(alsa-utils) ;;
    *) echo "unknown setup $s" >&2; exit 2 ;;
    esac
    sed -i '/^\[options\]/a DisableSandbox' /etc/pacman.conf
    pacman -Syu --noconfirm --needed sway dbus python "${pkgs[@]}" >/out/pacman.log 2>&1 ||
        { tail -20 /out/pacman.log; exit 1; }
    [ "$s" = pipewire ] && rm /etc/alsa/conf.d/50-pipewire.conf
    echo "  installed: $(pacman -Q pulseaudio pipewire pipewire-pulse wireplumber 2>/dev/null | tr '\n' ' ')"
    echo "  absent: $(for p in pipewire-alsa pulseaudio-alsa pipewire-pulse libpulse; do
        pacman -Q $p >/dev/null 2>&1 || printf '%s ' $p; done)"
    useradd -m b
    chmod 777 /out
    su b -c "XDG_RUNTIME_DIR=\$(mktemp -d) dbus-run-session -- bash /run.sh --session $s"
    ;;
*)
    here=$(cd "$(dirname "$0")/.." && pwd)
    bindir=$(realpath "${1:-$here/target/release}")
    shift || true
    setups=("$@")
    [ ${#setups[@]} -gt 0 ] || setups=(pulse pipewire-pulse pipewire alsa speech)
    cache=${BENTWIRE_PACMAN_CACHE:-$HOME/.cache/bentwire/pacman}
    out=$(mktemp -d "${TMPDIR:-/tmp}/audio-backends.XXXXXX")
    mkdir -p "$cache"
    echo "binaries: $bindir; output: $out"
    for s in "${setups[@]}"; do
        echo "== $s"
        mkdir -p "$out/$s"
        podman run --rm -v "$bindir:/b:ro" -v "${BENTWIRE_AUDIO_CHARS:-$here/characters/builtin}:/chars:ro" \
            -v "$here/scripts/audio-backends.sh:/run.sh:ro" -v "$out/$s:/out" \
            -v "$cache:/var/cache/pacman/pkg" docker.io/library/archlinux:latest bash /run.sh --root "$s" ||
            echo "  container failed"
        stats "$out/$s/control.wav" "control tone"
        stats "$out/$s/daemon.wav" "bentwired   "
        if [ "$s" = speech ]; then
            stats "$out/$s/speech.wav" "spoken say  "
            echo "  mouths drawn while speaking (count, value): $(sort "$out/$s/mouths.txt" |
                uniq -c | tr -s ' \n' ' ')"
        fi
        echo "  $(tr '\n' ' ' <"$out/$s/streams.txt" 2>/dev/null)"
        echo "  daemon log: $(grep -c 'ALSA lib' "$out/$s/daemon.log" 2>/dev/null || true) 'ALSA lib' lines"
        grep -E 'audio|sound|speech' "$out/$s/daemon.log" | sed 's/^\[[^]]*\] */    /' | head -8 || true
    done
    ;;
esac
