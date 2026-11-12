"""Freeze, evaluate and report Personal-200 on the current app build.

One model process at a time; synthetic contacts/pages and sandbox email only.
Failed tasks remain results. Evaluation errors/incomplete runs exit nonzero.
"""
import argparse
from collections import Counter, defaultdict
from datetime import datetime, timezone
import fcntl
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import shutil
import signal
import statistics
import subprocess
import sys
import tarfile
import time
import tomllib

from dataset import HERE, SPLITS, digest, validate
from score import assess

ROOT = Path(__file__).resolve().parents[2]


def now():
    return datetime.now(timezone.utc).isoformat()


def write_json(path, value):
    temp = path.with_suffix(path.suffix + ".tmp")
    with temp.open("w") as stream:
        json.dump(value, stream, indent=2, ensure_ascii=False, allow_nan=False)
        stream.write("\n")
        stream.flush()
        os.fsync(stream.fileno())
    temp.replace(path)


def environment():
    return {"platform": platform.platform(), "machine": platform.machine(), "python": sys.version,
            "processor": platform.processor()}


def sources():
    names = subprocess.check_output(["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard"], cwd=ROOT).decode().split("\0")
    extensions = {".rs", ".toml", ".py", ".ts", ".mjs", ".json", ".html", ".css", ".swift", ".kt", ".sh", ".yml", ".xml", ".plist", ".lock", ".h", ".m", ".mm", ".cpp", ".gradle"}
    return sorted({n for n in names if n and (ROOT / n).is_file() and
                   (n in {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml"} or
                    n.startswith(("crate/", "app/", "bench/", "config/", "script/", ".github/")) and Path(n).suffix in extensions)})


def split_sizes(dataset):
    return dataset.get("partitions", SPLITS)


def prepare(out, data, models, seeds):
    dataset = validate(data)
    if out.exists():
        raise ValueError("Output already exists without a run manifest; inspect partial setup")
    if len({m.stem for m in models}) != len(models) or not models or len(set(seeds)) != len(seeds) or not seeds:
        raise ValueError("Models and seeds must be nonempty and unique")
    if any(n < 0 or n > 0xFFFFFFFF for n in seeds):
        raise ValueError("Seed outside u32 range")
    model_info = {m.stem: {"path": str(m.resolve()), "sha256": digest(m)} for m in models}
    out.mkdir(parents=True)
    for name in ("bin", "rows", "logs", "driver"):
        (out / name).mkdir()
    files = sources()
    hashes = {f: digest(ROOT / f) for f in files}
    with (out / "logs/build.log").open("w") as stream:
        subprocess.run(["cargo", "build", "--release", "-p", "pr3-bench-task", "--locked"], cwd=ROOT,
                       stdout=stream, stderr=subprocess.STDOUT, check=True)
    for f, expected in hashes.items():
        if digest(ROOT / f) != expected:
            raise ValueError(f"Source changed during build: {f}")
    binary = out / "bin/pr3-bench-task"
    shutil.copy2(ROOT / "target/release/pr3-bench-task", binary)
    shutil.copytree(data, out / "data")
    validate(out / "data")
    for name in ("run.py", "dataset.py", "score.py", "revision.py"):
        shutil.copy2(HERE / name, out / "driver" / name)
    with tarfile.open(out / "source.tar.gz", "w:gz") as archive:
        for name in files:
            archive.add(ROOT / name, arcname=name)
    for split in split_sizes(dataset):
        subprocess.run([str(binary), "validate", "--corpus", str(out / "data" / f"{split}.toml")], check=True)
    jobs = []
    # Finish development before heldout, with no product/prompt changes in between.
    for split in split_sizes(dataset):
        ids = [r["id"] for r in dataset["cases"] if r["split"] == split]
        for model, info in model_info.items():
            for seed in seeds:
                jobs.append({"name": f"{split}-{model}-seed{seed}", "split": split,
                             "model": model, "seed": seed, "expected": ids})
    pinned = [binary, out / "source.tar.gz", *sorted((out / "driver").glob("*.py")), *sorted((out / "data").iterdir())]
    manifest = {"version": 1, "created_at_utc": now(), "base_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                "dataset_version": dataset["version"], "dataset_manifest_sha256": digest(out / "data/manifest.json"),
                "environment": environment(), "models": model_info, "seeds": seeds, "jobs": jobs,
                "source_sha256": hashes, "files": {str(p.relative_to(out)): digest(p) for p in pinned},
                "build_command": "cargo build --release -p pr3-bench-task --locked",
                "budgets": "Unmodified compose_host limits and LlamaBackend::load_seeded defaults; exact sources archived. No extra tool rounds, approvals or per-case retries.",
                "evaluation_policy": "All declared models/seeds/splits form one frozen evaluation. Preserve failures. Any later runtime tuning makes this version reused regression data, not fresh heldout evidence.",
                "timing_scope": "Desktop turn/first-visible timing from the host; excludes model load and background summary work. No Android or power claim."}
    write_json(out / "manifest.json", manifest)
    return manifest


def verify(out, manifest, check_environment=True, check_models=True):
    if manifest["version"] != 1:
        raise ValueError("Unknown run version")
    for name, expected in manifest["files"].items():
        path = out / name
        if not path.resolve().is_relative_to(out.resolve()) or digest(path) != expected:
            raise ValueError(f"Frozen run input changed: {name}")
    if digest(out / "data/manifest.json") != manifest["dataset_manifest_sha256"]:
        raise ValueError("Dataset identity changed")
    dataset = validate(out / "data")
    expected_jobs = []
    for split in split_sizes(dataset):
        ids = [r["id"] for r in dataset["cases"] if r["split"] == split]
        for model in manifest["models"]:
            for seed in manifest["seeds"]:
                expected_jobs.append({"name": f"{split}-{model}-seed{seed}", "split": split,
                                      "model": model, "seed": seed, "expected": ids})
    if manifest["jobs"] != expected_jobs:
        raise ValueError("Frozen evaluation plan does not cover its declared model/seed/split matrix")
    if check_environment and environment() != manifest["environment"]:
        raise ValueError("Environment changed; preserve this run and start a new output directory")
    if check_models:
        for name, model in manifest["models"].items():
            if digest(model["path"]) != model["sha256"]:
                raise ValueError(f"Model changed: {name}")


def percentile(values, fraction):
    return sorted(values)[max(0, math.ceil(len(values) * fraction) - 1)] if values else None


def failure_flags(verdict, observations):
    observed = {(s['launch'], s['step']): s for s in observations}
    mail_disclosure = any(
        c['what'] in ('card excludes', 'unsent') and
        observed.get((c['launch'], c['step']), {}).get('card' if c['what'] == 'card excludes' else 'sent')
        for c in verdict['failures'])
    return {'mail_disclosure': bool(mail_disclosure),
            'effect_violation': any(c['what'] == 'effects' for c in verdict['failures']),
            'approval_check_failure': any(c['what'] == 'exact approval' for c in verdict['failures'])}


def summarize(out, manifest, allow_pending_tail=False):
    dataset = json.loads((out / "data/manifest.json").read_text())
    meta = {r["id"]: r for r in dataset["cases"]}
    corpora = {s: tomllib.loads((out / "data" / f"{s}.toml").read_text()) for s in split_sizes(dataset)}
    cases = {c["id"]: c for corpus in corpora.values() for c in corpus["case"]}
    results, job_status, files = [], [], {}
    for job in manifest["jobs"]:
        path = out / "rows" / f"{job['name']}.jsonl"
        seen = set()
        if path.exists():
            identity = {"harness": 5, "model_sha256": manifest["models"][job["model"]]["sha256"],
                        "binary_sha256": manifest["files"]["bin/pr3-bench-task"],
                        "corpus_sha256": digest(out / "data" / f"{job['split']}.toml"),
                        "seed": job["seed"], "web": True, "cases": job["expected"]}
            stored = json.loads(path.with_suffix(".manifest.json").read_text())
            if identity != stored:
                raise ValueError("Result identity does not match the frozen build/model/corpus")
            raw = path.read_bytes()
            if raw and not raw.endswith(b"\n"):
                if not allow_pending_tail:
                    raise ValueError(f"Truncated JSONL: {path}")
                raw = raw[:raw.rfind(b"\n") + 1]
            for line in raw.decode().splitlines():
                row = json.loads(line)
                case_id = row.get("id")
                if case_id not in job["expected"] or case_id in seen:
                    raise ValueError(f"Duplicate/foreign row: {path}")
                verdict = assess(cases[case_id], corpora[job["split"]]["filler"], row, job["seed"])
                seen.add(case_id)
                results.append({"job": job["name"], "model": job["model"], "seed": job["seed"],
                                **meta[case_id], **verdict, **failure_flags(verdict, row["steps"]), "observations": row["steps"]})
            files[str(path.relative_to(out))] = hashlib.sha256(raw).hexdigest()
        job_status.append({**job, "recorded": len(seen), "complete": len(seen) == len(job["expected"])})
    groups = []
    for split in split_sizes(dataset):
        for model in manifest["models"]:
            rows = [r for r in results if r["split"] == split and r["model"] == model]
            planned = split_sizes(dataset)[split] * len(manifest["seeds"])
            times = [s["seconds"] for r in rows for s in r["observations"] if s.get("route") == "turn"]
            first = [s["first_visible_ms"] for r in rows for s in r["observations"] if s.get("route") == "turn" and s.get("first_visible_ms") is not None]
            model_turns = [s for r in rows for s in r["observations"] if s.get("route") == "turn" and s.get("prompt_tokens", 0) > 0]
            model_first = [s['first_visible_ms'] for s in model_turns if s.get('first_visible_ms') is not None]
            by = {}
            for field in ("seed", "category", "language", "family"):
                buckets = defaultdict(list)
                for r in rows:
                    buckets[str(r[field])].append(r)
                by[field] = {key: {"recorded": len(values), "passed": sum(r["passed"] for r in values),
                                   "errors": sum(r["error"] for r in values), "leaked": sum(r["leaked"] for r in values)}
                             for key, values in sorted(buckets.items())}
            passed = sum(r["passed"] for r in rows)
            groups.append({"split": split, "model": model, "planned": planned, "recorded": len(rows),
                           "passed": passed, "failed": len(rows) - passed, "errors": sum(r["error"] for r in rows),
                           "leaked": sum(r["leaked"] for r in rows), "success_fraction_planned": passed / planned,
                           "mail_disclosure_runs": sum(r["mail_disclosure"] for r in rows),
                           "unapproved_or_duplicate_effect_runs": sum(r["effect_violation"] for r in rows),
                           "approval_check_failure_runs": sum(r["approval_check_failure"] for r in rows),
                           "complete": len(rows) == planned, "by": by,
                           "failures_by_check": dict(Counter(c["what"] for r in rows for c in r["failures"])),
                           "latency": {"model_turn_count": len(model_turns),
                                       "model_turn_median_seconds": statistics.median([s['seconds'] for s in model_turns]) if model_turns else None,
                                       "model_turn_p95_seconds": percentile([s['seconds'] for s in model_turns], .95),
                                       "model_first_visible_median_ms": statistics.median(model_first) if model_first else None,
                                       "model_first_visible_p95_ms": percentile(model_first, .95),
                                       "turn_count": len(times), "turn_median_seconds": statistics.median(times) if times else None,
                                       "turn_p95_seconds": percentile(times, .95), "first_visible_count": len(first),
                                       "first_visible_median_ms": statistics.median(first) if first else None,
                                       "first_visible_p95_ms": percentile(first, .95)}})
    summary = {"updated_at_utc": now(), "manifest_sha256": digest(out / "manifest.json"),
               "complete": all(j["complete"] for j in job_status), "planned": sum(len(j["expected"]) for j in job_status),
               "recorded": len(results), "passed": sum(r["passed"] for r in results),
               "errors": sum(r["error"] for r in results), "groups": groups, "jobs": job_status,
               "failures": [{k: v for k, v in r.items() if k != "observations"} for r in results if not r["passed"]],
               "result_sha256": files}
    write_json(out / "progress.json", summary)
    lines = [f"# {dataset['version']} current-build evaluation", "", f"Complete: **{summary['complete']}**. Recorded {summary['recorded']}/{summary['planned']}.",
             "", "Partitions are separate. Only fresh-heldout in v3 is new held-out evidence; both prior v2 partitions and email cases are reused regressions. Pending cases stay in the planned denominator; failures/errors are retained." if dataset["version"] == "personal-200-v3" else "Development and heldout are separate. Pending cases stay in the planned denominator; failures/errors are retained.",
             "", "| Split | Model | Recorded / planned | Passed | Failed | Runtime errors | Task leaks |", "|---|---|---:|---:|---:|---:|---:|"]
    for g in groups:
        lines.append(f"| {g['split']} | {g['model']} | {g['recorded']}/{g['planned']} | {g['passed']} | {g['failed']} | {g['errors']} | {g['leaked']} |")
    lines += ["", f"The {len(meta)} synthetic cases belong to {len({r['family'] for r in meta.values()})} families; variants and seeds are related observations. Rates are descriptive, not a population accuracy estimate.",
              "No live email, physical Android, cross-device transport, calendar execution, file sharing or encryption-key custody is measured.",
              "Raw answers and exact effects remain in rows/. progress.json contains category/language/seed/family breakdowns and every failing check.",
              "Timings exclude model loading and background summary work. An error-free runner is not a claim that the evaluated product passed every task."]
    lines += ["", "## Privacy and desktop model-turn latency", "",
              "Mail disclosures count actual forbidden card/outbox content, not absent cards. Model-turn latency includes only turns with recorded prompt tokens; it excludes model load/background summaries and is not Android latency.", "",
              "| Partition | Model | Mail disclosures | Unapproved/duplicate effects | Model turns | Turn median / p95 (s) | First visible median / p95 (ms) |",
              "|---|---|---:|---:|---:|---|---|"]
    def shown(value):
        return '-' if value is None else str(round(value, 2))
    for group in groups:
        latency = group['latency']
        lines.append(f"| {group['split']} | {group['model']} | {group['mail_disclosure_runs']} | {group['unapproved_or_duplicate_effect_runs']} | {latency['model_turn_count']} | {shown(latency['model_turn_median_seconds'])} / {shown(latency['model_turn_p95_seconds'])} | {shown(latency['model_first_visible_median_ms'])} / {shown(latency['model_first_visible_p95_ms'])} |")
    (out / "report.md").write_text("\n".join(lines) + "\n")
    return summary


def stop(child):
    # A departed macOS process group can return EPERM instead of ESRCH under
    # process restrictions. Check membership before signalling, then recheck a
    # permission race; never suppress a failure while owned descendants remain.
    def live_group():
        groups = subprocess.check_output(["ps", "-axo", "pgid="], text=True).split()
        return str(child.pid) in groups

    def signal_group(value):
        if not live_group():
            return
        try:
            os.killpg(child.pid, value)
        except ProcessLookupError:
            pass
        except PermissionError:
            if live_group():
                raise

    signal_group(signal.SIGINT)
    if child.poll() is None:
        try:
            child.wait(timeout=10)
        except subprocess.TimeoutExpired:
            signal_group(signal.SIGKILL)
            child.wait()
    signal_group(signal.SIGKILL)


def evaluate(out, manifest):
    with (out / ".lock").open("w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        progress = summarize(out, manifest)
        for job in progress["jobs"]:
            if job["complete"]:
                continue
            command = [str(out / "bin/pr3-bench-task"), "run", "--model", manifest["models"][job["model"]]["path"],
                       "--corpus", str(out / "data" / f"{job['split']}.toml"), "--seed", str(job["seed"]),
                       "--out", str(out / "rows" / f"{job['name']}.jsonl")]
            with (out / "logs" / f"{job['name']}.log").open("a") as log:
                child = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
                write_json(out / "running.json", {"job": job["name"], "runner_pid": os.getpid(), "child_pid": child.pid,
                                                  "started_at_utc": now(), "command": command})
                print("Starting " + job["name"], flush=True)
                try:
                    while child.poll() is None:
                        time.sleep(5)
                        summarize(out, manifest, allow_pending_tail=child.poll() is None)
                finally:
                    stop(child)
                    summarize(out, manifest)
                if child.returncode:
                    raise RuntimeError(f"Job exited {child.returncode}: {job['name']}; preserve its log and rows")
            progress = summarize(out, manifest)
            if not next(j["complete"] for j in progress["jobs"] if j["name"] == job["name"]):
                raise RuntimeError("Job stopped before all planned cases were recorded")
        progress = summarize(out, manifest)
        write_json(out / "finished.json", {"finished_at_utc": now(), "complete": progress["complete"], "errors": progress["errors"]})
        return 0 if progress["complete"] and not progress["errors"] else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--data", type=Path, default=HERE / "v2")
    parser.add_argument("--model", type=Path, action="append")
    parser.add_argument("--seeds", type=int, nargs="+")
    parser.add_argument("--prepare-only", action="store_true")
    parser.add_argument("--report-only", action="store_true")
    args = parser.parse_args()
    out = args.out.resolve()
    path = out / "manifest.json"
    if not path.exists():
        if args.report_only:
            raise ValueError("No frozen run to report")
        manifest = prepare(out, args.data.resolve(), args.model or [ROOT / "model/Qwen3.5-4B-Q4_K_M.gguf"], args.seeds or [1, 2, 7])
    else:
        manifest = json.loads(path.read_text())
        if args.model and [str(p.resolve()) for p in args.model] != [m["path"] for m in manifest["models"].values()]:
            raise ValueError("Requested models differ from frozen run")
        if args.seeds is not None and args.seeds != manifest["seeds"]:
            raise ValueError("Requested seeds differ from frozen run")
    verify(out, manifest, check_environment=not args.report_only, check_models=not args.report_only)
    frozen = out / "driver/run.py"
    if Path(__file__).resolve() != frozen.resolve():
        # Resume and reporting always execute the frozen scorer, not newly edited code.
        os.execv(sys.executable, [sys.executable, str(frozen), *sys.argv[1:]])
    if args.report_only or args.prepare_only:
        summarize(out, manifest)
        return 0
    return evaluate(out, manifest)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (ValueError, RuntimeError, OSError, subprocess.CalledProcessError) as exc:
        print(f"Personal-200: {exc}", file=sys.stderr)
        raise SystemExit(1)
