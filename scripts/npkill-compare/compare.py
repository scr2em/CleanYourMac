"""Times CleanYourMac's Dependencies scan against npkill on the same folder, warm and cold.

    cargo build --release --bin cym
    npm install --prefix scripts/npkill-compare npkill@0.12.2
    python3 scripts/npkill-compare/compare.py ~/some/folder [runs]          # warm only
    sudo python3 scripts/npkill-compare/compare.py ~/some/folder [runs]     # warm and cold

Both run as whole processes. npkill runs its own scan pipeline without its terminal UI
(run-npkill.mjs). Cold runs drop the file system cache first, which needs root. Use a folder
whose path has no hidden component: npkill skips its last-modified step for those.
"""
import json, os, statistics, subprocess, sys, time

here = os.path.dirname(os.path.abspath(__file__))
cym = os.path.join(here, "../../target/release/cym")
npkill = os.path.join(here, "run-npkill.mjs")
root, runs = os.path.abspath(sys.argv[1]), int(sys.argv[2]) if len(sys.argv) > 2 else 5


def drop_caches():
    subprocess.run(["sync"], check=True)
    if sys.platform == "darwin":
        return subprocess.run(["purge"]).returncode == 0
    try:
        with open("/proc/sys/vm/drop_caches", "w") as f:
            f.write("3")
        return True
    except OSError:
        return False


def timed(cmd):
    start = time.perf_counter()
    out = subprocess.run(cmd, capture_output=True, text=True, check=True).stdout
    return (time.perf_counter() - start) * 1000, out


def run_cym():
    ms, out = timed([cym, "scan", "node", root, "--json"])
    findings = json.loads(out)["findings"]
    modules = [f for f in findings if f["resource"]["file"]["path"].endswith("/node_modules")]
    return ms, {
        "node_modules": len(modules),
        "allocated_kb": sum(f["allocatedBytes"] or 0 for f in modules) // 1024,
        "all_findings": len(findings),
    }


def run_npkill():
    ms, out = timed(["node", npkill, root])
    result = json.loads(out)
    return ms, {"node_modules": result["results"], "allocated_kb": result["allocated_kb"],
                "found_ms": result["found_ms"]}


states = ["warm"] + (["cold"] if drop_caches() else [])
for state in states:
    for name, run in [("CleanYourMac", run_cym), ("npkill", run_npkill)]:
        run()
        times, detail = [], None
        for _ in range(runs):
            if state == "cold":
                drop_caches()
            ms, detail = run()
            times.append(ms)
        print(f"{state} · {name}: median {statistics.median(times):.0f} ms "
              f"(min {min(times):.0f}, max {max(times):.0f}) · {detail}")
if states == ["warm"]:
    print("Cold runs skipped: dropping the file system cache needs root.")
