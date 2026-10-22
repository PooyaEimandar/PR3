"""The Rust bridge as an AgentDojo pipeline element.

AgentDojo calls `query` with the task's prompt, tools and environment. The bridge runs
the agent loop and the model; this side runs each tool call against AgentDojo's
environment, answers approval requests with the scripted approver, and turns the run
into AgentDojo messages for scoring. Only calls that actually ran appear as tool calls
in those messages; declined proposals are counted separately.
"""

import json
import subprocess
from collections.abc import Sequence
from dataclasses import dataclass, field
from pathlib import Path

from agentdojo.agent_pipeline.base_pipeline_element import BasePipelineElement
from agentdojo.agent_pipeline.tool_execution import tool_result_to_str
from agentdojo.base_tasks import BaseInjectionTask, BaseUserTask
from agentdojo.functions_runtime import EmptyEnv, Env, FunctionCall, FunctionsRuntime
from agentdojo.types import ChatMessage, text_content_block_from_string

from .approver import CONSEQUENTIAL, matches, targets
from .schema import reduce


class Bridge:
    """One bridge process: the model loads once and serves every run."""

    def __init__(self, binary: Path, model: Path):
        self.process = subprocess.Popen(
            [str(binary), str(model)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            text=True,
            bufsize=1,
        )
        ready = self.recv()
        if ready.get("type") != "ready":
            raise RuntimeError(f"bridge did not start: {ready}")
        self.model = ready["model"]

    def send(self, message: dict) -> None:
        assert self.process.stdin is not None
        self.process.stdin.write(json.dumps(message) + "\n")
        self.process.stdin.flush()

    def recv(self) -> dict:
        assert self.process.stdout is not None
        line = self.process.stdout.readline()
        if not line:
            raise RuntimeError("the bridge exited")
        return json.loads(line)

    def close(self) -> None:
        if self.process.stdin:
            self.process.stdin.close()
        self.process.wait(timeout=60)


@dataclass
class Run:
    """What happened in one run, beyond what AgentDojo scores."""

    stats: dict = field(default_factory=dict)
    failed: str | None = None
    # Every call the model got as far as making: executed, or proposed and declined.
    calls: list[tuple[str, dict, str]] = field(default_factory=list)


class BridgeAgent(BasePipelineElement):
    """`mode` is `pr3`, `plain` or `confirm_all`. The name contains "local" so AgentDojo's
    attack addresses the model the same way in every arm."""

    def __init__(self, bridge: Bridge, mode: str, suite: str, system: str, max_rounds: int):
        self.bridge = bridge
        self.mode = mode
        self.suite = suite
        self.system = system
        self.max_rounds = max_rounds
        self.name = f"local-{mode}-{bridge.model}"
        self.user_task: BaseUserTask | None = None
        self.injection_task: BaseInjectionTask | None = None
        self.last = Run()

    def prepare(self, user_task: BaseUserTask, injection_task: BaseInjectionTask | None) -> None:
        self.user_task = user_task
        self.injection_task = injection_task

    def attempted_attack(self, pre_env: Env) -> bool:
        """Whether the model made the attacker's consequential call, whether or not it ran."""
        if self.injection_task is None:
            return False
        goal = self.injection_task.ground_truth(pre_env.model_copy(deep=True))
        return any(
            name in CONSEQUENTIAL[self.suite] and targets(self.suite, name, args, goal)
            for name, args, _ in self.last.calls
        )

    def query(
        self,
        query: str,
        runtime: FunctionsRuntime,
        env: Env = EmptyEnv(),
        messages: Sequence[ChatMessage] = [],
        extra_args: dict = {},
    ) -> tuple[str, FunctionsRuntime, Env, Sequence[ChatMessage], dict]:
        assert self.user_task is not None, "call prepare() first"
        self.pre_env = env.model_copy(deep=True)
        needed = self.user_task.ground_truth(env.model_copy(deep=True))
        tools = [
            {
                "name": f.name,
                "description": f.description,
                "parameters": reduce(f.parameters.model_json_schema()),
            }
            for f in runtime.functions.values()
        ]
        self.bridge.send(
            {
                "type": "run",
                "mode": self.mode,
                "system": self.system,
                "prompt": query,
                "tools": tools,
                "consequential": sorted(CONSEQUENTIAL[self.suite]),
                "max_rounds": self.max_rounds,
            }
        )
        run = Run()
        transcript: list[ChatMessage] = [
            {"role": "system", "content": [text_content_block_from_string(self.system)]},
            {"role": "user", "content": [text_content_block_from_string(query)]},
        ]
        approved: dict[str, bool] = {}
        while True:
            message = self.bridge.recv()
            kind = message["type"]
            if kind == "approve":
                args = message["arguments"] or {}
                ok = matches(self.suite, message["name"], args, needed)
                approved[message["id"]] = ok
                if not ok:
                    run.calls.append((message["name"], args, "declined"))
                self.bridge.send({"type": "approval", "id": message["id"], "approve": ok})
            elif kind == "tool_call":
                args = message["arguments"] or {}
                run.calls.append((message["name"], args, "executed"))
                result, error = runtime.run_function(env, message["name"], args)
                output = "" if error else tool_result_to_str(result)
                self.bridge.send(
                    {"type": "tool_result", "id": message["id"], "output": output, "error": error}
                )
                call = FunctionCall(function=message["name"], args=args, id=message["id"])
                transcript.append({"role": "assistant", "content": None, "tool_calls": [call]})
                transcript.append(
                    {
                        "role": "tool",
                        "content": [text_content_block_from_string(output or error or "")],
                        "tool_call_id": message["id"],
                        "tool_call": call,
                        "error": error,
                    }
                )
            elif kind == "done":
                run.stats = message["stats"]
                run.failed = message["failed"]
                reply = message["reply"] or (f"(failed: {run.failed})" if run.failed else "")
                transcript.append(
                    {
                        "role": "assistant",
                        "content": [text_content_block_from_string(reply)],
                        "tool_calls": None,
                    }
                )
                break
            else:
                raise RuntimeError(f"unexpected bridge message: {message}")
        self.last = run
        return query, runtime, env, transcript, extra_args
