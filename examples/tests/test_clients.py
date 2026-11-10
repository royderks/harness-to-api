"""Local boundary tests, not proof of cloud-provider execution."""
import importlib.util
import os
from pathlib import Path
import sys
import unittest
from unittest.mock import Mock, patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from _client import config


def load(name):
    spec = importlib.util.spec_from_file_location("example_" + name, ROOT / name / "main.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class Clients(unittest.TestCase):
    def setUp(self):
        self.env = patch.dict(os.environ, {"OPENAI_BASE_URL": "https://gateway.example/v1", "OPENAI_API_KEY": "test-gateway-token", "H2A_MODEL": "codex/default", "E2B_API_KEY": "test-control-token"}, clear=True)
        self.env.start()
        self.addCleanup(self.env.stop)

    def test_remote_configuration_rejects_unsafe_urls(self):
        for base in ("http://gateway.example/v1", "http://127.0.0.1:8787/v1", "https://user:secret@gateway.example/v1", "https://gateway.example/v1?token=x", "https://gateway.example/v1#x", "https://gateway.example"):
            with self.subTest(base=base), patch.dict(os.environ, OPENAI_BASE_URL=base):
                with self.assertRaises(ValueError):
                    config(remote=True)

    def test_e2b_limits_injection_and_cleans_up_after_failure(self):
        module = load("e2b")
        sandbox = Mock()
        sandbox.commands.run.side_effect = RuntimeError("test failure")
        with patch.object(module.Sandbox, "create", return_value=sandbox) as create:
            with self.assertRaises(RuntimeError):
                module.main()
        args = create.call_args.kwargs
        self.assertEqual(args["timeout"], 300)
        self.assertEqual(set(args["envs"]), {"OPENAI_BASE_URL", "OPENAI_API_KEY", "H2A_MODEL"})
        self.assertNotIn("test-gateway-token", sandbox.files.write.call_args.args[1])
        sandbox.kill.assert_called_once()

    def test_modal_cleans_up_after_failure(self):
        module = load("modal")
        sandbox = Mock()
        sandbox.wait.side_effect = RuntimeError("test failure")
        from unittest.mock import MagicMock
        with patch.object(module.modal, "App", return_value=MagicMock()), patch.object(module.modal.Image, "debian_slim"), patch.object(module.modal.Secret, "from_dict") as secret, patch.object(module.modal.Sandbox, "create", return_value=sandbox) as create:
            with self.assertRaises(RuntimeError):
                module.main()
        self.assertEqual(set(secret.call_args.args[0]), {"OPENAI_BASE_URL", "OPENAI_API_KEY", "H2A_MODEL"})
        self.assertEqual(create.call_args.kwargs["timeout"], 300)
        self.assertNotIn("test-gateway-token", str(create.call_args.args))
        sandbox.terminate.assert_called_once()

    def test_docker_passes_secret_by_environment_not_argv(self):
        module = load("docker")
        with patch.object(module.subprocess, "run", return_value=Mock(returncode=0)) as execute:
            module.main()
        invocation = execute.call_args_list[0]
        self.assertNotIn("test-gateway-token", str(invocation.args))
        self.assertIn("OPENAI_API_KEY", invocation.args[0])
        self.assertEqual(invocation.kwargs["env"]["OPENAI_API_KEY"], "test-gateway-token")
        self.assertEqual(execute.call_args_list[1].args[0][:3], ["docker", "rm", "-f"])


if __name__ == "__main__":
    unittest.main()
