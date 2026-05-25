from __future__ import annotations

import argparse
import ctypes
import math
import os
import pickle
import platform
import queue
import signal
import subprocess
import sys
import timeit
from collections.abc import Iterable, Iterator, Sequence
from dataclasses import dataclass
from datetime import UTC, datetime
from multiprocessing import get_context
from multiprocessing.context import BaseContext
from multiprocessing.queues import Queue
from pathlib import Path
from types import ModuleType
from typing import Callable, TypeVar

import uuid

import uuideal

try:
    import fastuuid
except ModuleNotFoundError:  # pragma: no cover - exercised by users without benchmark extras.
    fastuuid = None

try:
    import uuid_utils
except ModuleNotFoundError:  # pragma: no cover - exercised by users without benchmark extras.
    uuid_utils = None

try:
    import uuid_utils.compat as uuid_utils_compat
except ModuleNotFoundError:  # pragma: no cover - exercised by users without benchmark extras.
    uuid_utils_compat = None

HEX_VALUE = "12345678123456789234567812345678"
DNS_NAMESPACE_HEX = "6ba7b8109dad11d180b400c04fd430c8"
GENERATION_FUNCTIONS = ("uuid1", "uuid3", "uuid4", "uuid5", "uuid6", "uuid7", "uuid8")
CONVERSION_OPERATIONS = (
    "`UUID('<hex>')`",
    "`str(value)`",
    "`pickle.dumps(value)`",
    "`pickle.loads(payload)`",
)
ACCESS_SORT_OPERATION = "`sorted(values)`"
PUBLIC_UUID_FIELDS = (
    "int",
    "bytes",
    "bytes_le",
    "hex",
    "fields",
    "time_low",
    "time_mid",
    "time_hi_version",
    "clock_seq_hi_variant",
    "clock_seq_low",
    "node",
    "time",
    "clock_seq",
    "urn",
    "variant",
    "version",
    "is_safe",
)
README_START_MARKER = "<!-- uuideal-benchmarks:start -->"
README_END_MARKER = "<!-- uuideal-benchmarks:end -->"
CANDIDATE_KEYS = ("stdlib", "fastuuid", "uuid_utils.compat", "uuid_utils", "uuideal")
BENCHMARK_CATEGORIES = ("generation", "conversions", "access")
DEFAULT_BENCHMARK_WORKERS = 1
LINUX_SCHED_FIFO_PRIORITY = 1
DARWIN_THREAD_QOS_USER_INTERACTIVE = 0x21
DARWIN_THREAD_QOS_POLICY = 9
BENCHMARK_TITLES = {
    "generation": "Generation",
    "conversions": "Conversions",
    "access": "Access",
}
TABLE_START_MARKERS = {
    category: f"<!-- uuideal-benchmarks:{category}:start -->" for category in BENCHMARK_CATEGORIES
}
TABLE_END_MARKERS = {
    category: f"<!-- uuideal-benchmarks:{category}:end -->" for category in BENCHMARK_CATEGORIES
}
GEOMEAN_FOOTNOTE_ID = "uuideal-benchmarks-geomean"
GEOMEAN_FOOTNOTE_MARKER = fr"<sup>[\*](#{GEOMEAN_FOOTNOTE_ID})</sup>"
BenchmarkProgressItem = TypeVar("BenchmarkProgressItem")
try:
    from tqdm import tqdm
except ModuleNotFoundError:  # pragma: no cover - exercised without benchmark extras.
    tqdm = None


@dataclass(frozen=True)
class BenchmarkConfig:
    repeats: int
    minimum_time: float
    update_readme: Path | None
    save_run: Path | None
    load_run: Path | None
    highlight_fastest: bool
    selected_candidate_keys: tuple[str, ...] | None
    excluded_candidate_keys: tuple[str, ...]
    selected_categories: tuple[str, ...]
    workers: int
    dedicated_cores: bool
    scheduler_hints: bool


@dataclass(frozen=True)
class GenerationCase:
    function_name: str
    operation: str
    expected_is_safe: uuid.SafeUUID | None = None
    force_generate_time_safe_none: bool = False


@dataclass(frozen=True)
class Candidate:
    key: str
    display_name: str
    class_name: str
    stdlib_compatible: bool
    setup: Callable[[], None]
    module: ModuleType | None
    uuid_constructor: Callable[..., object] | None
    categories: tuple[str, ...]
    namespace_dns: object | None
    hash_name: str | bytes
    required_distribution: str | None = None


@dataclass(frozen=True)
class BenchmarkCase:
    category: str
    operation: str
    candidate_key: str
    generation_function_name: str | None = None
    expected_is_safe: uuid.SafeUUID | None = None
    force_generate_time_safe_none: bool = False
    access_field: str | None = None


@dataclass(frozen=True)
class WorkerResult:
    case_index: int
    result: BenchmarkResult


@dataclass(frozen=True)
class SchedulingReport:
    worker_index: int
    policies: tuple[str, ...]


@dataclass(frozen=True)
class BenchmarkResult:
    category: str
    operation: str
    candidate_key: str
    candidate_name: str
    candidate_class_name: str
    stdlib_compatible: bool
    return_type: str
    nanoseconds_per_operation: float | None
    note: str = ""


@dataclass(frozen=True)
class CandidateAggregate:
    geomean_speedup: float | None


@dataclass(frozen=True)
class BenchmarkSuite:
    generated_at: datetime
    python: str
    cpu: str
    operating_system: str
    repeats: int
    minimum_time: float
    workers: int
    dedicated_cores: bool
    scheduler_hints: bool
    highlight_fastest: bool
    selected_categories: tuple[str, ...]
    generation: list[BenchmarkResult]
    conversions: list[BenchmarkResult]
    access: list[BenchmarkResult]
    applied_scheduling: tuple[str, ...] = ()


def uninstall_uuideal() -> None:
    uuideal.uninstall()


def install_uuideal() -> None:
    uuideal.install()


def stdlib_candidate() -> Candidate:
    return Candidate(
        key="stdlib",
        display_name="stdlib",
        class_name="`uuid.UUID`",
        stdlib_compatible=True,
        setup=uninstall_uuideal,
        module=uuid,
        uuid_constructor=uuid.UUID,
        categories=("generation", "conversions", "access"),
        namespace_dns=uuid.NAMESPACE_DNS,
        hash_name="python.org",
    )


