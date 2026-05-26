<h1>
uuideal
<a href="https://pypi.python.org/pypi/uuideal">
  <img src="https://img.shields.io/pypi/v/uuideal.svg" alt="PyPI Version Badge">
</a>
<a href="https://pypi.python.org/pypi/uuideal">
  <img src="https://img.shields.io/pypi/l/uuideal.svg" alt="PyPI License Badge">
</a>
<a href="https://pypi.python.org/pypi/uuideal">
  <img src="https://img.shields.io/pypi/pyversions/uuideal.svg" alt="PyPI Python Versions Badge">
</a>
<a href="https://github.com/Bobronium/uuideal/actions">
  <img src="https://github.com/percolab/uuideal/actions/workflows/cd.yaml/badge.svg" alt="CD Status Badge">
</a>
</h1>

<div align="center"><h3>Makes Python <code>uuid</code> fast.</h3></div>
<div align="center"><h6><code>import uuideal</code>. <code>uuideal.install()</code>. That's it.</h6></div>


<div align="center">
  <picture>
    <source srcset="https://raw.githubusercontent.com/percolab/uuideal/refs/tags/v0.1.0/assets/chart_dark.svg" media="(prefers-color-scheme: dark)">
    <source srcset="https://raw.githubusercontent.com/percolab/uuideal/refs/tags/v0.1.0/assets/chart_light.svg" media="(prefers-color-scheme: light)">
    <img
      src="https://raw.githubusercontent.com/percolab/uuideal/refs/tags/v0.1.0/assets/chart_light.svg"
      height="300"
      alt="Terminal window with `uv run --with uuideal ipython`, running timeit: uuid.uuid4() 1.46 μs; after uuideal.install(): 64.7 ns. Example timing from one local run; full benchmark results below."
    >
  </picture>
<p></p>
</div>


<!-- Picture above shows a Terminal window with `uv run --with uuideal ipython` running:
```ipython
In [1]: import uuid
In [2]: %timeit uuid.uuid4()
1.46 μs ± 5.26 ns per loop (mean ± std. dev. of 7 runs, 1,000,000 loops each)
In [3]: import uuideal
In [4]: %timeit uuideal.uuid4()
64.7 ns ± 0.393 ns per loop (mean ± std. dev. of 7 runs, 10,000,000 loops each)
In [5]: uuideal.install()
In [6]: %timeit uuid.uuid4()
64.7 ns ± 0.108 ns per loop (mean ± std. dev. of 7 runs, 10,000,000 loops each)
```
-->

> [!CAUTION]
> `uuideal.install()` patches CPython global state (vectorcall slots). 
> Don't use it unless you know what you're doing.
> Do not call `uuideal.install()` from reusable packages implicitly; leave that decision to
> applications.

```bash
uv add uuideal
```

```python
import uuid
import uuideal


same_function = uuid.uuid4
same_class = uuid.UUID

uuideal.install()  # This makes uuid fast.

assert uuideal.installed()
assert uuid.uuid4 is same_function
assert type(uuid.uuid4()) is same_class
assert uuideal.uuid7().version == 7

uuideal.uninstall()  # This makes uuid slow.
```

