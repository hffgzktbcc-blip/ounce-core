import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time
import unittest
from urllib.error import URLError
from urllib.request import urlopen
import uuid

from verify_corpus_run import pull_image


class PullRecoveryTests(unittest.TestCase):
    def exercise(self, diagnostics):
        with tempfile.TemporaryDirectory() as directory:
            evidence = Path(directory)
            calls, delays = [], []

            def stage(name, argv, seconds):
                calls.append((name, argv, seconds))
                diagnostic = diagnostics[len(calls) - 1]
                if diagnostic is None:
                    return 0
                if isinstance(diagnostic, Exception):
                    raise diagnostic
                (evidence / f"{name}.stderr.log").write_bytes(diagnostic)
                return 1

            error = None
            try:
                pull_image(stage, "image", 10, evidence, delay=delays.append)
            except ValueError as caught:
                error = caught
            return calls, delays, error

    def test_recovers_with_a_distinct_stage_and_the_same_command(self):
        calls, delays, error = self.exercise([b"503 Service Unavailable", None])
        self.assertIsNone(error)
        self.assertEqual(delays, [2])
        self.assertEqual(calls, [
            ("pull", ["docker", "pull", "image"], 10),
            ("pull-retry-1", ["docker", "pull", "image"], 10),
        ])

    def test_persistent_timeout_stops_after_three_attempts(self):
        timeout = subprocess.TimeoutExpired(["docker", "pull"], 10)
        calls, delays, error = self.exercise([timeout] * 3)
        self.assertEqual(len(calls), 3)
        self.assertEqual(delays, [2, 4])
        self.assertIsNotNone(error)

    def test_permanent_error_stops_immediately(self):
        calls, delays, error = self.exercise([b"pull access denied"])
        self.assertEqual(len(calls), 1)
        self.assertEqual(delays, [])
        self.assertIsNotNone(error)

    @unittest.skipUnless(os.getenv("OUNCE_VERIFY_DOCKER_RECOVERY_TEST") == "1", "requires Docker")
    def test_real_registry_connection_recovers(self):
        with tempfile.TemporaryDirectory() as directory:
            evidence = Path(directory)
            # Hold a bound port without listening so the first pull sees a refused connection.
            reserved = socket.socket()
            self.addCleanup(reserved.close)
            reserved.bind(("127.0.0.1", 0))
            port = reserved.getsockname()[1]
            name = f"ounce-recovery-test-{uuid.uuid4().hex}"
            image = f"127.0.0.1:{port}/ounce-recovery:proof"

            def docker(*args, **kwargs):
                return subprocess.run(["docker", *args], check=True, timeout=90, **kwargs)

            docker("pull", "registry:2")
            docker("pull", "hello-world:latest")
            calls = []

            def stage(stage_name, argv, seconds):
                with (evidence / f"{stage_name}.stderr.log").open("wb") as log:
                    completed = subprocess.run(argv, timeout=seconds, stderr=log)
                calls.append(completed.returncode)
                print(f"{stage_name}: docker pull exited {completed.returncode}", flush=True)
                if stage_name == "pull":
                    self.assertNotEqual(completed.returncode, 0)
                    self.assertIn(b"connection refused", (evidence / "pull.stderr.log").read_bytes())
                    reserved.close()
                    self.addCleanup(subprocess.run, ["docker", "rm", "-f", name], timeout=30)
                    docker("run", "-d", "--rm", "--name", name, "-p", f"127.0.0.1:{port}:5000", "registry:2")
                    self.addCleanup(subprocess.run, ["docker", "image", "rm", image], timeout=30)
                    deadline = time.monotonic() + 10
                    while True:
                        try:
                            with urlopen(f"http://127.0.0.1:{port}/v2/", timeout=1):
                                break
                        except URLError:
                            if time.monotonic() >= deadline:
                                raise
                            time.sleep(0.1)
                    docker("tag", "hello-world:latest", image)
                    docker("push", image)
                    docker("image", "rm", image)
                return completed.returncode

            pull_image(stage, image, 30, evidence)
            self.assertEqual(calls, [1, 0])
