"""Calendar.Gregorian version 3 over line-delimited JSON-RPC 2.0.

Dates are zero-based Gregorian ordinals; duration values are finite day counts.
This process requires Python 3.9+ and uses only the standard library.
"""
import datetime
import json
import math
import sys

LAST = datetime.date.max.toordinal() - 1
WEEKDAYS = ("Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday")


class Failure(Exception):
    pass


def date(value):
    if not 0 <= value <= LAST:
        raise Failure("out-of-range")
    return datetime.date.fromordinal(value + 1)


def parse_date(text):
    try:
        parsed = datetime.date.fromisoformat(text)
        if parsed.isoformat() != text:
            raise ValueError()
        return parsed.toordinal() - 1
    except ValueError:
        raise Failure("invalid-date") from None


def later(value, days):
    if not math.isfinite(days) or days != math.trunc(days):
        raise Failure("partial-day")
    result = value + int(days)
    date(result)
    return result


def mondays(span):
    first, last = span["start"], span["end"]
    date(first)
    date(last)
    start = first + (-first % 7)
    return list(range(start, last + 1, 7))


METHODS = {
    "parse-date": parse_date,
    "later": later,
    "earlier": lambda value, days: later(value, -days),
    "between": lambda left, right: float(left - right),
    "day-of-week": lambda value: WEEKDAYS[date(value).weekday()],
    "is-leap-year": lambda value: (date(value).year % 4 == 0 and
                                    (date(value).year % 100 != 0 or date(value).year % 400 == 0)),
    "mondays": mondays,
}

for line in sys.stdin:
    request = json.loads(line)
    method = METHODS.get(request.get("method"))
    try:
        response = ({"result": method(*request.get("params", []))} if method else
                    {"error": {"code": -32601, "message": "no such method"}})
    except Failure as failure:
        response = {"error": {"code": 1, "message": str(failure),
                              "data": {"failure": str(failure)}}}
    response.update(jsonrpc="2.0", id=request.get("id"))
    print(json.dumps(response, allow_nan=False), flush=True)
