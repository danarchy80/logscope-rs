import sys
sys.path.insert(0, "/home/danarchy/workspace/logscope/src")
from logscope.core.parser import parse_line
from datetime import timezone

inputs = [
    "2026-08-26T10:00:00Z INFO",
    "2026-08-26T10:00:00.123456Z INFO",
    "2026-08-26T10:00:00+00:00 INFO",
    "2026-08-26T10:00:00+0000 INFO",
    "2026-08-26T10:00:00.5+00:00 INFO",
    "2026-08-26 10:00:00 INFO",
    "2026-08-26 10:00:00.123 INFO",
    "2026-08-26 10:00:00 +0200 INFO",
    "2026-08-26 10:00:00 +02:00 INFO",
    "2026-08-26 10:00:00+0200 INFO",
    "26/Aug/2026:10:00:00 +0000 INFO",
    "26/Aug/2026:10:00:00 +00:00 INFO",
    "Aug 26 10:00:00 INFO",
    "This has no timestamp",
    "  File \"app.py\", line 42",
    "2026-08-26 10:00:00.123456 INFO",
    "2026-08-26T10:00:00.123+02:00 INFO",
    "Dec 31 23:59:59 INFO",
]
for s in inputs:
    dt = parse_line(s)
    if dt is None:
        print(f"{s}\t=>\tNone")
    else:
        if dt.tzinfo is None:
            dt = dt.replace(tzinfo=timezone.utc)
        else:
            dt = dt.astimezone(timezone.utc)
        # match chrono to_rfc3339_opts(Micros, true): e.g. 2026-08-26T10:00:00.123456Z
        if dt.microsecond:
            print(f"{s}\t=>\t{dt.strftime('%Y-%m-%dT%H:%M:%S.%f')}Z")
        else:
            print(f"{s}\t=>\t{dt.strftime('%Y-%m-%dT%H:%M:%S')}Z")