UUID generation is mostly backed by the [`uuid`](https://crates.io/crates/uuid) Rust crate, with two
exceptions:

- On Python builds where `uuid.uuid1()` would generate [multiprocessing safe uuids (
  `UUID.is_safe`)](https://docs.python.org/3/library/uuid.html#uuid.SafeUUID), `uuideal` will
  preserve that behavior.
- `uuid.uuid8()` will always use `random.getrandbits()` instead of Rust CSPRNG to generate
  randomness. It will
  remain [cryptographically unsafe](https://docs.python.org/3/library/uuid.html#uuid.uuid8) and
  compatible with seeded `random` module.

`uuideal.uuid6()`, `uuideal.uuid7()` and `uuideal.uuid8()` are also available on Python 3.12 and
3.13.

## Comparison

| Feature                                                                             | stdlib | `fastuuid` | `uuid_utils.compat` | _`uuid_utils`_ | stdlib&nbsp;+&nbsp;`uuideal` |
|-------------------------------------------------------------------------------------|:------:|:----------:|:-------------------:|:--------------:|:----------------------------:|
| Uses exact `uuid.UUID`                                                              |   ✅    |     ✗      |          ✅          |       ✗        |              ✅               |
| No call-site changes                                                                |   ✅    |     ✗      |          ✗          |       ✗        |              ✅               |
| [safe `uuid1()` support](https://docs.python.org/3/library/uuid.html#uuid.SafeUUID) |   ✅    |     ✗      |          ✗          |       ✗        |              ✅               |
| Fast generation<sup>[\*](#uuideal-fast-definition)</sup>                            |   ✗    |     ✗      |          ✗          |       ✅        |              ✅               |
| Fast parsing<sup>[\*](#uuideal-fast-definition)</sup>                               |   ✗    |     ✅      |          ✗          |       ✅        |              ✅               |
| Beautiful codebase                                                                  |   ✗    |     ✅      |          ✅          |       ✅        |              ✗               |

_<span id="uuideal-fast-definition">*</span> At least 15x faster than stdlib for both `uuid4()`
and `uuid7()` and at least 3x faster for `UUID('<hex>')`._

[//]: # (@formatter:off)
<!-- uuideal-benchmarks:start -->

<!-- uuideal-benchmarks:generation:start -->

### Generation

| Operation | stdlib | `uuid_utils.compat` | _`uuid_utils`_ | stdlib&nbsp;+&nbsp;`uuideal` |
|---|---:|---:|---:|---:|
| `uuid1()` | 1,752 ns | 443.4 ns (3.95×) | **72.5 ns (24.18×)** | 74.1 ns (23.64×) |
| `uuid1()`<br>`safe` | 737.7 ns | N/A | N/A | **131.4 ns (5.62×)** |
| `uuid3()` | 1,007 ns | 669.7 ns (1.50×) | 204.6 ns (4.92×) | **176.9 ns (5.69×)** |
| `uuid4()` | 1,195 ns | 259.0 ns (4.62×) | **60.5 ns (19.76×)** | 62.4 ns (19.17×) |
| `uuid5()` | 993.7 ns | 653.7 ns (1.52×) | 192.5 ns (5.16×) | **167.3 ns (5.94×)** |
| `uuid6()` | 868.5 ns | 324.8 ns (2.67×) | **71.4 ns (12.17×)** | 73.3 ns (11.84×) |
| `uuid7()` | 1,445 ns | 300.9 ns (4.80×) | 94.8 ns (15.25×) | **86.6 ns (16.70×)** |
| `uuid8()` | 720.1 ns | N/A | N/A | **166.1 ns (4.33×)** |
| **Speedup (geomean)**<sup>[\*](#uuideal-benchmarks-geomean)</sup> | 1.00× | 2.85× | 11.45× | **12.03×** |

<!-- uuideal-benchmarks:generation:end -->

<!-- uuideal-benchmarks:conversions:start -->

### Conversions

| Operation | stdlib | _`uuid_utils`_ | stdlib&nbsp;+&nbsp;`uuideal` |
|---|---:|---:|---:|
| `UUID('<hex>')` | 492.8 ns | **99.4 ns (4.96×)** | 101.2 ns (4.87×) |
| `str(value)` | 351.4 ns | 93.0 ns (3.78×) | **58.7 ns (5.99×)** |
| `pickle.dumps(value)` | 1,134 ns | 1,189 ns (0.95×) | **1,083 ns (1.05×)** |
| `pickle.loads(payload)` | 829.7 ns | **653.9 ns (1.27×)** | 700.3 ns (1.18×) |
| **Speedup (geomean)**<sup>[\*](#uuideal-benchmarks-geomean)</sup> | 1.00× | 2.18× | **2.45×** |

<!-- uuideal-benchmarks:conversions:end -->

<!-- uuideal-benchmarks:access:start -->

### Access

| Operation | stdlib | _`uuid_utils`_ | stdlib&nbsp;+&nbsp;`uuideal` |
|---|---:|---:|---:|
| `value.int` | **40.3 ns** | 61.3 ns (0.66×) | 40.7 ns (0.99×) |
| `value.bytes` | 89.7 ns | 53.8 ns (1.67×) | **52.6 ns (1.71×)** |
| `value.bytes_le` | 324.5 ns | 55.2 ns (5.88×) | **52.6 ns (6.17×)** |
| `value.hex` | 132.2 ns | 95.2 ns (1.39×) | **62.0 ns (2.13×)** |
| `value.fields` | 334.4 ns | 101.8 ns (3.29×) | **98.3 ns (3.40×)** |
| `value.time_low` | 78.3 ns | 50.0 ns (1.57×) | **48.4 ns (1.62×)** |
| `value.time_mid` | 96.0 ns | 50.2 ns (1.91×) | **48.5 ns (1.98×)** |
| `value.time_hi_version` | 95.8 ns | 50.8 ns (1.88×) | **48.6 ns (1.97×)** |
| `value.clock_seq_hi_variant` | 96.1 ns | 46.2 ns (2.08×) | **44.5 ns (2.16×)** |
| `value.clock_seq_low` | 96.4 ns | 45.6 ns (2.11×) | **44.3 ns (2.18×)** |
| `value.node` | 78.9 ns | 54.3 ns (1.45×) | **53.2 ns (1.48×)** |
| `value.time` | 550.0 ns | 54.1 ns (10.16×) | **53.2 ns (10.34×)** |
| `value.clock_seq` | 181.5 ns | 50.1 ns (3.62×) | **48.8 ns (3.72×)** |
| `value.urn` | 413.0 ns | 101.4 ns (4.08×) | **64.2 ns (6.43×)** |
| `value.variant` | 115.1 ns | 59.9 ns (1.92×) | **43.6 ns (2.64×)** |
| `value.version` | 175.3 ns | 46.1 ns (3.80×) | **43.7 ns (4.01×)** |
| `value.is_safe` | **40.2 ns** | 94.3 ns (0.43×) | 40.3 ns (1.00×) |
| `sorted(values)` | 494,154 ns | 260,985 ns (1.89×) | **146,656 ns (3.37×)** |
| **Speedup (geomean)**<sup>[\*](#uuideal-benchmarks-geomean)</sup> | 1.00× | 2.15× | **2.60×** |

<!-- uuideal-benchmarks:access:end -->



_<span id="uuideal-benchmarks-geomean">*</span> Geomean uses only operation groups where every displayed candidate has valid timing data._

Ran on `Apple M1` `macOS 15.0.1` `CPython 3.14.5` best of `5` repeats after autoranging each case to at least `100ms` using 2 worker process(es), nice adjusted by `-20`, thread QoS set to `USER_INTERACTIVE`.

<!-- uuideal-benchmarks:end -->
[//]: # (@formatter:on)