def fastuuid_candidate() -> Candidate:
    if fastuuid is None:
        return Candidate(
            key="fastuuid",
            display_name="_`fastuuid`_",
            class_name="_`fastuuid.UUID`_",
            stdlib_compatible=False,
            setup=uninstall_uuideal,
            module=None,
            uuid_constructor=None,
            categories=("generation", "conversions", "access"),
            namespace_dns=None,
            hash_name=b"python.org",
            required_distribution="fastuuid",
        )

    return Candidate(
        key="fastuuid",
        display_name="_`fastuuid`_",
        class_name="_`fastuuid.UUID`_",
        stdlib_compatible=False,
        setup=uninstall_uuideal,
        module=fastuuid,
        uuid_constructor=fastuuid.UUID,
        categories=("generation", "conversions", "access"),
        namespace_dns=fastuuid.UUID(DNS_NAMESPACE_HEX),
        hash_name=b"python.org",
    )


def uuid_utils_compat_candidate() -> Candidate:
    if uuid_utils_compat is None:
        return Candidate(
            key="uuid_utils.compat",
            display_name="`uuid_utils.compat`",
            class_name="`uuid.UUID`",
            stdlib_compatible=True,
            setup=uninstall_uuideal,
            module=None,
            uuid_constructor=None,
            categories=("generation",),
            namespace_dns=None,
            hash_name="python.org",
            required_distribution="uuid-utils",
        )

    return Candidate(
        key="uuid_utils.compat",
        display_name="`uuid_utils.compat`",
        class_name="`uuid.UUID`",
        stdlib_compatible=True,
        setup=uninstall_uuideal,
        module=uuid_utils_compat,
        uuid_constructor=uuid_utils_compat.UUID,
        categories=("generation",),
        namespace_dns=uuid_utils_compat.NAMESPACE_DNS,
        hash_name="python.org",
    )


def uuid_utils_candidate() -> Candidate:
    if uuid_utils is None:
        return Candidate(
            key="uuid_utils",
            display_name="_`uuid_utils`_",
            class_name="_`uuid_utils.UUID`_",
            stdlib_compatible=False,
            setup=uninstall_uuideal,
            module=None,
            uuid_constructor=None,
            categories=("generation", "conversions", "access"),
            namespace_dns=None,
            hash_name="python.org",
            required_distribution="uuid-utils",
        )

    return Candidate(
        key="uuid_utils",
        display_name="_`uuid_utils`_",
        class_name="_`uuid_utils.UUID`_",
        stdlib_compatible=False,
        setup=uninstall_uuideal,
        module=uuid_utils,
        uuid_constructor=uuid_utils.UUID,
        categories=("generation", "conversions", "access"),
        namespace_dns=uuid_utils.NAMESPACE_DNS,
        hash_name="python.org",
    )


def uuideal_candidate() -> Candidate:
    return Candidate(
        key="uuideal",
        display_name="stdlib&nbsp;+&nbsp;`uuideal`",
        class_name="`uuid.UUID`",
        stdlib_compatible=True,
        setup=install_uuideal,
        module=uuid,
        uuid_constructor=uuid.UUID,
        categories=("generation", "conversions", "access"),
        namespace_dns=uuid.NAMESPACE_DNS,
        hash_name="python.org",
    )


def all_candidates() -> list[Candidate]:
    return [
        stdlib_candidate(),
        fastuuid_candidate(),
        uuid_utils_compat_candidate(),
        uuid_utils_candidate(),
        uuideal_candidate(),
    ]


def candidate_by_key(candidate_key: str) -> Candidate:
    for candidate in all_candidates():
        if candidate.key == candidate_key:
            return candidate
    raise ValueError(f"unknown candidate: {candidate_key}")


def available_cpu_count() -> int:
    try:
        affinity = os.sched_getaffinity(0)
    except (AttributeError, OSError):
        affinity = None
    if affinity:
        return len(affinity)
    return os.cpu_count() or 1


def normalize_worker_count(requested_workers: int) -> int:
    return max(1, min(requested_workers, available_cpu_count()))


def affinity_eligible_cores() -> list[int]:
    try:
        return sorted(os.sched_getaffinity(0))
    except (AttributeError, OSError):
        return list(range(available_cpu_count()))


def set_linux_worker_affinity(worker_index: int, dedicated_cores: bool) -> str | None:
    if not dedicated_cores:
        return None
    if platform.system() != "Linux":
        return None
    if not hasattr(os, "sched_setaffinity"):
        return "CPU affinity unavailable (no os.sched_setaffinity)"

    cores = affinity_eligible_cores()
    if not cores:
        return "CPU affinity skipped (no eligible cores)"

    core = cores[worker_index % len(cores)]
    try:
        os.sched_setaffinity(0, {core})
        return f"CPU affinity set to core {core}"
    except PermissionError:
        return "CPU affinity denied (PermissionError)"
    except OSError as exc:
        return f"CPU affinity failed ({exc})"


def apply_linux_scheduler_hints() -> list[str]:
    if platform.system() != "Linux":
        return []

    applied = []
    try:
        os.nice(-20)
        applied.append("nice adjusted by -20")
    except PermissionError:
        applied.append("nice(-20) denied (PermissionError)")
    except OSError as exc:
        applied.append(f"nice(-20) failed ({exc})")

    if not hasattr(os, "sched_setscheduler"):
        applied.append("SCHED_FIFO unavailable (no os.sched_setscheduler)")
        return applied

    try:
        os.sched_setscheduler(
            0,
            os.SCHED_FIFO,
            os.sched_param(LINUX_SCHED_FIFO_PRIORITY),
        )
        applied.append("SCHED_FIFO applied")
    except PermissionError:
        applied.append("SCHED_FIFO denied (PermissionError)")
    except OSError as exc:
        applied.append(f"SCHED_FIFO failed ({exc})")
    return applied


def apply_darwin_scheduler_hints() -> list[str]:
    if platform.system() != "Darwin":
        return []

    applied = []
    try:
        os.nice(-20)
        applied.append("nice adjusted by `-20`")
    except PermissionError:
        applied.append("`nice(-20)` denied (`PermissionError`)")
    except OSError as exc:
        applied.append(f"`nice(-20)` failed (`{exc}`)")

    try:
        libc = ctypes.CDLL("/usr/lib/libc.dylib", use_errno=True)
        pthread_self = libc.pthread_self
        pthread_self.restype = ctypes.c_void_p
        pthread_set_qos_class_self_np = libc.pthread_set_qos_class_self_np
        pthread_set_qos_class_self_np.argtypes = [ctypes.c_uint, ctypes.c_int]
        pthread_set_qos_class_self_np.restype = ctypes.c_int
        rc = pthread_set_qos_class_self_np(DARWIN_THREAD_QOS_USER_INTERACTIVE, 0)
        if rc == 0:
            applied.append("thread QoS set to `USER_INTERACTIVE`")
        else:
            applied.append(f"thread QoS call returned `{rc}`")
    except AttributeError:
        applied.append("thread QoS unavailable (missing symbol)")
    except OSError as exc:
        applied.append(f"thread QoS failed (`{exc}`)")
    return applied


def apply_scheduler_hints() -> list[str]:
    return apply_linux_scheduler_hints() + apply_darwin_scheduler_hints()


