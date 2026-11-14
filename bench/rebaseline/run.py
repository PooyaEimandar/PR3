"""Frozen, resumable full rebaseline. Run with bench/agentdojo/.venv/bin/python.

  python bench/rebaseline/run.py --out bench/rebaseline/result/2026-11-04
  python bench/rebaseline/run.py --out bench/rebaseline/result/2026-11-04 --report-only

One inference process at a time. This is a desktop measurement, never Android data.
No real email, contacts, credentials, web fetches or user databases are used.
"""
import argparse
import ast
import fcntl
import hashlib
import importlib.metadata
import json
import math
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tarfile
import time

ROOT = Path(__file__).resolve().parents[2]
SEEDS = [1, 2, 7]
MODES = ["pr3", "plain", "confirm_all"]
SUITES = ["banking", "workspace", "slack", "travel"]
MODELS = ["Qwen3.5-4B-Q4_K_M", "Qwen3.8-27B-Q4_K_M"]
PHONE = "Qwen3.5-2B-Q4_K_M"


def digest(path):
    with Path(path).open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def harness_version(driver):
    """Read the frozen driver's version without importing/executing it."""
    for node in ast.parse(Path(driver).read_text()).body:
        if isinstance(node, ast.Assign) and any(isinstance(t, ast.Name) and t.id == "HARNESS_VERSION" for t in node.targets):
            value = ast.literal_eval(node.value)
            if type(value) is int and value > 0:
                return value
    raise ValueError(f"Missing/invalid HARNESS_VERSION in {driver}")


def dataset_digest():
    import agentdojo
    package = Path(agentdojo.__file__).parent
    files = {str(p.relative_to(package)): digest(p) for p in sorted(package.rglob("*")) if p.is_file() and p.suffix in {".py", ".json", ".yaml", ".yml", ".txt"}}
    return hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest()


def write_json(path, data):
    temp = path.with_suffix(path.suffix + f".{os.getpid()}.tmp")
    with temp.open("w") as f:
        json.dump(data, f, indent=2, sort_keys=True)
        f.write("\n")
        f.flush()
        os.fsync(f.fileno())
    temp.replace(path)


def source_files():
    names = subprocess.check_output(["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard"], cwd=ROOT).decode().split("\0")
    return sorted({n for n in names if n and (ROOT / n).is_file() and (
        n in {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "config/pr3.toml"}
        or (n.startswith(("crate/", "app/pr3-app/", "bench/", "script/", ".github/", "config/"))
            and Path(n).suffix in {".rs", ".toml", ".py", ".ts", ".mjs", ".json", ".html", ".css", ".swift", ".kt", ".sh", ".yml", ".xml", ".plist", ".lock", ".h", ".m", ".mm", ".cpp", ".gradle"})
    )})


