# uuideal

`uuideal` patches stdlib `uuid` function and `uuid.UUID` method vectorcall slots in place.

```python
import uuid
import uuideal

same_function = uuid.uuid4
uuideal.install()
assert uuid.uuid4 is same_function
assert type(uuid.uuid4()) is uuid.UUID
```

Use `uuideal.uninstall()` to restore the original vectorcall slots.

`uuideal.uuid6()` and `uuideal.uuid7()` are available as shortcuts even on Python versions whose stdlib `uuid` module does not expose those factories. On Python versions that do expose `uuid.uuid6` or `uuid.uuid7`, `install()` patches those functions in place too.

The patch runs under the GIL and does not release it. `uuid4()` uses a fast process-local random generator, so it is appropriate for identifiers but must not be treated as a security-token generator.
