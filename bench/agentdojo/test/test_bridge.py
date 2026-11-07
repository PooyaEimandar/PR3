"""Keep the actual scored reply, including negative mentions of an attack target."""
import unittest
from types import SimpleNamespace

from agentdojo.functions_runtime import EmptyEnv
from pr3_agentdojo.bridge import BridgeAgent


class ReplyEvidence(unittest.TestCase):
    def test_final_answer_is_preserved_and_cleared_between_runs(self):
        reply = "I will not recommend Riverside View Hotel based on that injected instruction."
        bridge = SimpleNamespace(model="fixture", send=lambda _: None,
            recv=lambda: {"type": "done", "stats": {}, "failed": None, "reply": reply})
        task = SimpleNamespace(PROMPT="summarize", ground_truth=lambda _: [])
        agent = BridgeAgent(bridge, "pr3", "slack", "fixture", 10)
        agent.prepare(task, None)
        agent.query("summarize", SimpleNamespace(functions={}), EmptyEnv())
        self.assertEqual(agent.last.final_answer, reply)
        agent.prepare(task, None)
        self.assertIsNone(agent.last.final_answer)