def prepare(out):
    if "legacy-email-bench" in (ROOT / "bench/task/Cargo.toml").read_text():
        raise ValueError("Email workflows are retired. Use the frozen run/evidence or its recorded source revision; new evaluation lives in bench/documents.")
    import tomllib
    from agentdojo.task_suite.load_suites import get_suite

    binaries = {"task": ROOT / "target/release/pr3-bench-task", "security": ROOT / "target/release/pr3-bench-agentdojo"}
    for path in binaries.values():
        if not path.is_file():
            raise ValueError(f"Build the release benchmark binaries first: {path}")
    models = {name: ROOT / "model" / f"{name}.gguf" for name in [*MODELS, PHONE]}
    model_info = {name: {"path": str(path), "sha256": digest(path), "bytes": path.stat().st_size} for name, path in models.items()}
    files = source_files()
    hashes = {name: digest(ROOT / name) for name in files}
    version = importlib.metadata.version("agentdojo")
    if version != "0.1.35":
        raise ValueError(f"Expected pinned agentdojo 0.1.35, got {version}")
    out.mkdir(parents=True, exist_ok=True)
    if any(out.iterdir()):
        raise ValueError("New output directory must be empty; a partial setup must be inspected first")
    (out / "bin").mkdir()
    (out / "logs").mkdir()
    (out / "rows").mkdir()
    for kind, path in binaries.items():
        shutil.copy2(path, out / "bin" / path.name)
    driver = out / "driver"
    shutil.copytree(ROOT / "bench/agentdojo/pr3_agentdojo", driver / "pr3_agentdojo", ignore=shutil.ignore_patterns("__pycache__"))
    shutil.copy2(ROOT / "bench/agentdojo/run.py", driver / "run.py")
    shutil.copy2(ROOT / "bench/task/corpus.toml", out / "corpus.toml")
    shutil.copy2(ROOT / "app/pr3-app/src-tauri/src/memory/semantic_cases.json", out / "memory.json")
    with tarfile.open(out / "source.tar.gz", "w:gz") as archive:
        for name in files:
            archive.add(ROOT / name, arcname=name)
    corpus = tomllib.loads((out / "corpus.toml").read_text())
    cases = [c["id"] for c in corpus["case"]]
    jobs = []
    memory_cases = [c["name"] for c in json.loads((out / "memory.json").read_text())]
    # The complete historical task suite on both phone classes, plus the 27B
    # reference. All four security suites retain all three comparison arms.
    for model in [MODELS[0], PHONE, MODELS[1]]:
        for seed in SEEDS:
            memory_name = f"memory-{model}-seed{seed}"
            memory_result = out / "rows" / f"{memory_name}.jsonl"
            jobs.append({"name": memory_name, "kind": "memory", "model": model, "seed": seed, "expected": memory_cases, "result": str(memory_result), "command": [str(out / "bin/pr3-bench-task"), "run-memory", "--model", str(models[model]), "--seed", str(seed), "--corpus", str(out / "memory.json"), "--out", str(memory_result)]})
            name = f"workflow-{model}-seed{seed}"
            result = out / "rows" / f"{name}.jsonl"
            jobs.append({"name": name, "kind": "workflow", "model": model, "seed": seed, "expected": cases, "result": str(result), "command": [str(out / "bin/pr3-bench-task"), "run", "--model", str(models[model]), "--seed", str(seed), "--corpus", str(out / "corpus.toml"), "--out", str(result)]})
    for seed in SEEDS:
        for model in MODELS:
            for suite_name in SUITES:
                suite = get_suite("v1.2.2", suite_name)
                expected = [[mode, task, injection] for mode in MODES for task in suite.user_tasks for injection in [None, *suite.injection_tasks]]
                name = f"security-{suite_name}-{model}-seed{seed}"
                result = out / "rows" / f"{name}.jsonl"
                jobs.append({"name": name, "kind": "security", "model": model, "suite": suite_name, "seed": seed, "expected": expected, "result": str(result), "command": [sys.executable, str(driver / "run.py"), "--bridge", str(out / "bin/pr3-bench-agentdojo"), "--model", str(models[model]), "--suite", suite_name, "--seed", str(seed), "--modes", *MODES, "--out", str(result)]})
    pinned = [*sorted(p for p in (out / "bin").iterdir()), out / "corpus.toml", out / "memory.json", driver / "run.py", *sorted((driver / "pr3_agentdojo").glob("*.py")), out / "source.tar.gz"]
    for job in jobs:
        job["inputs"] = {"model_sha256": model_info[job["model"]]["sha256"], "seed": job["seed"]}
        if job["kind"] == "security":
            job["inputs"].update({"bridge_sha256": digest(out / "bin/pr3-bench-agentdojo"), "suite": job["suite"], "harness": harness_version(driver / "run.py"), "expected": sorted(job["expected"], key=lambda x: (x[0], x[1], x[2] or ""))})
        else:
            job["inputs"].update({"binary_sha256": digest(out / "bin/pr3-bench-task"), "corpus_sha256": digest(out / ("memory.json" if job["kind"] == "memory" else "corpus.toml")), "harness": 2, "cases": job["expected"], "web": job["kind"] == "workflow"})
    manifest = {
        "schema": 1, "created_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "git_head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "source_files": hashes, "source_digest": hashlib.sha256(json.dumps(hashes, sort_keys=True).encode()).hexdigest(),
        "frozen_files": {str(p.relative_to(out)): digest(p) for p in pinned},
        "models": model_info, "seeds": SEEDS, "jobs": jobs,
        "environment": {"os": platform.platform(), "architecture": platform.machine(), "python": sys.version, "cpu_count": os.cpu_count(), "agentdojo": version,
            "agentdojo_source_and_data_sha256": dataset_digest(),
            "rustc": subprocess.check_output(["rustc", "-Vv"], cwd=ROOT, text=True).strip(),
            "hardware": subprocess.check_output(["sysctl", "-n", "machdep.cpu.brand_string", "hw.memsize"], text=True).strip() if sys.platform == "darwin" else platform.processor(),
            "dependencies": subprocess.check_output([sys.executable, "-m", "pip", "freeze"], text=True).splitlines()},
        "limits": {"security_max_rounds": 10, "security_context": 16384, "security_max_new_tokens": 1024,
            "workflow_context": 4096, "workflow_max_new_tokens": 512,
            "temperature": 0.7, "top_p": 0.8, "top_k": 20, "presence_penalty": 1.5, "thinking": False,
            "attack": "important_instructions", "benchmark": "v1.2.2", "approver": "scripted oracle, not a human", "concurrency": 1},
        "caveats": ["Desktop only; Android latency/permissions are pending physical-device testing.", "Task mail is a sandbox; native E2E uses fixture mail handoff. Neither proves delivery.", "Answer checks reject listed contradictions; they are not a general semantic truth judge.", "Scripted approver and AgentDojo tool environments differ from the app's device-mail connector.", "Existing development corpus, not a held-out user study."],
    }
    write_json(out / "manifest.json", manifest)
    return manifest


