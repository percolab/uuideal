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
  <img src="https://github.com/Bobronium/uuideal/actions/workflows/cd.yaml/badge.svg" alt="CD Status Badge">
</a>
</h1>

<div align="center"><h3>Makes Python <code>uuid</code> fast.</h3></div>
<div align="center"><h6><code>import uuideal</code>. <code>uuideal.install()</code>. That's it.</h6></div>


<div align="center">
  <picture>
    <source srcset="assets/chart_dark.svg" media="(prefers-color-scheme: dark)">
    <source srcset="assets/chart_light.svg" media="(prefers-color-scheme: light)">
    <img
      src="assets/chart_light.svg"
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
In [3]: import uuideal; uuideal.install()
In [4]: %timeit uuid.uuid4()
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

| Operation | stdlib | `uuid_utils.compat` | _`uuid_utils`_ | stdlib + `uuideal` |
|---|---:|---:|---:|---:|
| `uuid1()` | 1,267 ns | 319.0 ns (3.97×) | 74.6 ns (16.97×) | **74.1 ns (17.09×)** |
| `uuid1()`<br>`safe` | 742.7 ns | N/A | N/A | **129.3 ns (5.74×)** |
| `uuid3()` | 984.3 ns | 665.0 ns (1.48×) | 202.3 ns (4.86×) | **175.0 ns (5.62×)** |
| `uuid4()` | 1,182 ns | 246.0 ns (4.80×) | **58.7 ns (20.13×)** | 60.9 ns (19.39×) |
| `uuid5()` | 977.3 ns | 646.9 ns (1.51×) | 189.8 ns (5.15×) | **164.3 ns (5.95×)** |
| `uuid6()` | 847.6 ns | 318.8 ns (2.66×) | **71.8 ns (11.80×)** | 72.4 ns (11.71×) |
| `uuid7()` | 1,445 ns | 294.5 ns (4.91×) | 96.4 ns (14.99×) | **84.5 ns (17.10×)** |
| `uuid8()` | 704.1 ns | N/A | N/A | **164.8 ns (4.27×)** |
| **Speedup (geomean)**<sup>[\*](#uuideal-benchmarks-geomean)</sup> | 1.00× | 2.87× | 10.72× | **11.42×** |

<!-- uuideal-benchmarks:generation:end -->

<!-- uuideal-benchmarks:conversions:start -->

### Conversions

| Operation | stdlib | _`uuid_utils`_ | stdlib + `uuideal` |
|---|---:|---:|---:|
| `UUID('<hex>')` | 484.8 ns | **97.6 ns (4.97×)** | 99.4 ns (4.88×) |
| `str(value)` | 351.0 ns | 98.9 ns (3.55×) | **59.8 ns (5.87×)** |
| `pickle.dumps(value)` | 1,127 ns | 1,163 ns (0.97×) | **1,077 ns (1.05×)** |
| `pickle.loads(payload)` | 810.2 ns | **591.9 ns (1.37×)** | 674.4 ns (1.20×) |
| **Speedup (geomean)**<sup>[\*](#uuideal-benchmarks-geomean)</sup> | 1.00× | 2.20× | **2.45×** |

<!-- uuideal-benchmarks:conversions:end -->

<!-- uuideal-benchmarks:access:start -->

### Access

| Operation | stdlib | _`uuid_utils`_ | stdlib + `uuideal` |
|---|---:|---:|---:|
| `value.int` | 41.5 ns | 64.9 ns (0.64×) | **41.4 ns (1.00×)** |
| `value.bytes` | 91.6 ns | 57.4 ns (1.60×) | **54.0 ns (1.70×)** |
| `value.bytes_le` | 331.4 ns | 55.3 ns (6.00×) | **53.9 ns (6.15×)** |
| `value.hex` | 130.2 ns | 98.2 ns (1.33×) | **63.9 ns (2.04×)** |
| `value.fields` | 335.8 ns | 103.5 ns (3.24×) | **99.9 ns (3.36×)** |
| `value.time_low` | 79.1 ns | 51.2 ns (1.54×) | **49.7 ns (1.59×)** |
| `value.time_mid` | 95.6 ns | 51.2 ns (1.87×) | **49.8 ns (1.92×)** |
| `value.time_hi_version` | 95.6 ns | 51.2 ns (1.87×) | **49.6 ns (1.93×)** |
| `value.clock_seq_hi_variant` | 95.7 ns | 46.9 ns (2.04×) | **45.5 ns (2.10×)** |
| `value.clock_seq_low` | 95.3 ns | 46.7 ns (2.04×) | **45.6 ns (2.09×)** |
| `value.node` | 81.5 ns | 55.4 ns (1.47×) | **54.5 ns (1.49×)** |
| `value.time` | 535.5 ns | 57.7 ns (9.27×) | **54.3 ns (9.87×)** |
| `value.clock_seq` | 180.8 ns | 51.2 ns (3.53×) | **49.7 ns (3.64×)** |
| `value.urn` | 409.9 ns | 103.0 ns (3.98×) | **66.2 ns (6.19×)** |
| `value.variant` | 114.8 ns | 63.5 ns (1.81×) | **44.9 ns (2.55×)** |
| `value.version` | 175.3 ns | 47.0 ns (3.73×) | **44.9 ns (3.91×)** |
| `value.is_safe` | 41.6 ns | 100.0 ns (0.42×) | **41.5 ns (1.00×)** |
| `sorted(values)` | 491,792 ns | 261,831 ns (1.88×) | **150,660 ns (3.26×)** |
| **Speedup (geomean)**<sup>[\*](#uuideal-benchmarks-geomean)</sup> | 1.00× | 2.09× | **2.55×** |

<!-- uuideal-benchmarks:access:end -->


Ran on `Apple M1` `macOS 15.0.1` `CPython 3.14.3` best of `5` repeats after autoranging each case to at least `100ms` using 2 worker process(es), nice adjusted by `-20`, thread QoS set to `USER_INTERACTIVE`.

_<span id="uuideal-benchmarks-geomean">*</span> Geomean uses only operation groups where every displayed candidate has valid timing data._

<!-- uuideal-benchmarks:end -->
[//]: # (@formatter:on)
