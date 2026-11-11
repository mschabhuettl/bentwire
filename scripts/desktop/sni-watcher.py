"""Minimal org.kde.StatusNotifierWatcher for tests: accepts item registrations and
prints each registered service (one line per item) on stdout.

usage: python3 -I sni-watcher.py   (runs until killed; needs python3-gi)
"""
import sys

from gi.repository import Gio, GLib

XML = """
<node>
  <interface name="org.kde.StatusNotifierWatcher">
    <method name="RegisterStatusNotifierItem"><arg type="s" direction="in"/></method>
    <method name="RegisterStatusNotifierHost"><arg type="s" direction="in"/></method>
    <property name="RegisteredStatusNotifierItems" type="as" access="read"/>
    <property name="IsStatusNotifierHostRegistered" type="b" access="read"/>
    <property name="ProtocolVersion" type="i" access="read"/>
    <signal name="StatusNotifierItemRegistered"><arg type="s"/></signal>
    <signal name="StatusNotifierItemUnregistered"><arg type="s"/></signal>
    <signal name="StatusNotifierHostRegistered"/>
  </interface>
</node>
"""
items = []


def call(conn, sender, path, iface, method, params, inv):
    if method == "RegisterStatusNotifierItem":
        arg = params.unpack()[0]
        svc = sender if arg.startswith("/") else arg
        path = arg if arg.startswith("/") else "/StatusNotifierItem"
        items.append(svc)
        print(f"{svc} {path}", flush=True)
        conn.emit_signal(None, "/StatusNotifierWatcher", "org.kde.StatusNotifierWatcher",
                         "StatusNotifierItemRegistered", GLib.Variant("(s)", (svc,)))
    inv.return_value(None)


def prop(conn, sender, path, iface, name):
    return {
        "RegisteredStatusNotifierItems": GLib.Variant("as", items),
        "IsStatusNotifierHostRegistered": GLib.Variant("b", True),
        "ProtocolVersion": GLib.Variant("i", 0),
    }[name]


def acquired(conn, name):
    info = Gio.DBusNodeInfo.new_for_xml(XML).interfaces[0]
    conn.register_object("/StatusNotifierWatcher", info, call, prop, None)


Gio.bus_own_name(Gio.BusType.SESSION, "org.kde.StatusNotifierWatcher",
                 Gio.BusNameOwnerFlags.NONE, acquired, None, lambda *a: sys.exit(1))
GLib.MainLoop().run()