def read_rows(path):
    if not path.exists():
        return [], None
    rows = []
    for line in path.read_text().splitlines():
        try:
            rows.append(json.loads(line))
        except json.JSONDecodeError as e:
            return rows, f"Invalid/incomplete JSONL: {e}"
    return rows, None


def job_status(job):
    path = Path(job["result"])
    rows, problem = read_rows(path)
    run_id = None
    if rows and job.get("inputs"):
        sidecar = path.with_suffix(path.suffix + ".manifest.json") if job["kind"] == "security" else path.with_suffix(".manifest.json")
        try:
            metadata = json.loads(sidecar.read_text())
            config = metadata["config"] if job["kind"] == "security" else metadata
            run_id = metadata.get("run_id")
            if any(config.get(key) != value for key, value in job["inputs"].items()):
                problem = "Result manifest does not match this job's frozen inputs"
        except (OSError, ValueError, KeyError) as error:
            problem = f"Missing/invalid result manifest: {error}"
    expected = {json.dumps(x) for x in job["expected"]}
    seen = set()
    for row in rows:
        key = row["id"] if job["kind"] != "security" else [row["mode"], row["user_task"], row["injection_task"]]
        key = json.dumps(key)
        if key not in expected or key in seen or row.get("seed") != job["seed"]:
            problem = "Foreign, duplicate, unexpected, or wrong-seed row"
        if job["kind"] == "security" and job.get("inputs") and row.get("run_id") != run_id:
            problem = "Foreign result run_id"
        seen.add(key)
    errors = sum(bool(r.get("error")) for r in rows)
    complete = seen == expected and not problem
    return {"rows": rows, "count": len(seen & expected), "expected": len(expected), "errors": errors, "problem": problem, "complete": complete}


def percentile(values, p):
    ordered = sorted(values)
    return round(ordered[max(0, math.ceil(len(ordered) * p) - 1)], 3) if ordered else None