def configure_worker_process(
    *,
    worker_index: int,
    dedicated_cores: bool,
    scheduler_hints: bool,
) -> list[str]:
    signal.signal(signal.SIGINT, signal.SIG_IGN)
    policies: list[str] = []

    if dedicated_cores:
        affinity_result = set_linux_worker_affinity(worker_index, dedicated_cores)
        if affinity_result is not None:
            policies.append(affinity_result)
    else:
        policies.append("CPU affinity disabled")

    if scheduler_hints:
        policies.extend(apply_scheduler_hints())
    else:
        policies.append("scheduler hints disabled")

    return policies


def generation_cases() -> tuple[GenerationCase, ...]:
    return (
        GenerationCase(
            "uuid1",
            "`uuid1()`",
            expected_is_safe=uuid.SafeUUID.unknown,
            force_generate_time_safe_none=True,
        ),
        GenerationCase(
            "uuid1",
            "`uuid1()`<br>`safe`",
            expected_is_safe=uuid.SafeUUID.safe,
        ),
        GenerationCase("uuid3", "`uuid3()`"),
        GenerationCase("uuid4", "`uuid4()`"),
        GenerationCase("uuid5", "`uuid5()`"),
        GenerationCase("uuid6", "`uuid6()`"),
        GenerationCase("uuid7", "`uuid7()`"),
        GenerationCase("uuid8", "`uuid8()`"),
    )


def selected_candidates(config: BenchmarkConfig) -> list[Candidate]:
    return [
        candidate
        for candidate in all_candidates()
        if (
            config.selected_candidate_keys is None
            or candidate.key in config.selected_candidate_keys
        )
        and candidate.key not in config.excluded_candidate_keys
    ]


def candidates_for(category: str, config: BenchmarkConfig) -> list[Candidate]:
    return [
        candidate for candidate in selected_candidates(config) if category in candidate.categories
    ]


def fully_qualified_type_name(value: object) -> str:
    value_type = type(value)
    return f"`{value_type.__module__}.{value_type.__qualname__}`"


def progress_items(
    items: Iterable[BenchmarkProgressItem],
    *,
    total: int,
    description: str,
) -> Iterator[BenchmarkProgressItem]:
    if tqdm is None or not sys.stderr.isatty():
        yield from items
        return

    yield from tqdm(items, total=total, desc=description, unit="case", leave=False)


def cpu_name() -> str:
    if platform.system() == "Darwin":
        try:
            return subprocess.check_output(
                ["sysctl", "-n", "machdep.cpu.brand_string"],
                text=True,
            ).strip()
        except (OSError, subprocess.CalledProcessError, UnicodeDecodeError):
            pass

    return platform.processor() or platform.machine()


def operating_system_name() -> str:
    if platform.system() == "Darwin":
        macos_version = platform.mac_ver()[0]
        if macos_version:
            return f"macOS {macos_version}"

    return f"{platform.system()} {platform.release()}"


def benchmark(callable_under_test: Callable[[], object], config: BenchmarkConfig) -> float:
    timer = timeit.Timer(callable_under_test)
    loops = 1
    while timer.timeit(number=loops) < config.minimum_time:
        loops *= 2

    timings = timer.repeat(repeat=config.repeats, number=loops)
    return min(timings) * 1_000_000_000 / loops


def unavailable_result(
    *,
    category: str,
    operation: str,
    candidate: Candidate,
    note: str,
) -> BenchmarkResult:
    return BenchmarkResult(
        category=category,
        operation=operation,
        candidate_key=candidate.key,
        candidate_name=candidate.display_name,
        candidate_class_name=candidate.class_name,
        stdlib_compatible=candidate.stdlib_compatible,
        return_type="—",
        nanoseconds_per_operation=None,
        note=note,
    )


def missing_candidate_note(candidate: Candidate) -> str:
    if candidate.required_distribution is None:
        return "not available"
    return f"install `{candidate.required_distribution}` benchmark extra"


def uses_stdlib_uuid_module(candidate: Candidate) -> bool:
    return candidate.key in {"stdlib", "uuideal"}


def uuid_is_safe_value(value: object) -> uuid.SafeUUID | None:
    return getattr(value, "is_safe", None)


def unexpected_is_safe_note(expected: uuid.SafeUUID) -> str:
    return "N/A"


def interleave_case_groups(case_groups: Sequence[Sequence[BenchmarkCase]]) -> list[BenchmarkCase]:
    interleaved: list[BenchmarkCase] = []
    longest_group = max((len(group) for group in case_groups), default=0)
    for case_index in range(longest_group):
        for group in case_groups:
            if case_index < len(group):
                interleaved.append(group[case_index])
    return interleaved


def generation_benchmark_cases(config: BenchmarkConfig) -> list[BenchmarkCase]:
    candidates = candidates_for("generation", config)
    return interleave_case_groups(
        [
            [
                BenchmarkCase(
                    category="generation",
                    operation=generation_case.operation,
                    candidate_key=candidate.key,
                    generation_function_name=generation_case.function_name,
                    expected_is_safe=generation_case.expected_is_safe,
                    force_generate_time_safe_none=generation_case.force_generate_time_safe_none,
                )
                for generation_case in generation_cases()
            ]
            for candidate in candidates
        ]
    )


def conversion_benchmark_cases(config: BenchmarkConfig) -> list[BenchmarkCase]:
    candidates = candidates_for("conversions", config)
    return interleave_case_groups(
        [
            [
                BenchmarkCase(
                    category="conversions",
                    operation=operation,
                    candidate_key=candidate.key,
                )
                for operation in CONVERSION_OPERATIONS
            ]
            for candidate in candidates
        ]
    )


def access_benchmark_cases(config: BenchmarkConfig) -> list[BenchmarkCase]:
    candidates = candidates_for("access", config)
    field_groups = [
        [
            BenchmarkCase(
                category="access",
                operation=f"`value.{field}`",
                candidate_key=candidate.key,
                access_field=field,
            )
            for field in PUBLIC_UUID_FIELDS
        ]
        for candidate in candidates
    ]
    sort_cases = [
        BenchmarkCase(
            category="access",
            operation=ACCESS_SORT_OPERATION,
            candidate_key=candidate.key,
        )
        for candidate in candidates
    ]
    return interleave_case_groups(field_groups) + sort_cases


def benchmark_cases_for_category(category: str, config: BenchmarkConfig) -> list[BenchmarkCase]:
    if category == "generation":
        return generation_benchmark_cases(config)
    if category == "conversions":
        return conversion_benchmark_cases(config)
    if category == "access":
        return access_benchmark_cases(config)
    raise ValueError(f"unknown benchmark category: {category}")


