from __future__ import annotations

import multiprocessing
import uuid
from multiprocessing.context import BaseContext
from multiprocessing.queues import Queue
from typing import Any

import pytest

import uuideal


def child_uuid_smoke(
    queue: Queue[Any],
    child_index: int,
    install_in_child: bool,
) -> None:
    import uuid

    import uuideal

    if install_in_child:
        uuideal.install()
    else:
        uuideal.uninstall()

    try:
        value = uuid.uuid4()
        queue.put(
            {
                "ok": True,
                "child_index": child_index,
                "uuid": value.hex,
                "type_is_uuid": type(value) is uuid.UUID,
                "version": value.version,
                "variant_is_rfc": value.variant == uuid.RFC_4122,
                "hex_len": len(value.hex),
            }
        )
    except BaseException as error:
        queue.put(
            {
                "ok": False,
                "child_index": child_index,
                "error_type": type(error).__name__,
                "error_message": str(error),
            }
        )


def run_parent_child_uuid_smoke(
    context: BaseContext,
    install_in_parent: bool,
    install_in_children: tuple[bool, ...],
) -> dict[str, Any]:
    if install_in_parent:
        uuideal.install()
    else:
        uuideal.uninstall()

    parent_value = uuid.uuid4()

    queue: Queue[Any] = context.Queue()

    processes = [
        context.Process(
            target=child_uuid_smoke,
            args=(queue, child_index, install_in_child),
        )
        for child_index, install_in_child in enumerate(install_in_children)
    ]

    for process in processes:
        process.start()

    for process in processes:
        process.join(30)

        if process.is_alive():
            process.terminate()
            process.join()
            pytest.fail(
                f"child process did not exit for start method {context.get_start_method()!r}"
            )

        assert process.exitcode == 0

    child_results = [queue.get(timeout=5) for _ in processes]
    assert all(isinstance(result, dict) for result in child_results)

    child_results_by_index = {
        result["child_index"]: result
        for result in child_results
    }

    assert set(child_results_by_index) == set(range(len(install_in_children)))

    return {
        "parent": {
            "uuid": parent_value.hex,
            "type_is_uuid": type(parent_value) is uuid.UUID,
            "version": parent_value.version,
            "variant_is_rfc": parent_value.variant == uuid.RFC_4122,
            "hex_len": len(parent_value.hex),
        },
        "children": [
            child_results_by_index[index]
            for index in range(len(install_in_children))
        ],
    }


def assert_uuid4_result_is_valid(result: dict[str, Any]) -> None:
    assert result["type_is_uuid"] is True
    assert result["version"] == 4
    assert result["variant_is_rfc"] is True
    assert result["hex_len"] == 32


def assert_all_generated_uuids_are_unique(results: dict[str, Any]) -> None:
    generated = [
        results["parent"]["uuid"],
        *(child["uuid"] for child in results["children"]),
    ]

    assert len(generated) == len(set(generated)), generated


@pytest.mark.parametrize("install_in_parent", [False, True])
@pytest.mark.parametrize(
    "install_in_children",
    [
        (False, False),
        (False, True),
        (True, False),
        (True, True),
    ],
)
def test_spawn_parent_and_children_uuid_behavior_is_valid_and_unique(
    install_in_parent: bool,
    install_in_children: tuple[bool, bool],
) -> None:
    context = multiprocessing.get_context("spawn")

    results = run_parent_child_uuid_smoke(
        context,
        install_in_parent,
        install_in_children,
    )

    assert_uuid4_result_is_valid(results["parent"])

    for child in results["children"]:
        assert child["ok"] is True
        assert_uuid4_result_is_valid(child)

    assert_all_generated_uuids_are_unique(results)


@pytest.mark.skipif(
    "forkserver" not in multiprocessing.get_all_start_methods(),
    reason="forkserver multiprocessing start method is not available",
)
@pytest.mark.parametrize("install_in_parent", [False, True])
@pytest.mark.parametrize(
    "install_in_children",
    [
        (False, False),
        (False, True),
        (True, False),
        (True, True),
    ],
)
def test_forkserver_parent_and_children_uuid_behavior_is_valid_and_unique(
    install_in_parent: bool,
    install_in_children: tuple[bool, bool],
) -> None:
    context = multiprocessing.get_context("forkserver")

    results = run_parent_child_uuid_smoke(
        context,
        install_in_parent,
        install_in_children,
    )

    assert_uuid4_result_is_valid(results["parent"])

    for child in results["children"]:
        assert child["ok"] is True
        assert_uuid4_result_is_valid(child)

    assert_all_generated_uuids_are_unique(results)