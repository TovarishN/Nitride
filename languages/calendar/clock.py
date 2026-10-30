#!/usr/bin/env python3
"""Calendar.Clock over JSON-RPC on stdin and stdout, as clock.nbind binds it.

Each line in is a request; each line out answers it. Dates are day numbers
since 0001-01-01, the current one in UTC.
"""
import datetime
import json
import sys


def today():
    return datetime.datetime.now(datetime.timezone.utc).date().toordinal() - 1


METHODS = {"today": today}

for line in sys.stdin:
    request = json.loads(line)
    method = METHODS.get(request.get("method"))
    if method is None:
        response = {"error": {"code": -32601, "message": "no such method"}}
    else:
        response = {"result": method(*request.get("params", []))}
    response.update(jsonrpc="2.0", id=request.get("id"))
    print(json.dumps(response), flush=True)