def generation_callable(candidate: Candidate, function_name: str) -> Callable[[], object] | None:
    if candidate.module is None:
        return None

    module = candidate.module
    if candidate.key == "uuideal" and not hasattr(module, function_name):
        module = uuideal if hasattr(uuideal, function_name) else module

    function = getattr(module, function_name, None)
    if function is None:
        return None

    if function_name in {"uuid3", "uuid5"}:
        if candidate.namespace_dns is None:
            return None
        return lambda: function(candidate.namespace_dns, candidate.hash_name)

    return function


def run_generation_case(case: BenchmarkCase, config: BenchmarkConfig) -> BenchmarkResult:
    candidate = candidate_by_key(case.candidate_key)
    candidate.setup()

    if case.generation_function_name is None:
        raise ValueError("generation case is missing function name")

    callable_under_test = generation_callable(candidate, case.generation_function_name)
    if callable_under_test is None:
        return unavailable_result(
            category="generation",
            operation=case.operation,
            candidate=candidate,
            note=missing_candidate_note(candidate),
        )

    should_patch_generate_time_safe = (
        case.force_generate_time_safe_none and uses_stdlib_uuid_module(candidate)
    )

    if should_patch_generate_time_safe and not hasattr(uuid, "_generate_time_safe"):
        return unavailable_result(
            category="generation",
            operation=case.operation,
            candidate=candidate,
            note="no `uuid._generate_time_safe`",
        )

    original_generate_time_safe = getattr(uuid, "_generate_time_safe", None)
    if should_patch_generate_time_safe and original_generate_time_safe is None:
        return unavailable_result(
            category="generation",
            operation=case.operation,
            candidate=candidate,
            note="`uuid._generate_time_safe` is `None`",
        )

    try:
        if should_patch_generate_time_safe:
            uuid._generate_time_safe = None

        first_generated_value = callable_under_test()
        generated_value = (
            callable_under_test() if case.expected_is_safe is not None else first_generated_value
        )
        if (
            case.expected_is_safe is not None
            and uuid_is_safe_value(generated_value) is not case.expected_is_safe
        ):
            return unavailable_result(
                category="generation",
                operation=case.operation,
                candidate=candidate,
                note=unexpected_is_safe_note(case.expected_is_safe),
            )

        return BenchmarkResult(
            category="generation",
            operation=case.operation,
            candidate_key=candidate.key,
            candidate_name=candidate.display_name,
            candidate_class_name=candidate.class_name,
            stdlib_compatible=candidate.stdlib_compatible,
            return_type=fully_qualified_type_name(generated_value),
            nanoseconds_per_operation=benchmark(callable_under_test, config),
        )
    finally:
        if should_patch_generate_time_safe:
            uuid._generate_time_safe = original_generate_time_safe
        uninstall_uuideal()


def conversion_callable(
    candidate: Candidate, operation: str
) -> tuple[Callable[[], object], object] | None:
    if candidate.uuid_constructor is None:
        return None

    value = candidate.uuid_constructor(HEX_VALUE)
    payload = pickle.dumps(value, protocol=4)

    if operation == "`UUID('<hex>')`":
        return lambda: candidate.uuid_constructor(HEX_VALUE), value
    if operation == "`str(value)`":
        return lambda: str(value), str(value)
    if operation == "`pickle.dumps(value)`":
        return lambda: pickle.dumps(value, protocol=4), payload
    if operation == "`pickle.loads(payload)`":
        loaded_value = pickle.loads(payload)
        return lambda: pickle.loads(payload), loaded_value

    raise ValueError(f"unknown conversion operation: {operation}")


def run_conversion_case(case: BenchmarkCase, config: BenchmarkConfig) -> BenchmarkResult:
    candidate = candidate_by_key(case.candidate_key)
    try:
        candidate.setup()
        callable_with_sample = conversion_callable(candidate, case.operation)
        if callable_with_sample is None:
            return unavailable_result(
                category="conversions",
                operation=case.operation,
                candidate=candidate,
                note=missing_candidate_note(candidate),
            )

        callable_under_test, sample_value = callable_with_sample
        return BenchmarkResult(
            category="conversions",
            operation=case.operation,
            candidate_key=candidate.key,
            candidate_name=candidate.display_name,
            candidate_class_name=candidate.class_name,
            stdlib_compatible=candidate.stdlib_compatible,
            return_type=fully_qualified_type_name(sample_value),
            nanoseconds_per_operation=benchmark(callable_under_test, config),
        )
    finally:
        uninstall_uuideal()


def run_access_case(case: BenchmarkCase, config: BenchmarkConfig) -> BenchmarkResult:
    candidate = candidate_by_key(case.candidate_key)
    try:
        candidate.setup()
        if candidate.uuid_constructor is None:
            return unavailable_result(
                category="access",
                operation=case.operation,
                candidate=candidate,
                note=missing_candidate_note(candidate),
            )

        if case.operation == ACCESS_SORT_OPERATION:
            if candidate.key == "uuid_utils" and candidate.module is not None:
                values = [candidate.module.uuid4() for _ in range(1000)]
            else:
                values = [uuid.uuid4() for _ in range(1000)]

            def callable_under_test(values=values):
                return sorted(values)

            sample_value = callable_under_test()
            return BenchmarkResult(
                category="access",
                operation=case.operation,
                candidate_key=candidate.key,
                candidate_name=candidate.display_name,
                candidate_class_name=candidate.class_name,
                stdlib_compatible=candidate.stdlib_compatible,
                return_type=fully_qualified_type_name(sample_value),
                nanoseconds_per_operation=benchmark(callable_under_test, config),
            )

        if case.access_field is None:
            raise ValueError("access case is missing field")

        value = candidate.uuid_constructor(hex=HEX_VALUE)
        if not hasattr(value, case.access_field):
            return unavailable_result(
                category="access",
                operation=case.operation,
                candidate=candidate,
                note="attribute not available",
            )

        def callable_under_test(value=value, field=case.access_field):
            return getattr(value, field)

        sample_value = callable_under_test()
        return BenchmarkResult(
            category="access",
            operation=case.operation,
            candidate_key=candidate.key,
            candidate_name=candidate.display_name,
            candidate_class_name=candidate.class_name,
            stdlib_compatible=candidate.stdlib_compatible,
            return_type=fully_qualified_type_name(sample_value),
            nanoseconds_per_operation=benchmark(callable_under_test, config),
        )
    finally:
        uninstall_uuideal()


def run_benchmark_case(case: BenchmarkCase, config: BenchmarkConfig) -> BenchmarkResult:
    if case.category == "generation":
        return run_generation_case(case, config)
    if case.category == "conversions":
        return run_conversion_case(case, config)
    if case.category == "access":
        return run_access_case(case, config)
    raise ValueError(f"unknown benchmark category: {case.category}")


