"""A no-op content plugin for tests: reads {request, seed, out} as JSON on stdin, writes the
request's text into `out` and reports its length on stdout. Fails when asked to."""
import json
import sys

job = json.load(sys.stdin)
if job["request"].get("fail"):
    print("asked to fail", file=sys.stderr)
    sys.exit(3)
text = job["request"]["text"]
with open(job["out"], "wb") as f:
    f.write(text.encode())
print(json.dumps({"info": {"bytes": len(text), "seed": job["seed"]}}))