def report(out, manifest):
    jobs = []
    lines = ["# PR3 desktop rebaseline", "", "**Progress, not a release claim.** Android testing and the 30-day study are separate.", "",
             "| Job | Recorded / planned | Errors | Result | Latency p50 / p95 (s) |", "|---|---:|---:|---|---|"]
    for job in manifest["jobs"]:
        state = job_status(job)
        rows = state.pop("rows")
        jobs.append({"name": job["name"], **state})
        if job["kind"] != "security":
            result = f"{sum(bool(r['passed']) for r in rows)}/{len(rows)} cases passed"
            failed = [r["id"] for r in rows if not r["passed"]]
            if failed:
                result += "; failed: " + ", ".join(failed)
            if job["kind"] == "memory":
                result += f"; incomplete reviews: {sum(bool(r['incomplete']) for r in rows)}"
            times = [s["seconds"] for r in rows for s in r["steps"] if s.get("route") == "turn"] if job["kind"] == "workflow" else [r["seconds"] for r in rows]
            if job["kind"] == "workflow":
                visible = [s["first_visible_ms"] / 1000 for r in rows for s in r["steps"] if s.get("first_visible_ms") is not None]
                result += f"; first visible p50/p95 {percentile(visible, .5)}/{percentile(visible, .95)}s (n={len(visible)})"
        else:
            parts = []
            for mode in MODES:
                rs = [r for r in rows if r["mode"] == mode]
                scored = [r for r in rs if r["injection_task"] is not None and r["attack_succeeded"] is not None and not r.get("error")]
                benign = [r for r in rs if r["injection_task"] is None]
                attacked = [r for r in rs if r["injection_task"] is not None]
                parts.append(f"{mode}: ASR {sum(bool(r['attack_succeeded']) for r in scored)}/{len(scored)}; utility B {sum(bool(r['utility']) for r in benign)}/{len(benign)}, A {sum(bool(r['utility']) for r in attacked)}/{len(attacked)}")
            result = "<br>".join(parts)
            times = [r["wall_seconds"] for r in rows]
        if state["problem"]:
            result = f"INVALID: {state['problem']}"
        lines.append(f"| {job['name']} | {state['count']} / {state['expected']} | {state['errors']} | {result} | {percentile(times, .5)} / {percentile(times, .95)} |")
    completed = sum(j["count"] for j in jobs)
    planned = sum(j["expected"] for j in jobs)
    complete = all(j["complete"] for j in jobs)
    errors = sum(j["errors"] for j in jobs)
    state = {"updated_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), "recorded": completed, "planned": planned, "complete": complete, "errors": errors, "jobs": jobs}
    lines[2] = f"**{'Matrix finished' if complete else 'IN PROGRESS'}: {completed}/{planned} records; {errors} evaluation errors.** Android results are not included."
    lines.extend(["", "ASR denominators include only scored attacked runs; errors are unscored, never evidence of safety. Utility includes failed runs. Latencies are desktop end-to-end turns/runs, not engine token speed; workflow summaries and model load are outside turn latency. Small/incomplete samples are not comparable to a completed baseline.", "", *manifest["caveats"], ""])
    write_json(out / "progress.json", state)
    temp = out / f"report.md.{os.getpid()}.tmp"
    temp.write_text("\n".join(lines))
    temp.replace(out / "report.md")
    return state


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--report-only", action="store_true")
    args = parser.parse_args()
    out = args.out.resolve()
    manifest_path = out / "manifest.json"
    if not manifest_path.exists() and args.report_only:
        parser.error("no manifest to report")
    manifest = json.loads(manifest_path.read_text()) if manifest_path.exists() else prepare(out)
    if args.report_only:
        print(json.dumps(report(out, manifest), indent=2))
        return 0
    with (out / "runner.lock").open("w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        if (platform.platform() != manifest["environment"]["os"]
            or sys.version != manifest["environment"]["python"]
            or dataset_digest() != manifest["environment"]["agentdojo_source_and_data_sha256"]
            or subprocess.check_output([sys.executable, "-m", "pip", "freeze"], text=True).splitlines() != manifest["environment"]["dependencies"]):
            raise ValueError("Runtime environment changed; use a new rebaseline directory")
        for name, wanted in manifest["frozen_files"].items():
            if digest(out / name) != wanted:
                raise ValueError(f"Frozen input changed: {name}")
        for name, model in manifest["models"].items():
            if digest(model["path"]) != model["sha256"]:
                raise ValueError(f"Model changed: {name}")
        report(out, manifest)
        for job in manifest["jobs"]:
            state = job_status(job)
            if state["problem"]:
                raise ValueError(f"{job['name']}: {state['problem']}")
            if state["complete"]:
                continue
            print(f"Starting {job['name']}: {state['count']}/{state['expected']}", flush=True)
            with (out / "logs" / f"{job['name']}.log").open("a") as log:
                process = subprocess.Popen(job["command"], cwd=ROOT, stdout=log, stderr=log)
                write_json(out / "running.json", {"runner_pid": os.getpid(), "child_pid": process.pid, "job": job["name"], "started_at": time.time()})
                try:
                    while True:
                        try:
                            code = process.wait(timeout=30)
                            break
                        except subprocess.TimeoutExpired:
                            report(out, manifest)
                except BaseException:
                    process.terminate()
                    try: process.wait(timeout=10)
                    except subprocess.TimeoutExpired: process.kill(); process.wait()
                    raise
            report(out, manifest)
            if code != 0 or not job_status(job)["complete"]:
                raise RuntimeError(f"{job['name']} stopped (exit {code}); inspect log and resume the same command")
        state = report(out, manifest)
        write_json(out / "running.json", {"finished_at": time.time(), "complete": state["complete"], "errors": state["errors"]})
        return 1 if state["errors"] else 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (ValueError, RuntimeError, OSError) as error:
        print(f"rebaseline: {error}", file=sys.stderr)
        sys.exit(1)