def worker_loop(
    work_queue: Queue,
    result_queue: Queue,
    config: BenchmarkConfig,
    worker_index: int,
) -> None:
    policies = configure_worker_process(
        worker_index=worker_index,
        dedicated_cores=config.dedicated_cores,
        scheduler_hints=config.scheduler_hints,
    )
    result_queue.put(SchedulingReport(worker_index=worker_index, policies=tuple(policies)))

    while True:
        item = work_queue.get()
        if item is None:
            return

        case_index, case = item
        try:
            result = run_benchmark_case(case, config)
        except BaseException as exc:
            candidate = candidate_by_key(case.candidate_key)
            result = unavailable_result(
                category=case.category,
                operation=case.operation,
                candidate=candidate,
                note=f"N/A",
            )
        result_queue.put(WorkerResult(case_index=case_index, result=result))


def run_cases_in_processes(
    cases: list[BenchmarkCase],
    config: BenchmarkConfig,
    *,
    description: str,
) -> tuple[list[BenchmarkResult], tuple[str, ...]]:
    if not cases:
        return [], ()

    worker_count = normalize_worker_count(config.workers)
    if worker_count == 1:
        results: list[BenchmarkResult] = []
        policies = configure_worker_process(
            worker_index=0,
            dedicated_cores=config.dedicated_cores,
            scheduler_hints=config.scheduler_hints,
        )
        for case in progress_items(cases, total=len(cases), description=description):
            results.append(run_benchmark_case(case, config))
        return results, tuple(policies)

    context: BaseContext = get_context("spawn")
    work_queue: Queue = context.Queue()
    result_queue: Queue = context.Queue()

    for case_index, case in enumerate(cases):
        work_queue.put((case_index, case))
    for _ in range(worker_count):
        work_queue.put(None)

    workers = [
        context.Process(
            target=worker_loop,
            args=(work_queue, result_queue, config, worker_index),
        )
        for worker_index in range(worker_count)
    ]

    for worker in workers:
        worker.start()

    indexed_results: dict[int, BenchmarkResult] = {}
    scheduling_policies: tuple[str, ...] = ()
    scheduling_reports_pending = worker_count
    results_pending = len(cases)
    progress = progress_items(range(len(cases)), total=len(cases), description=description)
    progress_iter = iter(progress)
    try:
        while results_pending > 0 or scheduling_reports_pending > 0:
            try:
                message = result_queue.get(timeout=0.1)
            except queue.Empty:
                for worker in workers:
                    if not worker.is_alive() and worker.exitcode not in (None, 0):
                        raise RuntimeError(
                            f"benchmark worker {worker.pid} exited with {worker.exitcode}"
                        )
                continue

            if isinstance(message, SchedulingReport):
                if not scheduling_policies:
                    scheduling_policies = message.policies
                scheduling_reports_pending -= 1
            elif isinstance(message, WorkerResult):
                indexed_results[message.case_index] = message.result
                results_pending -= 1
                try:
                    next(progress_iter)
                except StopIteration:
                    pass
    finally:
        for worker in workers:
            worker.join(timeout=1)
        for worker in workers:
            if worker.is_alive():
                worker.terminate()
                worker.join()

    return [indexed_results[index] for index in range(len(cases))], scheduling_policies


def benchmark_suite(
    config: BenchmarkConfig,
    *,
    generated_at: datetime,
    selected_categories: tuple[str, ...],
    generation: list[BenchmarkResult],
    conversions: list[BenchmarkResult],
    access: list[BenchmarkResult],
    applied_scheduling: tuple[str, ...] = (),
) -> BenchmarkSuite:
    return BenchmarkSuite(
        generated_at=generated_at,
        python=sys.version.split()[0],
        cpu=cpu_name(),
        operating_system=operating_system_name(),
        repeats=config.repeats,
        minimum_time=config.minimum_time,
        workers=normalize_worker_count(config.workers),
        dedicated_cores=config.dedicated_cores,
        scheduler_hints=config.scheduler_hints,
        highlight_fastest=config.highlight_fastest,
        selected_categories=selected_categories,
        generation=generation,
        conversions=conversions,
        access=access,
        applied_scheduling=applied_scheduling,
    )


def run_benchmark_category(
    category: str,
    config: BenchmarkConfig,
) -> tuple[list[BenchmarkResult], tuple[str, ...]]:
    cases = benchmark_cases_for_category(category, config)
    return run_cases_in_processes(
        cases,
        config,
        description=BENCHMARK_TITLES[category],
    )


def run_benchmarks(
    config: BenchmarkConfig,
    *,
    table_ready: Callable[[BenchmarkSuite], None] | None = None,
) -> BenchmarkSuite:
    generated_at = datetime.now(UTC)
    generation: list[BenchmarkResult] = []
    conversions: list[BenchmarkResult] = []
    access: list[BenchmarkResult] = []
    applied_scheduling: tuple[str, ...] = ()

    for category in config.selected_categories:
        results, scheduling = run_benchmark_category(category, config)
        if not applied_scheduling:
            applied_scheduling = scheduling

        if category == "generation":
            generation = results
        elif category == "conversions":
            conversions = results
        elif category == "access":
            access = results

        if table_ready is not None:
            table_ready(
                benchmark_suite(
                    config,
                    generated_at=generated_at,
                    selected_categories=(category,),
                    generation=generation,
                    conversions=conversions,
                    access=access,
                    applied_scheduling=applied_scheduling,
                )
            )

    return benchmark_suite(
        config,
        generated_at=generated_at,
        selected_categories=config.selected_categories,
        generation=generation,
        conversions=conversions,
        access=access,
        applied_scheduling=applied_scheduling,
    )


def baseline_by_operation(results: list[BenchmarkResult]) -> dict[str, float]:
    baselines: dict[str, float] = {}
    for result in results:
        if result.candidate_key == "stdlib" and result.nanoseconds_per_operation is not None:
            baselines[result.operation] = result.nanoseconds_per_operation
    return baselines


def format_nanoseconds(value: float | None) -> str:
    if value is None:
        return "—"
    if value >= 1_000:
        return f"{value:,.0f}"
    return f"{value:,.1f}"


def format_speedup(result: BenchmarkResult, baseline: float | None) -> str:
    if result.nanoseconds_per_operation is None or baseline is None:
        return "—"
    return f"{baseline / result.nanoseconds_per_operation:.2f}×"


def escape_table_cell(value: str) -> str:
    return value.replace("|", "\\|")


def unique_operations(results: list[BenchmarkResult]) -> list[str]:
    operations: list[str] = []
    for result in results:
        if result.operation not in operations:
            operations.append(result.operation)
    return operations


def unique_candidate_keys(results: list[BenchmarkResult]) -> list[str]:
    candidate_keys: list[str] = []
    for result in results:
        if result.candidate_key not in candidate_keys:
            candidate_keys.append(result.candidate_key)
    return candidate_keys


