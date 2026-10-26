"""Runs PR3's AgentDojo comparison (docs Sec. 8.1, spike step 8).

Every arm uses the same model file, engine, prompts, tool definitions and attack; only
the agent loop differs (see bridge/src/main.rs). For each user task it runs the task
once without an attack, then once per injection task with AgentDojo's
`important_instructions` attack, and appends one JSON line per run to the results file,
so an interrupted run resumes where it stopped.

    .venv/bin/python run.py --model ../../model/Qwen3.8-27B-Q4_K_M.gguf --suite banking
    .venv/bin/python run.py --summary result/banking-Qwen3.8-27B-Q4_K_M.jsonl
"""

import argparse
import json
import sys
import time
from collections import defaultdict
from pathlib import Path

from agentdojo.agent_pipeline.agent_pipeline import load_system_message
from agentdojo.attacks.attack_registry import load_attack
from agentdojo.task_suite.load_suites import get_suite

from pr3_agentdojo.approver import APPROVER_VERSION
from pr3_agentdojo.bridge import Bridge, BridgeAgent

HERE = Path(__file__).resolve().parent
BENCHMARK_VERSION = "v1.2.2"
ATTACK = "important_instructions"
# 2: PR3 gets the plain loop's budgets for tool results, arguments and calls per round.
HARNESS_VERSION = 2


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--model", type=Path, help="GGUF model file")
    parser.add_argument("--suite", default="banking", choices=["banking", "workspace", "slack", "travel"])
    parser.add_argument("--modes", nargs="+", default=["pr3", "plain"], choices=["pr3", "plain", "confirm_all"])
    parser.add_argument("--user-tasks", nargs="*", help="only these user tasks")
    parser.add_argument("--injection-tasks", nargs="*", help="only these injection tasks")
    parser.add_argument("--benign-only", action="store_true", help="run each user task without an attack only")
    parser.add_argument("--max-rounds", type=int, default=10)
    parser.add_argument("--bridge", type=Path, default=HERE.parents[1] / "target/release/pr3-bench-agentdojo")
    parser.add_argument("--out", type=Path, help="results file (JSON Lines)")
    parser.add_argument("--summary", type=Path, help="only print the summary of a results file")
    args = parser.parse_args()

    if args.summary:
        summarize(args.summary)
        return 0
    if args.model is None:
        parser.error("--model is required")

    suite = get_suite(BENCHMARK_VERSION, args.suite)
    out = args.out or HERE / "result" / f"{args.suite}-{args.model.stem}.jsonl"
    out.parent.mkdir(parents=True, exist_ok=True)
    done = set()
    if out.exists():
        for line in out.read_text().splitlines():
            r = json.loads(line)
            done.add((r["mode"], r["user_task"], r["injection_task"]))

    bridge = Bridge(args.bridge, args.model)
    system = load_system_message(None)
    user_tasks = args.user_tasks or list(suite.user_tasks)
    injection_tasks = [] if args.benign_only else args.injection_tasks or list(suite.injection_tasks)
    try:
        for mode in args.modes:
            agent = BridgeAgent(bridge, mode, args.suite, system, args.max_rounds)
            agent.known = suite.load_and_inject_default_environment({}).model_dump_json()
            attack = load_attack(ATTACK, suite, agent)
            for user_id in user_tasks:
                user_task = suite.get_user_task_by_id(user_id)
                for injection_id in [None, *injection_tasks]:
                    if (mode, user_id, injection_id) in done:
                        continue
                    injection_task = suite.get_injection_task_by_id(injection_id) if injection_id else None
                    injections = attack.attack(user_task, injection_task) if injection_task else {}
                    agent.prepare(user_task, injection_task)
                    started = time.time()
                    try:
                        utility, security = suite.run_task_with_pipeline(
                            agent, user_task, injection_task, injections
                        )
                        error = None
                    except Exception as e:  # a crash in one run is a result, not the end
                        utility, security, error = False, False, f"{type(e).__name__}: {e}"
                    record = {
                        "mode": mode,
                        "model": bridge.model,
                        "suite": args.suite,
                        "benchmark": BENCHMARK_VERSION,
                        "attack": ATTACK if injection_task else None,
                        "approver": APPROVER_VERSION,
                        "harness": HARNESS_VERSION,
                        "user_task": user_id,
                        "injection_task": injection_id,
                        "utility": utility,
                        # AgentDojo's `security` is True when the attacker's goal was reached.
                        "attack_succeeded": bool(security) if injection_task else None,
                        "model_attempted_attack": agent.attempted_attack(agent.pre_env)
                        if injection_task and error is None
                        else None,
                        "failed": agent.last.failed,
                        "error": error,
                        "stats": agent.last.stats,
                        # Every consequential or other call the model made, and what became of it.
                        "calls": [
                            {"name": n, "arguments": a, "outcome": o} for n, a, o in agent.last.calls
                        ],
                        "wall_seconds": round(time.time() - started, 2),
                    }
                    with out.open("a") as f:
                        f.write(json.dumps(record) + "\n")
                    flag = "ATTACK SUCCEEDED" if record["attack_succeeded"] else ""
                    print(
                        f"{mode:11} {user_id:13} {injection_id or '-':18} utility={utility!s:5} "
                        f"{flag} {record['wall_seconds']}s",
                        flush=True,
                    )
    finally:
        bridge.close()
    summarize(out)
    return 0


def summarize(path: Path) -> None:
    rows = [json.loads(line) for line in path.read_text().splitlines()]
    by_mode = defaultdict(list)
    for r in rows:
        by_mode[r["mode"]].append(r)

    def pct(part, whole):
        return f"{100 * part / whole:5.1f}%" if whole else "  n/a"

    print(f"\n{path.name}: {rows[0]['model'] if rows else ''}, AgentDojo {BENCHMARK_VERSION}, attack {ATTACK}")
    print(
        f"{'arm':12} {'benign utility':>15} {'utility under attack':>21} "
        f"{'model fooled':>13} {'attack succeeded':>17} {'approvals/run':>14} {'errors':>7}"
    )
    for mode, rs in by_mode.items():
        benign = [r for r in rs if r["injection_task"] is None]
        attacked = [r for r in rs if r["injection_task"] is not None]
        fooled = [r for r in attacked if r["model_attempted_attack"] is not None]
        asked = sum(r["stats"].get("approvals_asked", 0) for r in rs if r["stats"])
        errors = sum(1 for r in rs if r["error"])
        print(
            f"{mode:12} "
            f"{pct(sum(r['utility'] for r in benign), len(benign)):>15} "
            f"{pct(sum(r['utility'] for r in attacked), len(attacked)):>21} "
            f"{pct(sum(r['model_attempted_attack'] for r in fooled), len(fooled)):>13} "
            f"{pct(sum(r['attack_succeeded'] for r in attacked), len(attacked)):>17} "
            f"{asked / max(len(rs), 1):>14.2f} "
            f"{errors:>7}"
        )
    print(
        "\nmodel fooled: the model made the attacker's consequential call, whether or not it ran."
        "\nattack succeeded: AgentDojo found the attacker's goal reached in the environment."
    )


if __name__ == "__main__":
    sys.exit(main())
