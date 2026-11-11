"""Check that bentwire Character1 signals in a dbus-monitor log contain the given members
as an ordered subsequence.

usage: signals.py dbus-monitor.log MEMBER...
"""
import re
import sys

log = open(sys.argv[1]).read()
got = re.findall(r"interface=org\.bentwire\.Character1; member=(\w+)", log)
want = sys.argv[2:]
i = 0
for m in got:
    if i < len(want) and m == want[i]:
        i += 1
print("    signals:", " ".join(got))
if i < len(want):
    print("    missing from", want[i], "on:", " ".join(want[i:]))
    sys.exit(1)