def candidate_names_by_key(results: list[BenchmarkResult]) -> dict[str, str]:
    candidate_names: dict[str, str] = {}
    for result in results:
        candidate_names.setdefault(result.candidate_key, result.candidate_name)
    return candidate_names


def candidate_classes_by_key(results: list[BenchmarkResult]) -> dict[str, str]:
    candidate_classes: dict[str, str] = {}
    for result in results:
        candidate_classes.setdefault(result.candidate_key, result.candidate_class_name)
    return candidate_classes


def best_times(results: list[BenchmarkResult], *, include_incompatible: bool) -> dict[str, float]:
    fastest_times: dict[str, float] = {}
    for operation in unique_operations(results):
        operation_times = [
            result.nanoseconds_per_operation
            for result in results
            if result.operation == operation
            and (include_incompatible or result.stdlib_compatible)
            and result.nanoseconds_per_operation is not None
        ]
        if operation_times:
            fastest_times[operation] = min(operation_times)
    return fastest_times


def results_by_operation_and_candidate(
    results: list[BenchmarkResult],
) -> dict[tuple[str, str], BenchmarkResult]:
    return {(result.operation, result.candidate_key): result for result in results}


def format_result_cell(
    result: BenchmarkResult | None,
    baseline: float | None,
    best_time: float | None,
) -> str:
    if result is None:
        return "—"
    if result.nanoseconds_per_operation is None:
        return escape_table_cell(result.note) if result.note else "—"

    cell = f"{format_nanoseconds(result.nanoseconds_per_operation)} ns"
    if result.candidate_key != "stdlib":
        cell += f" ({format_speedup(result, baseline)})"
    if best_time is not None and result.nanoseconds_per_operation == best_time:
        return f"**{cell}**"
    return cell


def candidate_aggregates_by_key(results: list[BenchmarkResult]) -> dict[str, CandidateAggregate]:
    baselines = baseline_by_operation(results)
    candidate_keys = unique_candidate_keys(results)
    speedups_by_candidate_and_group: dict[str, dict[str, list[float]]] = {
        candidate_key: {} for candidate_key in candidate_keys
    }

    for result in results:
        if result.nanoseconds_per_operation is None:
            continue
        if result.nanoseconds_per_operation <= 0:
            continue

        baseline = baselines.get(result.operation)
        if baseline is None or baseline <= 0:
            continue

        speedups_by_candidate_and_group[result.candidate_key].setdefault(
            aggregate_operation_key(result),
            [],
        ).append(baseline / result.nanoseconds_per_operation)

    complete_group_keys = {
        group_key
        for group_key in {
            group_key
            for grouped_speedups in speedups_by_candidate_and_group.values()
            for group_key in grouped_speedups
        }
        if all(
            speedups_by_candidate_and_group[candidate_key].get(group_key)
            for candidate_key in candidate_keys
        )
    }

    return {
        candidate_key: CandidateAggregate(
            geomean_speedup=geomean(
                [
                    max(speedups)
                    for group_key, speedups in grouped_speedups.items()
                    if group_key in complete_group_keys
                ]
            )
        )
        for candidate_key, grouped_speedups in speedups_by_candidate_and_group.items()
    }


def stdlib_compatible_candidate_keys(results: list[BenchmarkResult]) -> set[str]:
    return {result.candidate_key for result in results if result.stdlib_compatible}


def best_geomean_speedup(
    aggregates: dict[str, CandidateAggregate],
    compatible_candidate_keys: set[str],
    *,
    include_incompatible: bool,
) -> float | None:
    geomean_speedups = [
        aggregate.geomean_speedup
        for candidate_key, aggregate in aggregates.items()
        if (include_incompatible or candidate_key in compatible_candidate_keys)
        and aggregate.geomean_speedup is not None
    ]
    return max(geomean_speedups) if geomean_speedups else None


def maybe_bold(value: str, *, should_bold: bool) -> str:
    if should_bold:
        return f"**{value}**"
    return value


def format_geomean_cell(value: float | None, best_value: float | None) -> str:
    if value is None:
        return "—"

    cell = f"{value:.2f}×"
    return maybe_bold(cell, should_bold=best_value is not None and value == best_value)


def aggregate_operation_key(result: BenchmarkResult) -> str:
    if result.category == "generation" and result.operation.startswith("`uuid1()`"):
        return "uuid1"
    return result.operation


def geomean(values: list[float]) -> float | None:
    if not values:
        return None
    return math.exp(sum(math.log(value) for value in values) / len(values))


def format_stdlib_uuid_usage_cell(candidate_key: str, compatible_candidate_keys: set[str]) -> str:
    return "✓" if candidate_key in compatible_candidate_keys else "✗"


def render_table(results: list[BenchmarkResult], *, highlight_fastest: bool) -> str:
    if not results:
        return "_No selected candidates support this benchmark group._"

    baselines = baseline_by_operation(results)
    candidate_keys = unique_candidate_keys(results)
    candidate_names = candidate_names_by_key(results)
    fastest_times = best_times(results, include_incompatible=highlight_fastest)
    result_lookup = results_by_operation_and_candidate(results)
    aggregates = candidate_aggregates_by_key(results)
    compatible_candidate_keys = stdlib_compatible_candidate_keys(results)
    best_speedup = best_geomean_speedup(
        aggregates,
        compatible_candidate_keys,
        include_incompatible=highlight_fastest,
    )

    lines = [
        "| Operation | " + " | ".join(candidate_names[key] for key in candidate_keys) + " |",
        "|---|" + "|".join("---:" for _ in candidate_keys) + "|",
    ]

    for operation in unique_operations(results):
        baseline = baselines.get(operation)
        best_time = fastest_times.get(operation)
        cells = [
            format_result_cell(
                result_lookup.get((operation, candidate_key)),
                baseline,
                best_time,
            )
            for candidate_key in candidate_keys
        ]
        lines.append(f"| {escape_table_cell(operation)} | " + " | ".join(cells) + " |")

    geomean_cells = [
        format_geomean_cell(aggregates[candidate_key].geomean_speedup, best_speedup)
        for candidate_key in candidate_keys
    ]
    lines.append(
        f"| **Speedup (geomean)**{GEOMEAN_FOOTNOTE_MARKER} | " + " | ".join(geomean_cells) + " |"
    )

    return "\n".join(lines)


def highlight_description(highlight_fastest: bool) -> str:
    if highlight_fastest:
        return "Bold marks the fastest option per row."
    return "Bold marks the fastest stdlib-compatible option per row."


def format_minimum_time(seconds: float) -> str:
    if seconds < 1:
        return f"{seconds * 1_000:g}ms"
    return f"{seconds:g}s"


