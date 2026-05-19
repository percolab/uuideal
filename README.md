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
