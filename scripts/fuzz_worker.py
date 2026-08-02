"""Worker process: reads JSON test cases from stdin, one per line, runs
them against whichever `semantic_version` is importable in this venv,
writes JSON results to stdout. Used by differential_fuzz.py to compare
this repo's Rust port against the real PyPI package without an import
name collision (both packages are named `semantic_version`).
"""
import sys
import json
import semantic_version as sv


def run_case(case):
    op = case["op"]
    try:
        if op == "parse":
            v = sv.Version(case["text"])
            return {
                "ok": True,
                "major": v.major, "minor": v.minor, "patch": v.patch,
                "prerelease": list(v.prerelease), "build": list(v.build),
                "str": str(v),
            }
        elif op == "compare":
            result = sv.compare(case["a"], case["b"])
            return {"ok": True, "result": None if result is NotImplemented else result}
        elif op == "match":
            result = sv.match(case["spec"], case["version"])
            return {"ok": True, "result": result}
        elif op == "spec_contains":
            spec = sv.SimpleSpec(case["spec"])
            v = sv.Version(case["version"])
            return {"ok": True, "result": v in spec}
        elif op == "npm_contains":
            spec = sv.NpmSpec(case["spec"])
            v = sv.Version(case["version"])
            return {"ok": True, "result": v in spec}
        elif op == "validate":
            return {"ok": True, "result": sv.validate(case["text"])}
        else:
            return {"ok": False, "error": f"unknown op {op}"}
    except Exception as e:
        return {"ok": False, "error": f"{type(e).__name__}: {e}"}


for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    case = json.loads(line)
    result = run_case(case)
    print(json.dumps(result), flush=True)