def scheduler_policy_description(suite: BenchmarkSuite) -> str:
    policies = [f"{suite.workers} worker process(es)"]
    if suite.applied_scheduling:
        policies.extend(suite.applied_scheduling)
    else:
        if suite.dedicated_cores:
            policies.append("CPU affinity enabled (no worker report)")
        else:
            policies.append("CPU affinity disabled")
        if suite.scheduler_hints:
            policies.append("scheduler hints enabled (no worker report)")
        else:
            policies.append("scheduler hints disabled")
    return ", ".join(policies)


def run_metadata(suite: BenchmarkSuite) -> str:
    return (
        f"\nRan on `{suite.cpu}` `{suite.operating_system}` `CPython {suite.python}` "
        f"best of `{suite.repeats}` repeats after autoranging each case to at least "
        f"`{format_minimum_time(suite.minimum_time)}` using "
        f"{scheduler_policy_description(suite)}."
    )


def results_for_category(suite: BenchmarkSuite, category: str) -> list[BenchmarkResult]:
    if category == "generation":
        return suite.generation
    if category == "conversions":
        return suite.conversions
    if category == "access":
        return suite.access
    raise ValueError(f"unknown benchmark category: {category}")


def render_table_section(suite: BenchmarkSuite, category: str) -> str:
    lines = [
        TABLE_START_MARKERS[category],
        "",
        f"### {BENCHMARK_TITLES[category]}",
        "",
        render_table(
            results_for_category(suite, category),
            highlight_fastest=suite.highlight_fastest,
        ),
        "",
        TABLE_END_MARKERS[category],
    ]
    return "\n".join(lines)


def geomean_footnote() -> str:
    return (
        f'_<span id="{GEOMEAN_FOOTNOTE_ID}">*</span> '
        "Geomean uses only operation groups where every displayed candidate has valid timing data._"
    )


def render_markdown(suite: BenchmarkSuite) -> str:
    lines = [
        README_START_MARKER,
    ]

    for category in suite.selected_categories:
        lines.extend(("", render_table_section(suite, category)))

    lines.extend(
        (
            "",
            run_metadata(suite),
            "",
            geomean_footnote(),
            "",
            README_END_MARKER,
        )
    )
    return "\n".join(lines)


def replace_marked_section(
    original: str,
    *,
    start_marker: str,
    end_marker: str,
    replacement: str,
) -> str | None:
    start_index = original.find(start_marker)
    end_index = original.find(end_marker)
    if start_index == -1 or end_index == -1:
        return None

    replacement_end = end_index + len(end_marker)
    return original[:start_index] + replacement + original[replacement_end:]


def replace_legacy_table_section(original: str, category: str, replacement: str) -> str | None:
    heading = f"### {BENCHMARK_TITLES[category]}"
    heading_index = original.find(heading)
    if heading_index == -1:
        return None

    next_heading_index = original.find("\n### ", heading_index + len(heading))
    end_marker_index = original.find(README_END_MARKER, heading_index)
    if next_heading_index != -1 and (
        end_marker_index == -1 or next_heading_index < end_marker_index
    ):
        return original[:heading_index] + replacement + "\n\n" + original[next_heading_index + 1 :]
    if end_marker_index != -1:
        return original[:heading_index] + replacement + "\n\n" + original[end_marker_index:]

    return original[:heading_index] + replacement + "\n" + original[heading_index + len(heading) :]


def insert_table_section(original: str, replacement: str) -> str:
    end_marker_index = original.find(README_END_MARKER)
    if end_marker_index != -1:
        return (
            original[:end_marker_index].rstrip()
            + "\n\n"
            + replacement
            + "\n\n"
            + original[end_marker_index:]
        )
    return original.rstrip() + "\n\n" + replacement + "\n"


def remove_benchmark_metadata(original: str) -> str:
    lines = original.splitlines(keepends=True)
    return "".join(line for line in lines if not line.startswith("Ran on `"))


def insert_benchmark_metadata(original: str, suite: BenchmarkSuite) -> str:
    metadata = run_metadata(suite).lstrip()
    end_marker_index = original.find(README_END_MARKER)
    if end_marker_index != -1:
        return (
            original[:end_marker_index].rstrip()
            + "\n\n"
            + metadata
            + "\n\n"
            + original[end_marker_index:]
        )
    return original.rstrip() + "\n\n" + metadata + "\n"


def update_readme(readme_path: Path, suite: BenchmarkSuite) -> None:
    benchmark_markdown = render_markdown(suite)
    original = readme_path.read_text()
    should_update_whole_section = set(suite.selected_categories) == set(BENCHMARK_CATEGORIES)

    if should_update_whole_section:
        start_index = original.find(README_START_MARKER)
        end_index = original.find(README_END_MARKER)
        if start_index != -1 and end_index != -1:
            section_start = original.rfind("## Benchmarks", 0, start_index)
            if section_start == -1:
                section_start = start_index
            replacement_end = end_index + len(README_END_MARKER)
            updated = (
                original[:section_start].rstrip()
                + "\n\n"
                + benchmark_markdown
                + original[replacement_end:]
            )
        else:
            updated = original.rstrip() + "\n\n" + benchmark_markdown + "\n"
        readme_path.write_text(updated)
        return

    updated = remove_benchmark_metadata(original)
    for category in suite.selected_categories:
        replacement = render_table_section(suite, category)
        replaced = replace_marked_section(
            updated,
            start_marker=TABLE_START_MARKERS[category],
            end_marker=TABLE_END_MARKERS[category],
            replacement=replacement,
        )
        if replaced is None:
            replaced = replace_legacy_table_section(updated, category, replacement)
        updated = replaced if replaced is not None else insert_table_section(updated, replacement)

    readme_path.write_text(insert_benchmark_metadata(updated, suite))


def save_benchmark_suite(path: Path, suite: BenchmarkSuite) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("wb") as benchmark_file:
        pickle.dump(suite, benchmark_file, protocol=pickle.HIGHEST_PROTOCOL)


def load_benchmark_suite(path: Path) -> BenchmarkSuite:
    with path.open("rb") as benchmark_file:
        suite = pickle.load(benchmark_file)
    if not isinstance(suite, BenchmarkSuite):
        raise TypeError(f"{path} does not contain a BenchmarkSuite")
    return suite


def filter_results_for_loaded_run(
    results: list[BenchmarkResult],
    config: BenchmarkConfig,
) -> list[BenchmarkResult]:
    return [
        result
        for result in results
        if (
            config.selected_candidate_keys is None
            or result.candidate_key in config.selected_candidate_keys
        )
        and result.candidate_key not in config.excluded_candidate_keys
    ]


