#!/usr/bin/env python3
"""Compare Criterion benchmark results against a committed baseline.

This walks Criterion's per-benchmark ``estimates.json`` files under
``target/criterion`` (always produced by ``cargo bench``), extracts the mean
execution time, and compares it to the previously recorded baseline stored in
``baseline.json``.

Exit codes:
  0  success (no regression, or no previous baseline to compare against)
  1  one or more benchmarks regressed beyond the threshold

The (possibly updated) baseline is always written back to ``baseline.json`` so
the caller can commit it (e.g. on the ``main`` branch) to refresh the reference
numbers used for future regression checks.
"""
import glob
import json
import os
import sys

THRESHOLD_DEFAULT = 10.0  # percent


def find_estimates():
    """Walk target/criterion and return {name: mean_seconds}."""
    results = {}
    for path in glob.glob("target/criterion/**/estimates.json", recursive=True):
        # path looks like target/criterion/<group>/<id>/estimates.json
        parts = path.split(os.sep)
        try:
            idx = parts.index("criterion")
        except ValueError:
            continue
        rest = parts[idx + 1:]
        if len(rest) < 3:
            continue
        group = rest[-3]
        bench_id = rest[-2]
        # Criterion URL-encodes '/' as '%2F'
        name = (group + "/" + bench_id).replace("%2F", "/")
        try:
            with open(path) as f:
                data = json.load(f)
            mean = float(data["mean"]["estimate"])
        except Exception as e:  # noqa: BLE001 - defensive: never crash CI on parse issues
            print(f"warn: could not parse {path}: {e}", file=sys.stderr)
            continue
        results[name] = mean
    return results


def load_baseline(path):
    if not os.path.exists(path):
        return {}
    try:
        with open(path) as f:
            data = json.load(f)
        return data.get("benchmarks", {})
    except Exception:  # noqa: BLE001
        return {}


def main():
    if len(sys.argv) < 2:
        print("usage: benchmark_guard.py <baseline.json> [threshold_pct]", file=sys.stderr)
        sys.exit(2)

    baseline_path = sys.argv[1]
    threshold = float(sys.argv[2]) if len(sys.argv) > 2 else THRESHOLD_DEFAULT

    current = find_estimates()
    baseline = load_baseline(baseline_path)

    if not current:
        print("warn: no benchmark estimates found under target/criterion; "
              "skipping regression check.", file=sys.stderr)
        # Still refresh the baseline file so a later run can pick it up.
        with open(baseline_path, "w") as f:
            json.dump({"benchmarks": current}, f, indent=2)
        sys.exit(0)

    rows = []
    regressions = []
    for name, mean in sorted(current.items()):
        if name in baseline and baseline[name] > 0:
            pct = (mean - baseline[name]) / baseline[name] * 100.0
            rows.append((name, baseline[name], mean, pct))
            if pct > threshold:
                regressions.append((name, pct))
        else:
            rows.append((name, None, mean, None))

    summary_path = os.environ.get("GITHUB_STEP_SUMMARY")
    lines = [
        "## Benchmark results", "",
        f"Regression threshold: +{threshold:.0f}%", "",
        "| Benchmark | Baseline (s) | Current (s) | Change |",
        "|---|---|---|---|",
    ]
    for name, base, cur, pct in rows:
        if base is None:
            change = "new"
            base_s = "—"
        else:
            change = f"{pct:+.1f}%"
            base_s = f"{base:.6f}"
        lines.append(f"| {name} | {base_s} | {cur:.6f} | {change} |")
    table = "\n".join(lines)
    print(table)
    if summary_path:
        try:
            with open(summary_path, "a") as f:
                f.write(table + "\n")
        except Exception as e:  # noqa: BLE001
            print(f"warn: could not write summary: {e}", file=sys.stderr)

    # Always refresh the baseline file.
    with open(baseline_path, "w") as f:
        json.dump({"benchmarks": current}, f, indent=2)

    if regressions:
        print("\nREGRESSIONS DETECTED:", file=sys.stderr)
        for n, p in regressions:
            print(f"  {n}: +{p:.1f}%", file=sys.stderr)
        sys.exit(1)
    sys.exit(0)


if __name__ == "__main__":
    main()
