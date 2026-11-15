#!/usr/bin/env python3
"""bentwire D-Bus example in Python with PyGObject (Gio), docs/dbus.md.

Package: python3-gi (Debian, Ubuntu), python-gobject (Arch), python3-gobject (Fedora).
Speaks, plays an animation, asks a question with two buttons and prints the answer,
then prints Click, DblClick, Shown and Hidden until Ctrl+C.
"""

import sys

from gi.repository import Gio, GLib

NAME = "org.bentwire.Agent"
PATH = "/org/bentwire/Character"
IFACE = "org.bentwire.Character1"

bus = Gio.bus_get_sync(Gio.BusType.SESSION)
loop = GLib.MainLoop()
pending = {}  # request id -> callback(status, reason)
answers = {}  # ask id -> (result, checked)


def call(method, sig=None, *args):
    """Call a Character1 method. The first call D-Bus-activates bentwired if needed."""
    params = GLib.Variant(sig, args) if sig else None
    reply = bus.call_sync(NAME, PATH, IFACE, method, params, None,
                          Gio.DBusCallFlags.NONE, -1, None)
    return reply.unpack()[0] if reply.get_type_string() != "()" else None


def on_signal(_conn, _sender, _path, _iface, member, params):
    args = params.unpack()
    if member == "AskComplete":  # unicast, sent before RequestComplete (B-ASK-4)
        answers[args[0]] = (args[1], args[2])
    elif member == "RequestComplete":  # unicast (B-Q-2)
        done = pending.pop(args[0], None)
        if done:
            done(args[1], args[2])
    elif member in ("Click", "DblClick", "Shown", "Hidden"):  # broadcast
        print(member, *args, flush=True)


# Subscribe before the first call so that no signal is missed. The match rule also
# receives the unicast request signals addressed to this connection.
bus.signal_subscribe(NAME, IFACE, None, PATH, None, Gio.DBusSignalFlags.NONE, on_signal)


def result_name(result):
    if result == 0:
        return "none (dismissed or interrupted)"
    if result >= 1:
        return f"label {result}"
    if result <= -25:
        return f"button {-24 - result}"
    return f"standard button {result}"  # -1 OK, -2 Cancel, -3 Yes, ... (B-ASK-2)


def asked(ask_id):
    def done(status, reason):
        if status != 0:
            print(f"ask ended with status {status} {reason}", flush=True)
        result, _checked = answers.pop(ask_id, (0, []))
        print("answer:", result_name(result), flush=True)
        print("Listening for Click, DblClick, Shown, Hidden (Ctrl+C to quit)", flush=True)
    return done


def spoken(status, reason):
    print("spoken:", status, reason or "ok", flush=True)


try:
    call("Show", "(b)", False)
    pending[call("Speak", "(s)", "Hello from Python!")] = spoken
    call("Play", "(s)", "Wave")
    spec = {
        "heading": GLib.Variant("s", "Python example"),
        "text": GLib.Variant("s", "Coffee or tea?"),
        "buttons": GLib.Variant("as", ["&Coffee", "&Tea"]),
    }
    ask_id = call("Ask", "(a{sv})", spec)
    pending[ask_id] = asked(ask_id)
except GLib.Error as e:
    # e.message starts with the D-Bus error name, e.g. GDBus.Error:org.bentwire.Error...
    sys.exit(f"bentwire: {e.message}")

try:
    loop.run()
except KeyboardInterrupt:
    pass
