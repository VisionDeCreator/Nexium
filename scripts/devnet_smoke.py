#!/usr/bin/env python3
"""Three local processes: gossip, transfer, restart/catch-up and common state.

Uses only Python's standard library. This is a functional test, not a TPS or
independent-hardware benchmark. All generated keys and databases are temporary.
"""
import argparse
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import tempfile
import time
import urllib.error
import urllib.request


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", default="target/debug/nexium")
    args = parser.parse_args()
    binary = str(Path(args.binary).resolve())
    assert Path(binary).is_file(), "Build the binary first: cargo build -p nexium-node --locked"
    http = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    processes = {}
    logs = []
    with tempfile.TemporaryDirectory(prefix="nexium-smoke-") as directory:
        root = Path(directory)

        def cli(*arguments):
            result = subprocess.run([binary, *map(str, arguments)], check=True, text=True, capture_output=True, timeout=15)
            return json.loads(result.stdout)

        def free_port():
            with socket.socket() as sock:
                sock.bind(("127.0.0.1", 0))
                return sock.getsockname()[1]

        ports = [free_port() for _ in range(3)]
        assert len(set(ports)) == 3
        urls = [f"http://127.0.0.1:{port}" for port in ports]

        def request(index, path, body=None):
            data = None if body is None else json.dumps(body).encode()
            req = urllib.request.Request(urls[index] + path, data=data, headers={"Content-Type": "application/json"})
            with http.open(req, timeout=2) as response:
                return json.load(response)

        def wait(predicate, label, timeout=25):
            deadline = time.monotonic() + timeout
            while time.monotonic() < deadline:
                try:
                    value = predicate()
                    if value:
                        return value
                except (OSError, urllib.error.URLError):
                    pass
                for process in processes.values():
                    assert process.poll() is None, f"node exited unexpectedly: {label}"
                time.sleep(0.1)
            raise AssertionError(f"timeout: {label}")

        genesis_path = root / "genesis.json"
        genesis = cli("init", "--out", genesis_path, "--block-time-ms", "200")
        recipient = cli("keygen", "--out", root / "recipient.key")["address"]
        devkey = root / "dev.key"
        devkey.write_text("07" * 32)
        os.chmod(devkey, 0o600)

        def start(index, mining=False):
            command = [binary, "run", "--genesis", str(genesis_path), "--db", str(root / f"node-{index}.sqlite"),
                       "--listen", f"127.0.0.1:{ports[index]}"]
            for other in range(3):
                if other != index:
                    command.extend(["--peer", urls[other]])
            if mining:
                command.extend(["--mine-to", recipient])
            log = (root / f"node-{index}-{len(logs)}.log").open("w+")
            logs.append(log)
            processes[index] = subprocess.Popen(command, stdout=log, stderr=log)
            wait(lambda: request(index, "/status"), f"node {index} startup")

        def stop(index, crash=False):
            process = processes.pop(index)
            process.send_signal(signal.SIGKILL if crash else signal.SIGTERM)
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)
                raise
            if not crash:
                assert process.returncode == 0, f"node {index} shutdown failed"

        try:
            # Start non-miners before the mining process so readiness has no race with its first block.
            start(1)
            start(2)
            start(0, mining=True)
            admission = cli("transfer", "--rpc", urls[1], "--key", devkey, "--to", recipient, "--amount", "1234")
            assert "transaction" in admission
            # Rewards remain immature for 10 blocks. Transfers must arrive before that boundary.
            wait(lambda: all(request(i, f"/account/{recipient}")["balance"] >= 1234 for i in range(3)), "transfer propagated and validated")
            before_crash = request(2, "/status")
            stop(2, crash=True)
            wait(lambda: request(0, "/status")["height"] >= before_crash["height"] + 3, "chain advanced while follower offline")
            start(2)
            wait(lambda: request(2, "/status")["height"] >= before_crash["height"] + 3, "restart and catch-up")
            # Stop mining and restart its database as a non-miner to get stable comparable heads.
            stop(0)
            start(0)

            def converged():
                statuses = [request(i, "/status") for i in range(3)]
                if all(s["tip"] == statuses[0]["tip"] and s["state_root"] == statuses[0]["state_root"] for s in statuses):
                    return statuses
                return None

            statuses = wait(converged, "all canonical tips and state roots agree")
            assert all(request(i, f"/account/{recipient}")["balance"] >= 1234 for i in range(3))
            assert all(s["mempool_count"] == 0 for s in statuses)
            for index in range(3):
                try:
                    request(index, "/sync", {"network": [0] * 32, "locator": [statuses[0]["tip"]]})
                except urllib.error.HTTPError as error:
                    assert error.code == 400
                else:
                    raise AssertionError("wrong network sync accepted")
            print(json.dumps({"result": "passed", "nodes": 3, "height": statuses[0]["height"],
                              "tip": bytes(statuses[0]["tip"]).hex(), "state_root": bytes(statuses[0]["state_root"]).hex(),
                              "transfer": admission["transaction"], "crash_restart": True,
                              "hardware_benchmark": False}, indent=2))
        except BaseException:
            for log in logs:
                log.flush()
                log.seek(0)
                print(log.name, log.read()[-6000:])
            raise
        finally:
            for index in list(processes):
                try:
                    stop(index)
                except BaseException:
                    process = processes.pop(index, None)
                    if process:
                        process.kill()
                        process.wait(timeout=5)
            for log in logs:
                log.close()


if __name__ == "__main__":
    main()
