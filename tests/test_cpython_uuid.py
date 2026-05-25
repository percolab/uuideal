from __future__ import annotations

import importlib
import sys
import unittest
from functools import lru_cache
from pathlib import Path
from typing import Iterator

import pytest

import uuideal
from tools.run_cpython_uuid_tests import (
    CpythonTestConfig,
    default_cpython_reference,
    default_tests_directory,
    ensure_cpython_tests,
)


def cpython_uuid_config(pytest_config: pytest.Config) -> CpythonTestConfig:
    reference = pytest_config.getoption("--cpython-uuid-ref") or default_cpython_reference()
    tests_directory_option = pytest_config.getoption("--cpython-uuid-tests-dir")
    tests_directory = Path(tests_directory_option) if tests_directory_option else default_tests_directory()
    return CpythonTestConfig(
        python=Path(sys.executable),
        cpython_reference=reference,
        tests_directory=tests_directory,
        patched=not pytest_config.getoption("--cpython-uuid-unpatched"),
        verbose=False,
    )


def iter_unittest_cases(suite: unittest.TestSuite) -> Iterator[unittest.TestCase]:
    for item in suite:
        if isinstance(item, unittest.TestSuite):
            yield from iter_unittest_cases(item)
        else:
            yield item


@lru_cache(maxsize=8)
def load_cpython_uuid_test_ids(
    cpython_reference: str,
    tests_directory: str,
) -> tuple[str, ...]:
    config = CpythonTestConfig(
        python=Path(sys.executable),
        cpython_reference=cpython_reference,
        tests_directory=Path(tests_directory),
        patched=False,
        verbose=False,
    )
    cpython_lib = ensure_cpython_tests(config)
    cpython_lib_string = str(cpython_lib)
    if cpython_lib_string not in sys.path:
        sys.path.insert(0, cpython_lib_string)

    module = importlib.import_module("test.test_uuid")
    suite = unittest.defaultTestLoader.loadTestsFromModule(module)
    return tuple(sorted(test.id() for test in iter_unittest_cases(suite)))


def pytest_generate_tests(metafunc: pytest.Metafunc) -> None:
    if "cpython_uuid_test_name" not in metafunc.fixturenames:
        return

    config = cpython_uuid_config(metafunc.config)
    test_names = load_cpython_uuid_test_ids(
        config.cpython_reference,
        str(config.tests_directory),
    )
    ids = [test_name.removeprefix("test.test_uuid.") for test_name in test_names]
    metafunc.parametrize("cpython_uuid_test_name", test_names, ids=ids)


def load_unittest_case(test_name: str) -> unittest.TestCase:
    module_name, class_name, method_name = test_name.rsplit(".", 2)
    module = importlib.import_module(module_name)
    case_type = getattr(module, class_name)
    return case_type(method_name)


def format_unittest_failures(result: unittest.TestResult) -> str:
    parts: list[str] = []
    for test, traceback_text in result.failures:
        parts.append(f"FAIL: {test.id()}\n{traceback_text}")
    for test, traceback_text in result.errors:
        parts.append(f"ERROR: {test.id()}\n{traceback_text}")
    return "\n".join(parts)


@pytest.mark.cpython_uuid
def test_cpython_uuid(cpython_uuid_test_name: str, pytestconfig: pytest.Config) -> None:
    config = cpython_uuid_config(pytestconfig)

    if config.patched:
        uuideal.install()
    else:
        uuideal.uninstall()

    case = load_unittest_case(cpython_uuid_test_name)
    result = unittest.TestResult()
    case.run(result)

    if result.skipped:
        reason = result.skipped[0][1]
        pytest.skip(reason)

    if result.expectedFailures:
        pytest.xfail(result.expectedFailures[0][1])

    if result.unexpectedSuccesses:
        pytest.fail(f"unexpected success: {result.unexpectedSuccesses[0].id()}")

    if not result.wasSuccessful():
        pytest.fail(format_unittest_failures(result), pytrace=False)