def suite_for_rendering_loaded_run(
    suite: BenchmarkSuite,
    config: BenchmarkConfig,
) -> BenchmarkSuite:
    return BenchmarkSuite(
        generated_at=suite.generated_at,
        python=suite.python,
        cpu=suite.cpu,
        operating_system=suite.operating_system,
        repeats=suite.repeats,
        minimum_time=suite.minimum_time,
        workers=suite.workers,
        dedicated_cores=suite.dedicated_cores,
        scheduler_hints=suite.scheduler_hints,
        highlight_fastest=config.highlight_fastest,
        selected_categories=config.selected_categories,
        generation=filter_results_for_loaded_run(suite.generation, config),
        conversions=filter_results_for_loaded_run(suite.conversions, config),
        access=filter_results_for_loaded_run(suite.access, config),
        applied_scheduling=suite.applied_scheduling,
    )


def parse_candidate_key_arguments(
    parser: argparse.ArgumentParser, values: list[str] | None
) -> tuple[str, ...]:
    if values is None:
        return ()

    candidate_keys: list[str] = []
    for value in values:
        for candidate_key in value.split(","):
            normalized_key = candidate_key.strip()
            if not normalized_key:
                continue
            if normalized_key not in CANDIDATE_KEYS:
                choices = ", ".join(CANDIDATE_KEYS)
                parser.error(f"unknown candidate {normalized_key!r}; choose from: {choices}")
            if normalized_key not in candidate_keys:
                candidate_keys.append(normalized_key)
    return tuple(candidate_keys)


def parse_table_arguments(
    parser: argparse.ArgumentParser,
    values: list[str] | None,
) -> tuple[str, ...]:
    if values is None:
        return BENCHMARK_CATEGORIES

    categories: list[str] = []
    for value in values:
        for category in value.split(","):
            normalized_category = category.strip()
            if not normalized_category:
                continue
            if normalized_category not in BENCHMARK_CATEGORIES:
                choices = ", ".join(BENCHMARK_CATEGORIES)
                parser.error(
                    f"unknown benchmark table {normalized_category!r}; choose from: {choices}"
                )
            if normalized_category not in categories:
                categories.append(normalized_category)

    if not categories:
        parser.error("at least one benchmark table must be selected")
    return tuple(categories)


def parse_args() -> BenchmarkConfig:
    parser = argparse.ArgumentParser(description="Benchmark uuid, uuideal, and UUID alternatives.")
    parser.add_argument(
        "--repeats",
        type=int,
        default=3,
        help="Number of measured repeats after autoranging each benchmark case.",
    )
    parser.add_argument(
        "--minimum-time",
        type=float,
        default=0.05,
        help="Minimum seconds per repeated benchmark case before timing repeats.",
    )
    parser.add_argument(
        "--update-readme",
        type=Path,
        help="README.md path to update with the generated benchmark section.",
    )
    parser.add_argument(
        "--save-run",
        type=Path,
        help="Path to save the completed benchmark suite as a pickle file.",
    )
    parser.add_argument(
        "--load-run",
        type=Path,
        help=(
            "Path to a previously saved benchmark suite pickle. "
            "Skips benchmark execution and only renders markdown / updates README."
        ),
    )
    parser.add_argument(
        "--highlight-fastest",
        action="store_true",
        help="Bold the fastest option per row even when it returns a custom UUID class.",
    )
    parser.add_argument(
        "--candidate",
        action="append",
        metavar="KEY",
        help=(
            "Candidate to include. Repeat or comma-separate values. "
            f"Choices: {', '.join(CANDIDATE_KEYS)}. Defaults to all candidates."
        ),
    )
    parser.add_argument(
        "--exclude-candidate",
        action="append",
        metavar="KEY",
        help=(
            "Candidate to remove. Repeat or comma-separate values. "
            f"Choices: {', '.join(CANDIDATE_KEYS)}."
        ),
    )
    parser.add_argument(
        "--table",
        action="append",
        metavar="NAME",
        help=(
            "Benchmark table to run and update. Repeat or comma-separate values. "
            f"Choices: {', '.join(BENCHMARK_CATEGORIES)}. Defaults to all tables."
        ),
    )
    parser.add_argument(
        "--workers",
        type=int,
        default=DEFAULT_BENCHMARK_WORKERS,
        help=(
            "Number of spawned worker processes. Values above the available CPU count "
            "are capped. Use more than 1 to interleave benchmark cases across processes."
        ),
    )
    parser.add_argument(
        "--no-dedicated-cores",
        action="store_true",
        help="Disable best-effort Linux per-worker CPU affinity.",
    )
    parser.add_argument(
        "--no-scheduler-hints",
        action="store_true",
        help="Disable best-effort process/thread priority hints.",
    )
    args = parser.parse_args()

    if args.repeats < 1:
        parser.error("--repeats must be at least 1")
    if args.minimum_time <= 0:
        parser.error("--minimum-time must be positive")
    if args.workers < 1:
        parser.error("--workers must be at least 1")
    if args.load_run is not None and args.save_run is not None:
        parser.error("--save-run cannot be used with --load-run")

    selected_candidate_keys = parse_candidate_key_arguments(parser, args.candidate)
    excluded_candidate_keys = parse_candidate_key_arguments(parser, args.exclude_candidate)
    selected_categories = parse_table_arguments(parser, args.table)
    overlap = set(selected_candidate_keys) & set(excluded_candidate_keys)
    if overlap:
        parser.error(
            f"candidate cannot be both selected and excluded: {', '.join(sorted(overlap))}"
        )

    if selected_candidate_keys:
        active_candidate_keys = [
            candidate_key
            for candidate_key in selected_candidate_keys
            if candidate_key not in excluded_candidate_keys
        ]
    else:
        active_candidate_keys = [
            candidate_key
            for candidate_key in CANDIDATE_KEYS
            if candidate_key not in excluded_candidate_keys
        ]
    if not active_candidate_keys:
        parser.error("at least one candidate must remain selected")

    return BenchmarkConfig(
        repeats=args.repeats,
        minimum_time=args.minimum_time,
        update_readme=args.update_readme,
        save_run=args.save_run,
        load_run=args.load_run,
        highlight_fastest=args.highlight_fastest,
        selected_candidate_keys=selected_candidate_keys or None,
        excluded_candidate_keys=excluded_candidate_keys,
        selected_categories=selected_categories,
        workers=normalize_worker_count(args.workers),
        dedicated_cores=not args.no_dedicated_cores,
        scheduler_hints=not args.no_scheduler_hints,
    )


def main() -> None:
    config = parse_args()
    if config.load_run is not None:
        suite = suite_for_rendering_loaded_run(load_benchmark_suite(config.load_run), config)
        if config.update_readme is not None:
            update_readme(config.update_readme, suite)
    elif config.update_readme is None:
        suite = run_benchmarks(config)
    else:
        suite = run_benchmarks(
            config,
            table_ready=lambda ready_suite: update_readme(config.update_readme, ready_suite),
        )

    if config.save_run is not None:
        save_benchmark_suite(config.save_run, suite)

    markdown = render_markdown(suite)
    print(markdown)


if __name__ == "__main__":
    main()
