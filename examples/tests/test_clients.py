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

    def test_openshell_http_requires_explicit_local_route(self):
        with patch.dict(os.environ, OPENAI_BASE_URL='http://host.openshell.internal:8787/v1'):
            with self.assertRaises(ValueError):
                config()
            self.assertEqual(config(openshell_local=True)[0], 'http://host.openshell.internal:8787/v1')
            with self.assertRaises(ValueError):
                config(remote=True, openshell_local=True)
        for base in ('http://public.example/v1', 'http://host.openshell.internal.evil/v1',
                     'http://user:password@host.openshell.internal/v1'):
            with patch.dict(os.environ, OPENAI_BASE_URL=base):
                with self.assertRaises(ValueError):
                    config(openshell_local=True)

    def test_openshell_local_mode_rejects_other_hosts_before_cli(self):
        module = load('openshell')
        for base in ('https://gateway.example/v1', 'http://127.0.0.1:8787/v1'):
            with patch.dict(os.environ, OPENAI_BASE_URL=base), patch.object(module, 'cli') as cli:
                with self.assertRaises(ValueError):
                    module.main(local=True)
                cli.assert_not_called()

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
        owned_id = "a" * 64
        def docker(args, **kwargs):
            if args[1] == "run":
                Path(args[args.index("--cidfile") + 1]).write_text(owned_id)
            return Mock(returncode=0)
        with patch.object(module.subprocess, "run", side_effect=docker) as execute:
            module.main()
        invocation = execute.call_args_list[0]
        self.assertNotIn("test-gateway-token", str(invocation.args))
        self.assertIn("OPENAI_API_KEY", invocation.args[0])
        self.assertEqual(invocation.kwargs["env"]["OPENAI_API_KEY"], "test-gateway-token")
        self.assertEqual(execute.call_args_list[1].args[0], ["docker", "rm", "-f", owned_id])

    def test_docker_name_collision_does_not_remove_existing_container(self):
        module = load("docker")
        with patch.object(module.uuid, "uuid4", return_value=Mock(hex="1" * 32)), patch.object(module.subprocess, "run", return_value=Mock(returncode=125)) as execute:
            with self.assertRaises(RuntimeError):
                module.main()
        self.assertEqual(execute.call_count, 1)
        args = execute.call_args.args[0]
        self.assertEqual(args[1], "run")
        self.assertEqual(args[args.index("--name") + 1], "h2a-example-" + "1" * 12)

    def test_docker_failure_cleans_up_only_recorded_id(self):
        module = load("docker")
        owned_id = "b" * 64
        def docker(args, **kwargs):
            if args[1] == "run":
                Path(args[args.index("--cidfile") + 1]).write_text(owned_id)
                return Mock(returncode=125)
            return Mock(returncode=0)
        with patch.object(module.subprocess, "run", side_effect=docker) as execute:
            with self.assertRaises(RuntimeError):
                module.main()
        self.assertEqual(execute.call_args_list[1].args[0], ["docker", "rm", "-f", owned_id])

    def test_docker_timeout_cleans_up_recorded_id(self):
        module = load("docker")
        owned_id = "c" * 64
        def docker(args, **kwargs):
            if args[1] == "run":
                Path(args[args.index("--cidfile") + 1]).write_text(owned_id)
                raise module.subprocess.TimeoutExpired(args, 240)
            return Mock(returncode=0)
        with patch.object(module.subprocess, "run", side_effect=docker) as execute:
            with self.assertRaises(module.subprocess.TimeoutExpired):
                module.main()
        self.assertEqual(execute.call_args_list[1].args[0], ["docker", "rm", "-f", owned_id])

    def test_docker_ignores_invalid_id(self):
        module = load("docker")
        def docker(args, **kwargs):
            Path(args[args.index("--cidfile") + 1]).write_text("not-a-container-id")
            return Mock(returncode=125)
        with patch.object(module.subprocess, "run", side_effect=docker) as execute:
            with self.assertRaises(RuntimeError):
                module.main()
        self.assertEqual(execute.call_count, 1)


if __name__ == "__main__":
    unittest.main()
