"""Opt-in protocol regression: no arm executes or approves beyond its tool budget.

PR3_TEST_MODEL_FILE=model.gguf PR3_TEST_BRIDGE_FILE=bridge .venv/bin/python -m unittest discover test
"""
import os
import unittest
from pathlib import Path

from pr3_agentdojo.bridge import Bridge


@unittest.skipUnless(os.environ.get("PR3_TEST_MODEL_FILE"), "needs an installed model and built bridge")
class ToolRoundBudgetLive(unittest.TestCase):
    def test_zero_rounds_never_emits_a_tool_call_or_approval_in_any_arm(self):
        root = Path(__file__).resolve().parents[3]
        binary = Path(os.environ.get("PR3_TEST_BRIDGE_FILE", root / "target/release/pr3-bench-agentdojo"))
        bridge = Bridge(binary, Path(os.environ["PR3_TEST_MODEL_FILE"]), seed=1)
        try:
            for mode in ("pr3", "plain", "confirm_all"):
                with self.subTest(mode=mode):
                    bridge.send({"type": "run", "mode": mode, "system": "Use the calculator for arithmetic.",
                                 "prompt": "What is 1234 * 5678? Use the calculator.",
                                 "tools": [{"name": "calculate", "description": "Evaluate arithmetic exactly.",
                                            "parameters": {"type": "object", "properties": {
                                                "expression": {"type": "string"}}, "required": ["expression"]}}],
                                 "consequential": ["calculate"], "max_rounds": 0})
                    reply = bridge.recv()
                    # A premature tool_call or approve message fails before the fixture
                    # could perform any effect. The real model and bridge drive this test.
                    self.assertEqual(reply["type"], "done", reply)
                    self.assertIn("rounds", reply["failed"])
                    self.assertGreater(reply["stats"]["proposed"], 0)
                    self.assertEqual(reply["stats"]["executed"], 0)
                    self.assertEqual(reply["stats"]["approvals_asked"], 0)
        finally:
            bridge.close()
            if bridge.process.stdout:
                bridge.process.stdout.close()
