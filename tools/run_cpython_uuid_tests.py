from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass, replace
from pathlib import Path


@dataclass(frozen=True)
class CpythonTestConfig:
    python: Path
    cpython_reference: str
    tests_directory: Path
    patched: bool
    verbose: bool
    test_names: tuple[str, ...] = ()


@dataclass(frozen=True)
class CpythonTestResult:
    command: tuple[str, ...]
    cwd: Path | None
    environment: dict[str, str]
    returncode: int
    stdout: str
    stderr: str


def default_cpython_reference() -> str:
    return f"{sys.version_info.major}.{sys.version_info.minor}"


def default_tests_directory() -> Path:
    return Path(".cpython-tests") / default_cpython_reference()


def run_command(command: list[str], *, cwd: Path | None = None, env: dict[str, str] | None = None) -> None:
    subprocess.run(command, cwd=cwd, env=env, check=True)


def run_command_capture(
    command: list[str],
    *,
    cwd: Path | None = None,
    env: dict[str, str] | None = None,
) -> CpythonTestResult:
    completed = subprocess.run(
        command,
        cwd=cwd,
        env=env,
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    return CpythonTestResult(
        command=tuple(command),
        cwd=cwd,
        environment=env if env is not None else os.environ.copy(),
        returncode=completed.returncode,
        stdout=completed.stdout,
        stderr=completed.stderr,
    )


def ensure_cpython_tests(config: CpythonTestConfig) -> Path:
    test_uuid_path = config.tests_directory / "Lib" / "test" / "test_uuid.py"
    if test_uuid_path.exists():
        return config.tests_directory / "Lib"

    if shutil.which("git") is None:
        raise RuntimeError("git is required to fetch CPython's Lib/test/test_uuid.py")

    config.tests_directory.parent.mkdir(parents=True, exist_ok=True)
    run_command(
        [
            "git",
            "clone",
            "--filter=blob:none",
            "--sparse",
            "--depth",
            "1",
            "--branch",
            config.cpython_reference,
            "https://github.com/python/cpython.git",
            str(config.tests_directory),
        ]
    )
    run_command(["git", "sparse-checkout", "set", "Lib/test"], cwd=config.tests_directory)
    return config.tests_directory / "Lib"


def build_environment(config: CpythonTestConfig, cpython_lib: Path, sitecustomize_directory: Path) -> dict[str, str]:
    python_path_entries = [str(cpython_lib), str(Path.cwd())]
    if config.patched:
        python_path_entries.insert(0, str(sitecustomize_directory))
        (sitecustomize_directory / "sitecustomize.py").write_text(
            "import uuideal\nuuideal.install()\n",
            encoding="utf-8",
        )

    environment = os.environ.copy()
    existing_python_path = environment.get("PYTHONPATH")
    if existing_python_path:
        python_path_entries.append(existing_python_path)
    environment["PYTHONPATH"] = os.pathsep.join(python_path_entries)
    return environment


def normalize_cpython_uuid_test_name(test_name: str) -> str:
    if test_name.startswith("test.test_uuid."):
        return test_name
    return f"test.test_uuid.{test_name}"


def cpython_uuid_test_command(config: CpythonTestConfig) -> list[str]:
    command = [str(config.python), "-m", "unittest"]
    if config.verbose:
        command.append("-v")
    if config.test_names:
        command.extend(normalize_cpython_uuid_test_name(test_name) for test_name in config.test_names)
    else:
        command.append("test.test_uuid")
    return command


def run_cpython_uuid_tests_result(config: CpythonTestConfig) -> CpythonTestResult:
    cpython_lib = ensure_cpython_tests(config)
    with tempfile.TemporaryDirectory(prefix="uuideal-cpython-tests-") as temporary_directory:
        environment = build_environment(config, cpython_lib, Path(temporary_directory))
        return run_command_capture(cpython_uuid_test_command(config), env=environment)


def run_cpython_uuid_tests(config: CpythonTestConfig) -> None:
    result = run_cpython_uuid_tests_result(config)
    if result.returncode != 0:
        raise subprocess.CalledProcessError(
            result.returncode,
            result.command,
            output=result.stdout,
            stderr=result.stderr,
        )


def discover_cpython_uuid_tests(config: CpythonTestConfig) -> tuple[str, ...]:
    cpython_lib = ensure_cpython_tests(config)
    discovery_config = replace(config, patched=False, verbose=False, test_names=())
    script = """
import test.test_uuid
import unittest

loader = unittest.defaultTestLoader
suite = loader.loadTestsFromModule(test.test_uuid)

def walk(test):
    if isinstance(test, unittest.TestSuite):
        for item in test:
            yield from walk(item)
    else:
        yield test.id()

for test_id in sorted(walk(suite)):
    print(test_id)
"""
    with tempfile.TemporaryDirectory(prefix="uuideal-cpython-discovery-") as temporary_directory:
        environment = build_environment(discovery_config, cpython_lib, Path(temporary_directory))
        result = run_command_capture([str(discovery_config.python), "-c", script], env=environment)

    if result.returncode != 0:
        raise subprocess.CalledProcessError(
            result.returncode,
            result.command,
            output=result.stdout,
            stderr=result.stderr,
        )

    return tuple(line for line in result.stdout.splitlines() if line.startswith("test.test_uuid."))


def parse_args() -> CpythonTestConfig:
    parser = argparse.ArgumentParser(description="Run CPython's Lib/test/test_uuid.py against uuideal.")
    parser.add_argument("--python", type=Path, default=Path(sys.executable))
    parser.add_argument("--ref", default=default_cpython_reference())
    parser.add_argument(
        "--tests-dir",
        type=Path,
        default=default_tests_directory(),
    )
    parser.add_argument(
        "--unpatched",
        action="store_true",
        help="Run CPython test_uuid without installing uuideal's stdlib patch.",
    )
    parser.add_argument("-q", "--quiet", action="store_true")
    args = parser.parse_args()
    return CpythonTestConfig(
        python=args.python,
        cpython_reference=args.ref,
        tests_directory=args.tests_dir,
        patched=not args.unpatched,
        verbose=not args.quiet,
    )


def main() -> None:
    run_cpython_uuid_tests(parse_args())


if __name__ == "__main__":
    main()
